//! JSONPath rendering, matching check-json5 1.4: `$.key` for identifier-like keys,
//! `$["other key"]` otherwise, with the key quoted as Python's `json.dumps` quotes it.

use std::borrow::Cow;
use std::fmt::Write;

/// One step from a container to a child.
#[derive(Clone, Debug)]
pub(crate) enum Segment<'a> {
    Root,
    Key(Cow<'a, [u8]>),
    Index(usize),
}

/// Append one segment to a path.
pub(crate) fn push(path: &mut String, segment: &Segment<'_>) {
    match segment {
        Segment::Root => path.push('$'),
        Segment::Key(key) => push_key(path, key),
        Segment::Index(i) => {
            let _ = write!(path, "[{i}]");
        }
    }
}

/// Append a key, given as the parser's WTF-8 (see `parse::Key`).
pub(crate) fn push_key(path: &mut String, key: &[u8]) {
    let chars = decode(key);
    if is_identifier(&chars) {
        path.push('.');
        path.extend(chars.iter().flatten());
    } else {
        path.push('[');
        quote(path, &chars);
        path.push(']');
    }
}

/// A key's characters, with each lone surrogate as `Err(unit)`.
fn decode(mut key: &[u8]) -> Vec<Result<char, u32>> {
    let mut chars = Vec::new();
    loop {
        let valid = match std::str::from_utf8(key) {
            Ok(text) => text,
            Err(e) => std::str::from_utf8(&key[..e.valid_up_to()]).unwrap_or_default(),
        };
        chars.extend(valid.chars().map(Ok));
        // Anything after the valid part is a lone surrogate, which the parser wrote as
        // `ED`, then 6 bits, then 6 bits.
        let Some(&[_, high, low]) = key.get(valid.len()..valid.len() + 3) else {
            return chars;
        };
        chars.push(Err(0xD000
            | (u32::from(high & 0x3F) << 6)
            | u32::from(low & 0x3F)));
        key = &key[valid.len() + 3..];
    }
}

/// Python's `[A-Za-z_$][\w$]*`, where `\w` takes any Unicode letter or digit, and no surrogate.
fn is_identifier(chars: &[Result<char, u32>]) -> bool {
    let mut chars = chars.iter();
    chars
        .next()
        .is_some_and(|c| matches!(c, Ok(c) if c.is_ascii_alphabetic() || *c == '_' || *c == '$'))
        && chars.all(|c| matches!(c, Ok(c) if c.is_alphanumeric() || *c == '_' || *c == '$'))
}

/// Quote a key as `json.dumps` does with its defaults: non-ASCII as lowercase `\uXXXX` escapes.
fn quote(out: &mut String, chars: &[Result<char, u32>]) {
    out.push('"');
    for c in chars {
        let c = match c {
            Ok(c) => *c,
            Err(unit) => {
                let _ = write!(out, "\\u{unit:04x}");
                continue;
            }
        };
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::push_key;

    fn rendered(key: &str) -> String {
        let mut path = "$".to_owned();
        push_key(&mut path, key.as_bytes());
        path
    }

    #[test]
    fn keys_render_as_check_json5_rendered_them() {
        assert_eq!(rendered("server"), "$.server");
        assert_eq!(rendered("$ref"), "$.$ref");
        assert_eq!(rendered("café"), "$.café");
        assert_eq!(rendered("1st"), "$[\"1st\"]");
        assert_eq!(rendered("b c"), "$[\"b c\"]");
        assert_eq!(rendered("ünicode"), "$[\"\\u00fcnicode\"]");
        assert_eq!(rendered("q\"\\\n"), "$[\"q\\\"\\\\\\n\"]");
        assert_eq!(rendered("😀"), "$[\"\\ud83d\\ude00\"]");
        assert_eq!(rendered(""), "$[\"\"]");
        let mut path = "$".to_owned();
        push_key(&mut path, b"a\xED\xA0\x80b");
        assert_eq!(path, "$[\"a\\ud800b\"]");
    }
}
