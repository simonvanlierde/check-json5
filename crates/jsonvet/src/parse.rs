//! The parser: one pass over the bytes, an explicit stack of open containers, and every
//! dialect rule applied where the syntax it governs appears.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::path::{self, Segment};
use crate::problem::{Detail, Expected, Feature, SyntaxError, SyntaxKind};
use crate::{Config, Dialect, Problem, Rule, Severity};

/// Past this many keys, an object's duplicate check switches from a linear scan to a hash map.
const LINEAR_KEYS: usize = 16;

/// The largest integer a double holds exactly: 2⁵³ − 1.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

pub(crate) fn check(text: &str, config: &Config) -> Vec<Problem> {
    let dialect = config.dialect();
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
        dialect,
        json5: dialect.json5(),
        lines: dialect == Dialect::JsonLines,
        duplicates: config.severity(Rule::DuplicateKey),
        numbers: config.severity(Rule::InvalidNumber),
        code_points: config.severity(Rule::InvalidCodePoint),
        problems: Vec::new(),
        stack: Vec::new(),
    };
    parser.byte_order_mark(config.severity(Rule::ByteOrderMark));
    let outcome = if parser.lines {
        parser.json_lines(config.severity(Rule::MissingFinalNewline))
    } else {
        parser.document()
    };
    if let Err(fail) = outcome {
        parser.problems.push(Problem {
            offset: fail.offset,
            severity: Severity::Error,
            detail: Detail::Syntax(SyntaxError {
                kind: fail.kind,
                started: fail.started,
            }),
        });
    }
    parser.problems
}

/// A syntax error on its way out of the parser.
struct Fail {
    offset: usize,
    kind: SyntaxKind,
    started: Option<usize>,
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    dialect: Dialect,
    json5: bool,
    /// JSON Lines: a newline ends the document instead of being whitespace.
    lines: bool,
    duplicates: Severity,
    numbers: Severity,
    code_points: Severity,
    problems: Vec<Problem>,
    stack: Vec<Frame<'a>>,
}

/// An open object or array.
struct Frame<'a> {
    object: bool,
    /// How this container is reached from its parent.
    segment: Segment<'a>,
    /// The keys seen so far with the offset of each, for objects.
    keys: Vec<(Cow<'a, str>, usize)>,
    /// The same keys, indexed once the object grows past `LINEAR_KEYS`.
    index: Option<HashMap<Cow<'a, str>, usize>>,
    /// The current member's key, in an object.
    key: Cow<'a, str>,
    /// The current element's position, in an array.
    element: usize,
}

impl<'a> Frame<'a> {
    fn first_offset(&self, key: &str) -> Option<usize> {
        match &self.index {
            Some(index) => index.get(key).copied(),
            None => self.keys.iter().find(|(k, _)| k == key).map(|(_, at)| *at),
        }
    }

