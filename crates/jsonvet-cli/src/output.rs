//! How problems are printed: `file:line:col: severity[rule] message`, plus a GitHub Actions
//! annotation for each when running there, or one SARIF document for the whole run.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, Write};

use jsonvet::{Location, Problem, Report, Rule, Severity};

use crate::sarif;

/// The output format, from `--format`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Format {
    #[default]
    Text,
    Sarif,
}

/// A newline in a file name or message would start a line of its own, which GitHub Actions
/// reads as a workflow command when it starts with `::`, and other control characters such as
/// ESC can drive the terminal. Each is written as an escape, as duplicate-key paths write them.
pub(crate) fn one_line(value: &str) -> String {
    let mut line = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\n' => line.push_str("\\n"),
            '\r' => line.push_str("\\r"),
            '\t' => line.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(line, "\\u{:04x}", u32::from(c));
            }
            c => line.push(c),
        }
    }
    line
}

/// Escape a value for a GitHub workflow command: `%`, CR and LF always, `:` and `,` in
/// properties such as `file=`, and other control characters as [`one_line`] does.
fn escape(value: &str, property: bool) -> String {
    let value = one_line(
        &value
            .replace('%', "%25")
            .replace('\r', "%0D")
            .replace('\n', "%0A"),
    );
    if property {
        value.replace(':', "%3A").replace(',', "%2C")
    } else {
        value
    }
}

pub(crate) struct Printer<W: Write> {
    pub(crate) out: W,
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
        let mut message = problem.to_string();
        if let Some(related) = problem.related_offset() {
            let first = report.location(related);
            let what = if problem.rule() == Rule::DuplicateKey {
                "first at"
            } else {
                "started at"
            };
            let _ = write!(message, " ({what} {}:{})", first.line, first.column);
        }
        let rule = problem.rule();
        let related = problem.related_offset().map(|offset| {
            let what = if rule == Rule::DuplicateKey {
                "first copy"
            } else {
                "starts here"
            };
            (report.location(offset), what)
        });
        self.line(file, rule.id(), problem.severity(), &message, at, related)
    }

    /// A value the `--schema` schema rejects, at `offset`.
    pub(crate) fn schema_error(
        &mut self,
        file: &OsStr,
        report: &Report<'_>,
        offset: usize,
        message: &str,
    ) -> io::Result<()> {
        let at = report.location(offset);
        self.line(file, sarif::SCHEMA, Severity::Error, message, at, None)
    }

    /// One problem, in whichever format: a text line and maybe an annotation, or a SARIF
    /// result. `related` is only for SARIF; the text message already names it.
    fn line(
        &mut self,
        file: &OsStr,
        rule: &str,
        severity: Severity,
        message: &str,
        at: Location,
        related: Option<(Location, &str)>,
    ) -> io::Result<()> {
        if let Some(results) = &mut self.sarif {
            results.push(sarif::result(file, rule, severity, message, at, related));
            return Ok(());
        }
        let file = &file.to_string_lossy();
        writeln!(
            self.out,
            "{}:{}:{}: {severity}[{rule}] {}",
            one_line(file),
            at.line,
            at.column,
            one_line(message)
        )?;
        if self.annotate {
            writeln!(
                self.out,
                "::{} file={},line={},col={},title=jsonvet ({rule})::{}",
                annotation_level(severity),
                escape(file, true),
                at.line,
                at.column,
                escape(message, false)
            )?;
        }
        Ok(())
    }

    /// A file that could not be checked at all.
    pub(crate) fn unreadable(&mut self, file: &OsStr, message: &str) -> io::Result<()> {
        if let Some(results) = &mut self.sarif {
            // GitHub code scanning needs a line in every result.
            let start = Location { line: 1, column: 1 };
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
        writeln!(self.out, "{}: error: {}", one_line(file), one_line(message))?;
        if self.annotate {
            writeln!(
                self.out,
                "::error file={},title=jsonvet::{}",
                escape(file, true),
                escape(message, false)
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

fn annotation_level(severity: Severity) -> &'static str {
    if severity == Severity::Error {
        "error"
    } else {
        "warning"
    }
}

#[cfg(test)]
mod tests {
    use super::{escape, one_line};

    #[test]
    fn messages_stay_on_one_line_and_annotation_fields_are_percent_encoded() {
        assert_eq!(one_line("a\n::warning::x"), "a\\n::warning::x");
        assert_eq!(
            one_line("a\u{1b}[31m\u{7}\t\u{9b}"),
            "a\\u001b[31m\\u0007\\t\\u009b"
        );
        assert_eq!(escape("a\u{1b}]0;x\n", false), "a\\u001b]0;x%0A");
        assert_eq!(escape("50% a\r\nb", false), "50%25 a%0D%0Ab");
        assert_eq!(escape("a:b,c", true), "a%3Ab%2Cc");
        assert_eq!(escape("not strict: x, y", false), "not strict: x, y");
    }
}
