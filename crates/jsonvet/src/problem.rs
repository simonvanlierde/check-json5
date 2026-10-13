use std::fmt;

use crate::{Dialect, Rule, Severity};

/// One problem in a file: what is wrong, where, and how much it matters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub(crate) offset: usize,
    pub(crate) severity: Severity,
    pub(crate) detail: Detail,
}

impl Problem {
    /// The byte offset in the text where the problem is. Always on a character boundary.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The problem's severity under the config it was checked with. Never `Off`.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// What is wrong.
    pub fn detail(&self) -> &Detail {
        &self.detail
    }

    /// The rule the problem belongs to.
    pub fn rule(&self) -> Rule {
        match self.detail {
            Detail::Syntax(_) => Rule::Syntax,
            Detail::DuplicateKey { .. } => Rule::DuplicateKey,
            Detail::InvalidNumber { .. } => Rule::InvalidNumber,
            Detail::InvalidCodePoint { .. } => Rule::InvalidCodePoint,
            Detail::ByteOrderMark => Rule::ByteOrderMark,
            Detail::MissingFinalNewline => Rule::MissingFinalNewline,
        }
    }

    /// A second offset the problem refers to: the first occurrence of a duplicate key, or
    /// where an unterminated string or comment starts.
    pub fn related_offset(&self) -> Option<usize> {
        match &self.detail {
            Detail::DuplicateKey { first, .. } => Some(*first),
            Detail::Syntax(s) => s.started,
            _ => None,
        }
    }
}

/// The message, without its position or rule id.
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.detail.fmt(f)
    }
}

impl std::error::Error for Problem {}

/// What a problem is about.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detail {
    /// A syntax error.
    Syntax(SyntaxError),
    /// A key repeated in the same object.
    DuplicateKey {
        /// The offset of the key's first occurrence.
        first: usize,
        /// The JSONPath of the key, such as `$.server.port`.
        path: String,
    },
    /// A number I-JSON does not allow.
    InvalidNumber {
        /// True when the number overflows a double; false for an integer past ±(2⁵³ − 1).
        overflows: bool,
    },
    /// A lone surrogate or a noncharacter in a string.
    InvalidCodePoint {
        /// The code point.
        code_point: u32,
    },
    /// A byte order mark at the start of the file.
    ByteOrderMark,
    /// A JSON Lines file that does not end with a newline.
    MissingFinalNewline,
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Detail::Syntax(s) => s.fmt(f),
            Detail::DuplicateKey { path, .. } => write!(f, "duplicate key {path}"),
            Detail::InvalidNumber { overflows: true } => f.write_str("number overflows a double"),
            Detail::InvalidNumber { overflows: false } => {
                f.write_str("integer beyond ±(2^53 − 1) cannot be represented exactly")
            }
            Detail::InvalidCodePoint { code_point } if (0xD800..0xE000).contains(code_point) => {
                write!(f, "lone surrogate U+{code_point:04X}")
            }
            Detail::InvalidCodePoint { code_point } => {
                write!(f, "noncharacter U+{code_point:04X}")
            }
            Detail::ByteOrderMark => f.write_str("byte order mark at the start of the file"),
            Detail::MissingFinalNewline => f.write_str("no newline at the end of the file"),
        }
    }
}

/// A syntax error: the first place where the text stops being valid in its dialect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    /// What went wrong.
    pub kind: SyntaxKind,
    /// Where the unterminated string or comment the error is about starts, if it is about one.
    pub started: Option<usize>,
}

/// The kinds of syntax error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntaxKind {
    /// The text ended where something else was needed.
    UnexpectedEnd {
        /// What was needed.
        expected: Expected,
    },
    /// A character that cannot continue the document here.
    Unexpected {
        /// The character found.
        found: char,
        /// What was needed instead.
        expected: Expected,
    },
    /// Syntax that a larger dialect allows but this one does not.
    NotAllowed {
        /// The syntax.
        feature: Feature,
        /// The dialect being checked.
        dialect: Dialect,
    },
    /// A trailing comma before `]` or `}` in a dialect without them.
    TrailingComma {
        /// The closing bracket.
        close: char,
        /// The dialect being checked.
        dialect: Dialect,
    },
    /// A string that runs to the end of the text.
    UnterminatedString,
    /// A raw line break inside a string.
    LineBreakInString,
    /// A raw control character inside a string, which this dialect requires to be escaped.
    ControlCharacter {
        /// The character.
        found: char,
    },
    /// A backslash escape this dialect does not define.
    InvalidEscape {
        /// The character after the backslash.
        found: char,
    },
    /// `\u` without four hex digits after it.
    InvalidUnicodeEscape,
    /// `\x` without two hex digits after it.
    InvalidHexEscape,
    /// A block comment without its closing `*/`.
    UnterminatedComment,
    /// An empty line in a JSON Lines file.
    BlankLine,
}