    fn insert(&mut self, key: Cow<'a, str>, at: usize) {
        if let Some(index) = &mut self.index {
            index.insert(key, at);
            return;
        }
        self.keys.push((key, at));
        if self.keys.len() > LINEAR_KEYS {
            self.index = Some(self.keys.drain(..).collect());
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// A value must come next.
    Value,
    /// Just after `[`: a value or `]`.
    ArrayFirst,
    /// Just after `,` in an array: a value, or `]` where trailing commas are allowed.
    ArrayNext,
    /// Just after `{`: a key or `}`.
    ObjectFirst,
    /// Just after `,` in an object: a key, or `}` where trailing commas are allowed.
    ObjectNext,
    /// After a key: `:`.
    Colon,
    /// After a value inside a container: `,` or the closing bracket.
    AfterValue,
    /// The document's value is complete.
    Done,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn char_at(&self, at: usize) -> char {
        self.text
            .get(at..)
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{FFFD}')
    }

    fn fail(&self, kind: SyntaxKind) -> Fail {
        Fail {
            offset: self.pos,
            kind,
            started: None,
        }
    }

    /// The error for whatever sits at the current position when `expected` was needed.
    fn unexpected(&self, expected: Expected) -> Fail {
        if self.pos >= self.bytes.len() {
            self.fail(SyntaxKind::UnexpectedEnd { expected })
        } else {
            self.fail(SyntaxKind::Unexpected {
                found: self.char_at(self.pos),
                expected,
            })
        }
    }

    fn not_allowed(&self, at: usize, feature: Feature) -> Fail {
        Fail {
            offset: at,
            kind: SyntaxKind::NotAllowed {
                feature,
                dialect: self.dialect,
            },
            started: None,
        }
    }

    fn report(&mut self, offset: usize, severity: Severity, detail: Detail) {
        if severity > Severity::Off {
            self.problems.push(Problem {
                offset,
                severity,
                detail,
            });
        }
    }

    fn byte_order_mark(&mut self, severity: Severity) {
        if self.text.starts_with('\u{FEFF}') {
            self.report(0, severity, Detail::ByteOrderMark);
            self.pos = '\u{FEFF}'.len_utf8();
        }
    }

    // --- documents -------------------------------------------------------------------------

    fn document(&mut self) -> Result<(), Fail> {
        self.stack.clear();
        let mut state = State::Value;
        loop {
            self.trivia()?;
            state = match state {
                State::ArrayFirst | State::ArrayNext if self.peek() == Some(b']') => {
                    self.close(']', state == State::ArrayNext)?
                }
                State::Value | State::ArrayFirst | State::ArrayNext => self.value()?,
                State::ObjectFirst | State::ObjectNext if self.peek() == Some(b'}') => {
                    self.close('}', state == State::ObjectNext)?
                }
                State::ObjectFirst | State::ObjectNext => {
                    self.key()?;
                    State::Colon
                }
                State::Colon if self.peek() == Some(b':') => {
                    self.pos += 1;
                    State::Value
                }
                State::Colon => return Err(self.unexpected(Expected::Colon)),
                State::AfterValue => self.after_value()?,
                State::Done => {
                    let end = match self.peek() {
                        None => true,
                        Some(b'\n') => self.lines,
                        Some(_) => false,
                    };
                    if end {
                        return Ok(());
                    }
                    return Err(self.unexpected(if self.lines {
                        Expected::LineEnd
                    } else {
                        Expected::End
                    }));
                }
            };
        }
    }

    /// The state after a complete value.
    fn completed(&self) -> State {
        if self.stack.is_empty() {
            State::Done
        } else {
            State::AfterValue
        }
    }

    fn after_value(&mut self) -> Result<State, Fail> {
        let object = self.stack.last().is_some_and(|f| f.object);
        match self.peek() {
            Some(b',') => {
                self.pos += 1;
                if let Some(frame) = self.stack.last_mut() {
                    frame.element += 1;
                }
                Ok(if object {
                    State::ObjectNext
                } else {
                    State::ArrayNext
                })
            }
            Some(b'}') if object => self.close('}', false),
            Some(b']') if !object => self.close(']', false),
            _ => Err(self.unexpected(if object {
                Expected::CommaOrBrace
            } else {
                Expected::CommaOrBracket
            })),
        }
    }

    fn close(&mut self, close: char, after_comma: bool) -> Result<State, Fail> {
        if after_comma && !self.dialect.trailing_commas() {
            return Err(self.fail(SyntaxKind::TrailingComma {
                close,
                dialect: self.dialect,
            }));
        }
        self.pos += 1;
        self.stack.pop();
        Ok(self.completed())
    }

    fn open(&mut self, object: bool) -> State {
        let segment = match self.stack.last() {
            None => Segment::Root,
            Some(parent) if parent.object => Segment::Key(parent.key.clone()),
            Some(parent) => Segment::Index(parent.element),
        };
        self.stack.push(Frame {
            object,
            segment,
            keys: Vec::new(),
            index: None,
            key: Cow::Borrowed(""),
            element: 0,
        });
        self.pos += 1;
        if object {
            State::ObjectFirst
        } else {
            State::ArrayFirst
        }
    }

    fn value(&mut self) -> Result<State, Fail> {
        let Some(b) = self.peek() else {
            return Err(self.unexpected(Expected::Value));
        };
        match b {
            b'{' => return Ok(self.open(true)),
            b'[' => return Ok(self.open(false)),
            b'"' => {
                self.string(b'"', false)?;
            }
            b'\'' if self.json5 => {
                self.string(b'\'', false)?;
            }
            b'\'' => return Err(self.not_allowed(self.pos, Feature::SingleQuotes)),
            b'-' | b'+' | b'.' | b'0'..=b'9' | b'I' | b'N' => self.number()?,
            b't' => self.literal("true")?,
            b'f' => self.literal("false")?,
            b'n' => self.literal("null")?,
            _ => return Err(self.unexpected(Expected::Value)),
        }
        Ok(self.completed())
    }

    fn json_lines(&mut self, final_newline: Severity) -> Result<(), Fail> {
        let len = self.bytes.len();
        while self.pos < len {
            let line_start = self.pos;
            self.trivia()?;
            match self.peek() {
                None | Some(b'\n') => {
                    return Err(Fail {
                        offset: line_start,
                        kind: SyntaxKind::BlankLine,
                        started: None,
                    });
                }
                Some(_) => self.document()?,
            }
            if self.peek() == Some(b'\n') {
                self.pos += 1;
            } else {
                self.report(len, final_newline, Detail::MissingFinalNewline);
            }
        }
        Ok(())
    }

    // --- keys and duplicates ---------------------------------------------------------------

    fn key(&mut self) -> Result<(), Fail> {
        let start = self.pos;
        let decode = self.duplicates > Severity::Off;
        let key = match self.peek() {
            Some(b'"') => self.string(b'"', decode)?,
            Some(b'\'') if self.json5 => self.string(b'\'', decode)?,
            Some(b'\'') => return Err(self.not_allowed(start, Feature::SingleQuotes)),
            Some(_) if self.identifier_start(start) => {
                if !self.json5 {
                    return Err(self.not_allowed(start, Feature::UnquotedKeys));
                }
                self.identifier()?
            }
            _ => return Err(self.unexpected(Expected::Key)),
        };
        if !decode {
            if let Some(frame) = self.stack.last_mut() {
                frame.key = key;
            }
            return Ok(());
        }
        let first = self.stack.last().and_then(|f| f.first_offset(&key));
        if let Some(first) = first {
            let mut path = String::new();
            for frame in &self.stack {
                path::push(&mut path, &frame.segment);
            }
            path::push_key(&mut path, &key);
            self.report(start, self.duplicates, Detail::DuplicateKey { first, path });
        } else if let Some(frame) = self.stack.last_mut() {
            frame.insert(key.clone(), start);
        }
        if let Some(frame) = self.stack.last_mut() {
            frame.key = key;
        }
        Ok(())
    }

    fn identifier_start(&self, at: usize) -> bool {
        let c = self.char_at(at);
        c == '\\' || is_identifier_start(c)
    }

    /// A JSON5 identifier key, decoded.
    fn identifier(&mut self) -> Result<Cow<'a, str>, Fail> {
        let start = self.pos;
        let mut decoded: Option<String> = None;
        let mut first = true;
        loop {
            let c = self.char_at(self.pos);
            if self.pos >= self.bytes.len() {
                break;
            }
            let (c, len) = if c == '\\' {
                if self.bytes.get(self.pos + 1) != Some(&b'u') {
                    return Err(self.fail(SyntaxKind::InvalidEscape {
                        found: self.char_at(self.pos + 1),
                    }));
                }
                let Some(unit) = self.hex(self.pos + 2, 4) else {
                    return Err(self.fail(SyntaxKind::InvalidUnicodeEscape));
                };
                let c = char::from_u32(unit).unwrap_or('\u{FFFD}');
                let buffer = decoded.get_or_insert_with(|| self.text[start..self.pos].to_owned());
                buffer.push(c);
                (c, 6)
            } else {
                if let Some(buffer) = &mut decoded {
                    buffer.push(c);
                }
                (c, c.len_utf8())
            };
            let fits = if first {
                is_identifier_start(c)
            } else {
                is_identifier_part(c)
            };
            if !fits {
                if first {
                    return Err(self.unexpected(Expected::Key));
                }
                if let Some(buffer) = &mut decoded {
                    buffer.pop();
                }
                break;
            }
            first = false;
            self.pos += len;
        }
        Ok(match decoded {
            Some(buffer) => Cow::Owned(buffer),
            None => Cow::Borrowed(&self.text[start..self.pos]),
        })
    }

    // --- strings ---------------------------------------------------------------------------

    /// Scan a string starting at its quote. With `decode`, return its value; otherwise the
    /// returned value is empty.
    fn string(&mut self, quote: u8, decode: bool) -> Result<Cow<'a, str>, Fail> {
        let start = self.pos;
        self.pos += 1;
        let mut decoded: Option<String> = None;
        let mut run_start = self.pos;
        loop {
            let Some(b) = self.peek() else {
                return Err(Fail {
                    offset: self.pos,
                    kind: SyntaxKind::UnterminatedString,
                    started: Some(start),
                });
            };
            match b {
                _ if b == quote => {
                    let value = match decoded {
                        Some(mut buffer) => {
                            buffer.push_str(&self.text[run_start..self.pos]);
                            Cow::Owned(buffer)
                        }
                        None if decode => Cow::Borrowed(&self.text[start + 1..self.pos]),
                        None => Cow::Borrowed(""),
                    };
                    self.pos += 1;
                    return Ok(value);
                }
                b'\\' => {
                    let buffer = decode.then(|| {
                        let buffer = decoded.get_or_insert_with(String::new);
                        buffer.push_str(&self.text[run_start..self.pos]);
                        buffer
                    });
                    let c = self.escape(start)?;
                    if let (Some(buffer), Some(c)) = (buffer, c) {
                        buffer.push(c);
                    }
                    run_start = self.pos;
                }
                b'\n' | b'\r' => {
                    return Err(Fail {
                        offset: self.pos,
                        kind: SyntaxKind::LineBreakInString,
                        started: Some(start),
                    });
                }
                0..0x20 if !self.json5 => {
                    return Err(self.fail(SyntaxKind::ControlCharacter {
                        found: char::from(b),
                    }));
                }
                0x80.. if self.code_points > Severity::Off => {
                    let c = self.char_at(self.pos);
                    if is_noncharacter(u32::from(c)) {
                        self.report(
                            self.pos,
                            self.code_points,
                            Detail::InvalidCodePoint {
                                code_point: u32::from(c),
                            },
                        );
                    }
                    self.pos += c.len_utf8();
                }
                _ => self.pos += 1,
            }
        }
    }

