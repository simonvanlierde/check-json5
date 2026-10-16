//! How problems are printed: `file:line:col: severity[rule] message`, plus a GitHub Actions
//! annotation for each when running there, or one SARIF document for the whole run.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, Write};

use jsonvet::{Location, Problem, RelatedKind, Report, Severity};

use crate::sarif;

/// The output format, from `--format`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Format {
    #[default]
    Text,
    Sarif,
}

/// A file name or message with control characters made visible: a newline would start a line
/// of its own, which GitHub Actions reads as a workflow command when it starts with `::`, and
/// an escape character would reach the terminal. Bidi controls, which reorder how the rest of
/// the line displays, show the same way. CR and LF show as `\r` and `\n`, the rest as
/// `\u001b`, as JSONPaths in messages show them. The runner also reads its older `##[command]`
/// form anywhere in a line, so the `[` of `##[` shows as `\u005b`.
pub(crate) fn printable(value: &str) -> String {
    let mut shown = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\r' => shown.push_str("\\r"),
            '\n' => shown.push_str("\\n"),
            c if c.is_control() || is_bidi_control(c) => {
                let _ = write!(shown, "\\u{:04x}", u32::from(c));
            }
            '[' if shown.ends_with("##") => shown.push_str("\\u005b"),
            c => shown.push(c),
        }
    }
    shown
}

/// Unicode's `Bidi_Control` characters: the marks and the embedding, override and isolate
/// controls. NOTE: the same list as `is_bidi_control` in jsonvet, which is not public.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

/// A file name as it starts an output line. A name starting with `::`, after any leading
/// white space as the runner trims it, gets `./` in front, so the line is not a workflow
/// command.
fn line_start(file: &str) -> String {
    let shown = printable(file);
    if shown.trim_start().starts_with("::") {
        format!("./{shown}")
    } else {
        shown
    }
}

/// Escape a value for a GitHub workflow command: `%`, CR and LF always, `:` and `,` in
/// properties such as `file=`.
fn escape(value: &str, property: bool) -> String {
    let value = value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    if property {
        value.replace(':', "%3A").replace(',', "%2C")
    } else {
        value
    }
}

pub(crate) struct Printer<W: Write> {
    out: W,
    annotate: bool,
    /// SARIF results, held until `finish` writes the document; `None` for text output.
    sarif: Option<Vec<String>>,
}

impl<W: Write> Printer<W> {
    pub(crate) fn new(out: W, format: Format, annotate: bool) -> Self {
        let sarif = format == Format::Sarif;
        Printer {
            out,
            annotate: annotate && !sarif,
            sarif: sarif.then(Vec::new),
        }
    }

    /// A problem in a file's contents.
    pub(crate) fn problem(
        &mut self,
        file: &OsStr,
        report: &Report<'_>,
        problem: &Problem,
    ) -> io::Result<()> {
        let at = report.location(problem.offset());
        // Where the related offset is, and its labels in the text message and in SARIF.
        let related = problem.related().map(|related| {
            let labels = match related.kind {
                RelatedKind::FirstOccurrence => ("first at", "first copy"),
                // NOTE: `RelatedKind` is non-exhaustive; `Start` is the only other kind so far.
                _ => ("started at", "starts here"),
            };
            (report.location(related.offset), labels)
        });
        let mut message = problem.to_string();
        if let Some((first, (what, _))) = related {
            let _ = write!(message, " ({what} {}:{})", first.line, first.column);
        }
        let severity = problem.severity();
        let rule = problem.rule();
        if let Some(results) = &mut self.sarif {
            let related = related.map(|(first, (_, what))| (first, what));
            results.push(sarif::result(
                file,
                rule.id(),
                severity,
                &message,
                at,
                related,
            ));
            return Ok(());
        }
        let file = &file.to_string_lossy();
        writeln!(
            self.out,
            "{}:{}:{}: {severity}[{rule}] {}",
            line_start(file),
            at.line,
            at.column,
            printable(&message)
        )?;
        if self.annotate {
            writeln!(
                self.out,
                "::{} file={},line={},col={},title=jsonvet ({rule})::{}",
                sarif::level(severity),
                escape(&printable(file), true),
                at.line,
                at.column,
                escape(&printable(&message), false)
            )?;
        }
        Ok(())
    }

    /// A file that could not be checked at all.
    pub(crate) fn unreadable(&mut self, file: &OsStr, message: &str) -> io::Result<()> {
        if let Some(results) = &mut self.sarif {
            // GitHub code scanning needs a line in every result.
            let start = Location::START;
            results.push(sarif::result(
                file,
                sarif::UNREADABLE,
                Severity::Error,
                message,
                start,
                None,
            ));
            return Ok(());
        }
        let file = &file.to_string_lossy();
        writeln!(
            self.out,
            "{}: error: {}",
            line_start(file),
            printable(message)
        )?;
        if self.annotate {
            writeln!(
                self.out,
                "::error file={},title=jsonvet::{}",
                escape(&printable(file), true),
                escape(&printable(message), false)
            )?;
        }
        Ok(())
    }

    /// Write what was held back until every file was checked: the SARIF document.
    pub(crate) fn finish(&mut self) -> io::Result<()> {
        if let Some(results) = &self.sarif {
            writeln!(self.out, "{}", sarif::document(results))?;
        }
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{escape, line_start, printable};

    #[test]
    fn messages_stay_on_one_line_and_annotation_fields_are_percent_encoded() {
        assert_eq!(printable("a\n::warning::x"), "a\\n::warning::x");
        assert_eq!(
            printable("a\u{1b}]8;;x\u{7}\u{9b}b"),
            "a\\u001b]8;;x\\u0007\\u009bb"
        );
        assert_eq!(printable("a##[add-mask]b#[c"), "a##\\u005badd-mask]b#[c");
        assert_eq!(printable("C:\\x\te.json"), "C:\\x\\u0009e.json");
        assert_eq!(
            printable("a\u{202e}b\u{2066}c\u{200f}d\u{61c}e\u{200d}f"),
            "a\\u202eb\\u2066c\\u200fd\\u061ce\u{200d}f"
        );
        assert_eq!(escape("50% a\r\nb", false), "50%25 a%0D%0Ab");
        assert_eq!(escape("a:b,c", true), "a%3Ab%2Cc");
        assert_eq!(escape("not strict: x, y", false), "not strict: x, y");
    }

    #[test]
    fn a_name_never_starts_a_workflow_command() {
        assert_eq!(
            line_start("::stop-commands::t.json"),
            "./::stop-commands::t.json"
        );
        assert_eq!(
            line_start("\u{3000} ::add-mask::x"),
            "./\u{3000} ::add-mask::x"
        );
        assert_eq!(line_start("\n::error::x"), "\\n::error::x");
        assert_eq!(line_start("a::b.json"), "a::b.json");
    }
}
