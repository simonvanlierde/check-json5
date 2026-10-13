//! JSONPath rendering, matching check-json5 1.4: `$.key` for identifier-like keys,
//! `$["other key"]` otherwise, with the key quoted as Python's `json.dumps` quotes it.

use std::borrow::Cow;
use std::fmt::Write;

/// One step from a container to a child.
#[derive(Clone, Debug)]
pub(crate) enum Segment<'a> {
    Root,
    Key(Cow<'a, str>),
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

pub(crate) fn push_key(path: &mut String, key: &str) {
    if is_identifier(key) {
        path.push('.');
        path.push_str(key);
    } else {
        path.push('[');
        quote(path, key);
        path.push(']');
    }
}

/// Python's `[A-Za-z_$][\w$]*`, where `\w` takes any Unicode letter or digit.
fn is_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Quote a key as `json.dumps` does with its defaults: non-ASCII as lowercase `\uXXXX` escapes.
fn quote(out: &mut String, key: &str) {
    out.push('"');
    for c in key.chars() {
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
        push_key(&mut path, key);
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
    }
}