    /// Read the escape at the current backslash and return the character it stands for, or
    /// `None` for a line continuation.
    fn escape(&mut self, string_start: usize) -> Result<Option<char>, Fail> {
        let at = self.pos;
        let Some(b) = self.bytes.get(at + 1).copied() else {
            return Err(Fail {
                offset: self.bytes.len(),
                kind: SyntaxKind::UnterminatedString,
                started: Some(string_start),
            });
        };
        self.pos += 2;
        let simple = match b {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'/' => Some('/'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b't' => Some('\t'),
            _ => None,
        };
        if simple.is_some() {
            return Ok(simple);
        }
        if b == b'u' {
            return self.unicode_escape(at).map(Some);
        }
        if !self.json5 {
            self.pos = at;
            return Err(self.fail(SyntaxKind::InvalidEscape {
                found: self.char_at(at + 1),
            }));
        }
        match b {
            b'\'' => Ok(Some('\'')),
            b'v' => Ok(Some('\u{b}')),
            // `\0` may not be followed by a digit: JSON5 has no octal escapes.
            b'0' if self.peek().is_some_and(|d| d.is_ascii_digit()) => {
                self.pos = at;
                Err(self.fail(SyntaxKind::InvalidEscape { found: '0' }))
            }
            b'0' => Ok(Some('\0')),
            b'1'..=b'9' => {
                self.pos = at;
                Err(self.fail(SyntaxKind::InvalidEscape {
                    found: char::from(b),
                }))
            }
            b'x' => {
                let Some(value) = self.hex(self.pos, 2) else {
                    self.pos = at;
                    return Err(self.fail(SyntaxKind::InvalidHexEscape));
                };
                self.pos += 2;
                Ok(char::from_u32(value))
            }
            // Line continuations: the backslash and the line break vanish.
            b'\n' => Ok(None),
            b'\r' => {
                if self.peek() == Some(b'\n') {
                    self.pos += 1;
                }
                Ok(None)
            }
            _ => {
                let c = self.char_at(at + 1);
                self.pos = at + 1 + c.len_utf8();
                Ok(if c == '\u{2028}' || c == '\u{2029}' {
                    None
                } else {
                    Some(c)
                })
            }
        }
    }

    /// The `\uXXXX` escape whose backslash is at `at`, with the current position just after
    /// the `u`. Joins a surrogate pair written as two escapes.
    fn unicode_escape(&mut self, at: usize) -> Result<char, Fail> {
        let Some(unit) = self.hex(self.pos, 4) else {
            self.pos = at;
            return Err(self.fail(SyntaxKind::InvalidUnicodeEscape));
        };
        self.pos += 4;
        let code_point = if (0xD800..0xDC00).contains(&unit)
            && self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u")
            && let Some(low) = self.hex(self.pos + 2, 4)
            && (0xDC00..0xE000).contains(&low)
        {
            self.pos += 6;
            0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
        } else {
            unit
        };
        if (0xD800..0xE000).contains(&code_point) || is_noncharacter(code_point) {
            self.report(
                at,
                self.code_points,
                Detail::InvalidCodePoint { code_point },
            );
        }
        Ok(char::from_u32(code_point).unwrap_or('\u{FFFD}'))
    }

    /// The value of `digits` hex digits at `at`, if they are all there.
    fn hex(&self, at: usize, digits: usize) -> Option<u32> {
        let text = self.text.get(at..at + digits)?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(text, 16).ok()
    }

    // --- numbers and literals --------------------------------------------------------------

    fn number(&mut self) -> Result<(), Fail> {
        let start = self.pos;
        match self.peek() {
            Some(b'+') if !self.json5 => return Err(self.not_allowed(start, Feature::Json5Numbers)),
            Some(b'+' | b'-') => self.pos += 1,
            _ => {}
        }
        match self.peek() {
            Some(b'I') => return self.word(start, "Infinity"),
            Some(b'N') => return self.word(start, "NaN"),
            Some(b'0') if matches!(self.bytes.get(self.pos + 1), Some(b'x' | b'X')) => {
                if !self.json5 {
                    return Err(self.not_allowed(start, Feature::Json5Numbers));
                }
                self.pos += 2;
                let digits = self.pos;
                while self.peek().is_some_and(|b| b.is_ascii_hexdigit()) {
                    self.pos += 1;
                }
                if self.pos == digits {
                    return Err(self.unexpected(Expected::HexDigit));
                }
                self.check_hex(start, digits);
                return Ok(());
            }
            _ => {}
        }
        let int_start = self.pos;
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else {
            self.digits();
        }
        let int_digits = self.pos > int_start;
        let mut point = false;
        let mut fraction_digits = false;
        if self.peek() == Some(b'.') {
            let next_is_digit = self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit);
            if !self.json5 && !next_is_digit {
                return Err(if int_digits {
                    self.not_allowed(start, Feature::Json5Numbers)
                } else {
                    self.unexpected(Expected::Value)
                });
            }
            self.pos += 1;
            point = true;
            fraction_digits = self.digits();
        }
        if !int_digits && !fraction_digits {
            if point {
                self.pos = int_start;
                return Err(self.unexpected(Expected::Value));
            }
            return Err(self.unexpected(Expected::Digit));
        }
        if !int_digits && !self.json5 {
            return Err(self.not_allowed(start, Feature::Json5Numbers));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.digits() {
                return Err(self.unexpected(Expected::Digit));
            }
        }
        self.check_decimal(start);
        Ok(())
    }

