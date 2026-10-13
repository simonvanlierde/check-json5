//! How problems are printed: `file:line:col: severity[rule] message`, plus a GitHub Actions
//! annotation for each when running there.

use std::fmt::Write as _;
use std::io::{self, Write};

use jsonvet::{Detail, Problem, Report, Severity};

/// A newline in a file name or message would start a line of its own, which GitHub Actions
/// reads as a workflow command when it starts with `::`.
pub(crate) fn one_line(value: &str) -> String {
    value.replace('\r', "\\r").replace('\n', "\\n")
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
    pub(crate) out: W,
    pub(crate) annotate: bool,
}

impl<W: Write> Printer<W> {
    /// A problem in a file's contents.
    pub(crate) fn problem(
        &mut self,
        file: &str,
        report: &Report<'_>,
        problem: &Problem,
    ) -> io::Result<()> {
        let at = report.location(problem.offset());
        let mut message = problem.to_string();
        if let Some(related) = problem.related_offset() {
            let first = report.location(related);
            let what = if matches!(problem.detail(), Detail::DuplicateKey { .. }) {
                "first at"
            } else {
                "started at"
            };
            let _ = write!(message, " ({what} {}:{})", first.line, first.column);
        }
        let severity = problem.severity();
        let rule = problem.rule();
        writeln!(
            self.out,
            "{}:{}:{}: {severity}[{rule}] {}",
            one_line(file),
            at.line,
            at.column,
            one_line(&message)
        )?;
        if self.annotate {
            writeln!(
                self.out,
                "::{} file={},line={},col={},title=jsonvet ({rule})::{}",
                annotation_level(severity),
                escape(file, true),
                at.line,
                at.column,
                escape(&message, false)
            )?;
        }
        Ok(())
    }

    /// A file that could not be checked at all.
    pub(crate) fn unreadable(&mut self, file: &str, message: &str) -> io::Result<()> {
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
    fn escaping() {
        assert_eq!(one_line("a\n::warning::x"), "a\\n::warning::x");
        assert_eq!(escape("50% a\r\nb", false), "50%25 a%0D%0Ab");
        assert_eq!(escape("a:b,c", true), "a%3Ab%2Cc");
        assert_eq!(escape("not strict: x, y", false), "not strict: x, y");
    }
}
