use std::cell::OnceCell;

use crate::{Problem, Severity};

/// The problems found in one text, in document order.
#[derive(Debug)]
pub struct Report<'a> {
    text: &'a str,
    problems: Vec<Problem>,
    line_starts: OnceCell<Vec<usize>>,
}

/// A 1-based line and column. Lines end at LF, CR, CRLF, U+2028 and U+2029, as JSON5 defines
/// them; columns count Unicode code points from the start of the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Location {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
}

impl<'a> Report<'a> {
    pub(crate) fn new(text: &'a str, mut problems: Vec<Problem>) -> Self {
        // Stable, so problems at the same offset keep the order they were found in.
        problems.sort_by_key(Problem::offset);
        Report {
            text,
            problems,
            line_starts: OnceCell::new(),
        }
    }

    /// Every problem, ordered by offset.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// True when no problem is an error.
    pub fn passed(&self) -> bool {
        self.problems.iter().all(|p| p.severity() < Severity::Error)
    }

    /// The line and column of a byte offset in the text. The first call finds the line starts;
    /// later calls cost a binary search and a count of the characters before the offset on its
    /// line. An offset past the end is treated as the end.
    pub fn location(&self, offset: usize) -> Location {
        let offset = offset.min(self.text.len());
        let starts = self.line_starts.get_or_init(|| line_starts(self.text));
        let line = starts.partition_point(|&s| s <= offset);
        let start = starts[line - 1];
        let column = self
            .text
            .get(start..offset)
            .map_or(1, |s| s.chars().count() + 1);
        Location { line, column }
    }
}

/// The byte offset where each line starts, the first being 0.
fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    let mut i = 0;
    while let Some(found) = memchr::memchr3(b'\n', b'\r', 0xE2, &bytes[i..]) {
        let at = i + found;
        match bytes[at] {
            b'\r' if bytes.get(at + 1) == Some(&b'\n') => {
                starts.push(at + 2);
                i = at + 2;
                continue;
            }
            b'\n' | b'\r' => starts.push(at + 1),
            // U+2028 and U+2029 are E2 80 A8 and E2 80 A9 in UTF-8.
            _ if bytes.get(at + 1) == Some(&0x80)
                && matches!(bytes.get(at + 2), Some(0xA8 | 0xA9)) =>
            {
                starts.push(at + 3);
            }
            _ => {}
        }
        i = at + 1;
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::line_starts;

    #[test]
    fn every_json5_line_terminator_starts_a_line() {
        assert_eq!(
            line_starts("a\nb\r\nc\rd\u{2028}e\u{2029}f"),
            vec![0, 2, 5, 7, 11, 15]
        );
        assert_eq!(line_starts("no breaks, but é and ★"), vec![0]);
    }
}