    /// Skip a run of decimal digits; true if there was at least one.
    fn digits(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        self.pos > start
    }

    /// `NaN` or `Infinity`, which only JSON5 allows.
    fn word(&mut self, start: usize, word: &'static str) -> Result<(), Fail> {
        if !self.json5 {
            return Err(if self.text[self.pos..].starts_with(word) {
                self.not_allowed(start, Feature::NanAndInfinity)
            } else {
                self.unexpected(Expected::Value)
            });
        }
        self.literal(word)
    }

    fn literal(&mut self, word: &'static str) -> Result<(), Fail> {
        for &expected in word.as_bytes() {
            if self.peek() != Some(expected) {
                return Err(self.unexpected(Expected::Literal(word)));
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn check_decimal(&mut self, start: usize) {
        if self.numbers == Severity::Off {
            return;
        }
        let text = &self.text[start..self.pos];
        let overflows = text.parse::<f64>().is_ok_and(f64::is_infinite);
        let integer = !text.contains(['.', 'e', 'E']);
        let digits = text.trim_start_matches(['+', '-']).trim_start_matches('0');
        let unsafe_integer = integer
            && (digits.len() > 16 || digits.parse::<u64>().is_ok_and(|n| n > MAX_SAFE_INTEGER));
        if overflows || unsafe_integer {
            self.report(start, self.numbers, Detail::InvalidNumber { overflows });
        }
    }

    fn check_hex(&mut self, start: usize, digits: usize) {
        if self.numbers == Severity::Off {
            return;
        }
        let digits = self.text[digits..self.pos].trim_start_matches('0');
        // 256 hex digits is past f64::MAX; 14 or more, or 2⁵³ itself, is past the safe range.
        let overflows = digits.len() > 256;
        let unsafe_integer = digits.len() > 14
            || u64::from_str_radix(digits, 16).is_ok_and(|n| n > MAX_SAFE_INTEGER);
        if overflows || unsafe_integer {
            self.report(start, self.numbers, Detail::InvalidNumber { overflows });
        }
    }

    // --- whitespace and comments -----------------------------------------------------------

    fn trivia(&mut self) -> Result<(), Fail> {
        loop {
            let Some(b) = self.peek() else { return Ok(()) };
            match b {
                b'\n' if self.lines => return Ok(()),
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                b'/' => match self.bytes.get(self.pos + 1) {
                    Some(b'/' | b'*') if !self.dialect.comments() => {
                        return Err(self.not_allowed(self.pos, Feature::Comments));
                    }
                    Some(b'/') => self.line_comment(),
                    Some(b'*') => self.block_comment()?,
                    _ => return Ok(()),
                },
                0x0B | 0x0C if self.json5 => self.pos += 1,
                0x80.. if self.json5 => {
                    let c = self.char_at(self.pos);
                    if !is_json5_space(c) {
                        return Ok(());
                    }
                    self.pos += c.len_utf8();
                }
                _ => return Ok(()),
            }
        }
    }

    /// Skip to the line terminator that ends a `//` comment, leaving it in place.
    fn line_comment(&mut self) {
        let mut from = self.pos + 2;
        while let Some(found) = memchr::memchr3(b'\n', b'\r', 0xE2, &self.bytes[from..]) {
            let at = from + found;
            let separator = self.bytes[at] != 0xE2
                || (self.json5
                    && self.bytes.get(at + 1) == Some(&0x80)
                    && matches!(self.bytes.get(at + 2), Some(0xA8 | 0xA9)));
            if separator {
                self.pos = at;
                return;
            }
            from = at + 1;
        }
        self.pos = self.bytes.len();
    }

    fn block_comment(&mut self) -> Result<(), Fail> {
        let start = self.pos;
        let mut from = self.pos + 2;
        while let Some(found) = memchr::memchr(b'*', &self.bytes[from..]) {
            let at = from + found;
            if self.bytes.get(at + 1) == Some(&b'/') {
                self.pos = at + 2;
                return Ok(());
            }
            from = at + 1;
        }
        Err(Fail {
            offset: self.bytes.len(),
            kind: SyntaxKind::UnterminatedComment,
            started: Some(start),
        })
    }
}

/// JSON5's whitespace outside ASCII: NBSP, the line and paragraph separators, the BOM, and
/// every other space separator (Unicode category Zs).
fn is_json5_space(c: char) -> bool {
    matches!(
        c,
        '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Unicode noncharacters: U+FDD0 to U+FDEF, and the last two code points of every plane.
fn is_noncharacter(code_point: u32) -> bool {
    (0xFDD0..=0xFDEF).contains(&code_point) || code_point & 0xFFFE == 0xFFFE
}

/// ECMAScript 5.1's `IdentifierStart` (section 7.6), which JSON5 identifiers follow: `$`, `_` or a
/// letter by general category. Not Unicode's `ID_Start`, which adds and drops a few characters.
fn is_identifier_start(c: char) -> bool {
    use unicode_general_category::GeneralCategory::{
        LetterNumber, LowercaseLetter, ModifierLetter, OtherLetter, TitlecaseLetter,
        UppercaseLetter,
    };
    if c.is_ascii() {
        return c.is_ascii_alphabetic() || c == '$' || c == '_';
    }
    matches!(
        unicode_general_category::get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | LetterNumber
    )
}

/// ECMAScript 5.1's `IdentifierPart`: a start character, a combining mark, a decimal digit, a
/// connector punctuation, ZWNJ or ZWJ.
fn is_identifier_part(c: char) -> bool {
    use unicode_general_category::GeneralCategory::{
        ConnectorPunctuation, DecimalNumber, NonspacingMark, SpacingMark,
    };
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '$' || c == '_';
    }
    is_identifier_start(c)
        || matches!(c, '\u{200C}' | '\u{200D}')
        || matches!(
            unicode_general_category::get_general_category(c),
            NonspacingMark | SpacingMark | DecimalNumber | ConnectorPunctuation
        )
}