/// What the parser needed where it found something else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Expected {
    /// Any value.
    Value,
    /// An object key.
    Key,
    /// `:` after a key.
    Colon,
    /// `,` or `]` after an array element.
    CommaOrBracket,
    /// `,` or `}` after an object member.
    CommaOrBrace,
    /// Nothing more: the document's value is complete.
    End,
    /// The end of the line, in JSON Lines.
    LineEnd,
    /// A decimal digit.
    Digit,
    /// A hexadecimal digit.
    HexDigit,
    /// The rest of a literal such as `null`.
    Literal(&'static str),
}

/// Syntax that only some dialects allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Feature {
    /// `//` and `/* */` comments.
    Comments,
    /// Strings in single quotes.
    SingleQuotes,
    /// Object keys without quotes.
    UnquotedKeys,
    /// `NaN` and `Infinity`.
    NanAndInfinity,
    /// Hex numbers, a leading `+`, and a leading or trailing decimal point.
    Json5Numbers,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SyntaxKind::UnexpectedEnd { expected } => {
                write!(f, "expected {expected}, found the end of the file")
            }
            SyntaxKind::Unexpected { found, expected } => {
                write!(f, "expected {expected}, found {}", Shown(*found))
            }
            SyntaxKind::NotAllowed { feature, dialect } => {
                let what = match feature {
                    Feature::Comments => "comments are",
                    Feature::SingleQuotes => "single-quoted strings are",
                    Feature::UnquotedKeys => "unquoted keys are",
                    Feature::NanAndInfinity => "NaN and Infinity are",
                    Feature::Json5Numbers => "this number form is",
                };
                write!(f, "{what} not allowed in {dialect}")
            }
            SyntaxKind::TrailingComma { close, dialect } => {
                write!(
                    f,
                    "trailing comma before `{close}` is not allowed in {dialect}"
                )
            }
            SyntaxKind::UnterminatedString => f.write_str("unterminated string"),
            SyntaxKind::LineBreakInString => f.write_str("line break inside a string"),
            SyntaxKind::ControlCharacter { found } => {
                write!(
                    f,
                    "control character {} inside a string must be escaped",
                    Shown(*found)
                )
            }
            SyntaxKind::InvalidEscape { found } => {
                write!(f, "invalid escape `\\{}`", found.escape_debug())
            }
            SyntaxKind::InvalidUnicodeEscape => f.write_str("`\\u` needs four hex digits"),
            SyntaxKind::InvalidHexEscape => f.write_str("`\\x` needs two hex digits"),
            SyntaxKind::UnterminatedComment => f.write_str("unterminated block comment"),
            SyntaxKind::BlankLine => {
                f.write_str("blank line; JSON Lines needs a value on every line")
            }
        }
    }
}

impl std::error::Error for SyntaxError {}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expected::Value => f.write_str("a value"),
            Expected::Key => f.write_str("a key"),
            Expected::Colon => f.write_str("`:`"),
            Expected::CommaOrBracket => f.write_str("`,` or `]`"),
            Expected::CommaOrBrace => f.write_str("`,` or `}`"),
            Expected::End => f.write_str("the end of the file"),
            Expected::LineEnd => f.write_str("the end of the line"),
            Expected::Digit => f.write_str("a digit"),
            Expected::HexDigit => f.write_str("a hex digit"),
            Expected::Literal(word) => write!(f, "`{word}`"),
        }
    }
}

/// A character as a message shows it: in backticks when printable, as U+XXXX otherwise.
struct Shown(char);

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = self.0;
        if c.is_control() || c.is_whitespace() || c == '\u{FEFF}' {
            write!(f, "U+{:04X}", u32::from(c))
        } else {
            write!(f, "`{c}`")
        }
    }
}
