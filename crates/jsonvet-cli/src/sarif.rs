//! SARIF 2.1.0 output, for GitHub code scanning and other tools that read static analysis
//! results. The JSON is written by hand: the CLI keeps its dependencies few.

use std::fmt::Write as _;
use std::path::Path;

use jsonvet::{Location, Rule, Severity};

/// A file that could not be read gets its own rule, so it shows as an alert. GitHub code scanning
/// lists tool notifications only on its tool status page, where a failing file is easy to miss.
pub(crate) const UNREADABLE: &str = "unreadable";

/// One result, as a JSON object. `related` is a second position with its message, such as a
/// duplicate key's first copy.
pub(crate) fn result(
    file: &str,
    rule: &str,
    severity: Severity,
    message: &str,
    at: Location,
    related: Option<(Location, &str)>,
) -> String {
    let level = if severity == Severity::Error {
        "error"
    } else {
        "warning"
    };
    let mut json = format!(
        r#"{{"ruleId":{},"level":"{level}","message":{{"text":{}}},"locations":[{{{}}}]"#,
        string(rule),
        string(message),
        physical(file, at),
    );
    if let Some((first, text)) = related {
        let _ = write!(
            json,
            r#","relatedLocations":[{{{},"message":{{"text":{}}}}}]"#,
            physical(file, first),
            string(text)
        );
    }
    json.push('}');
    json
}

/// The whole document: one run holding every result.
pub(crate) fn document(results: &[String]) -> String {
    let rules: Vec<String> = Rule::ALL
        .iter()
        .map(|rule| (rule.id(), description(*rule)))
        .chain([(UNREADABLE, "a file that could not be read, or is not UTF-8")])
        .map(|(id, text)| {
            format!(
                r#"{{"id":{},"shortDescription":{{"text":{}}}}}"#,
                string(id),
                string(text)
            )
        })
        .collect();
    format!(
        concat!(
            r#"{{"$schema":"https://json.schemastore.org/sarif-2.1.0.json","version":"2.1.0","#,
            r#""runs":[{{"tool":{{"driver":{{"name":"jsonvet","version":"{}","#,
            r#""informationUri":"https://github.com/simonvanlierde/jsonvet","rules":[{}]}}}},"#,
            // jsonvet counts columns in code points; SARIF's default is UTF-16 code units.
            r#""columnKind":"unicodeCodePoints","results":[{}]}}]}}"#
        ),
        env!("CARGO_PKG_VERSION"),
        rules.join(","),
        results.join(",")
    )
}

fn description(rule: Rule) -> &'static str {
    match rule {
        Rule::Syntax => "the first syntax error; checking stops there",
        Rule::DuplicateKey => "a key repeated in one object",
        Rule::InvalidNumber => "a number a double cannot hold, or an integer past 2^53 - 1",
        Rule::InvalidCodePoint => "a lone surrogate or a noncharacter in a string",
        Rule::ByteOrderMark => "a byte order mark at the start of the file",
        Rule::MissingFinalNewline => "a JSON Lines file that does not end with a newline",
        _ => "",
    }
}

/// The `physicalLocation` member of a location object.
fn physical(file: &str, at: Location) -> String {
    let (uri, base) = uri(file);
    let base = if base {
        r#","uriBaseId":"%SRCROOT%""#
    } else {
        ""
    };
    format!(
        r#""physicalLocation":{{"artifactLocation":{{"uri":{}{base}}},"region":{{"startLine":{},"startColumn":{}}}}}"#,
        string(&uri),
        at.line,
        at.column
    )
}

/// The file name as a URI reference, and whether it is relative to the source root. Bytes
/// outside a URI's unreserved set are percent-encoded, so a space or `#` in a name stays part
/// of the path.
fn uri(file: &str) -> (String, bool) {
    let absolute = Path::new(file).is_absolute();
    let path = if cfg!(windows) {
        file.replace('\\', "/")
    } else {
        file.to_owned()
    };
    let mut uri = String::new();
    if absolute {
        // A Windows path starts with its drive letter, which needs a `/` before it.
        uri.push_str(if path.starts_with('/') {
            "file://"
        } else {
            "file:///"
        });
    }
    for byte in path.bytes() {
        // `:` stays literal only in an absolute path's drive letter: in the first segment of a
        // relative one it would read as a URI scheme.
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) || (absolute && byte == b':') {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    (uri, !absolute)
}

/// A JSON string literal.
pub(crate) fn string(value: &str) -> String {
    let mut json = String::with_capacity(value.len() + 2);
    json.push('"');
    for c in value.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            c if c < ' ' => {
                let _ = write!(json, "\\u{:04x}", u32::from(c));
            }
            c => json.push(c),
        }
    }
    json.push('"');
    json
}

#[cfg(test)]
mod tests {
    use super::{string, uri};

    #[test]
    fn strings_escape_quotes_backslashes_and_control_characters() {
        assert_eq!(
            string("a\"b\\c\n\u{1}\u{7f}\u{e9}"),
            "\"a\\\"b\\\\c\\u000a\\u0001\u{7f}\u{e9}\""
        );
    }

    #[test]
    fn relative_paths_stay_relative_and_odd_bytes_are_encoded() {
        assert_eq!(uri("a/b c#1.json"), ("a/b%20c%231.json".to_owned(), true));
        assert_eq!(uri("c:x.json"), ("c%3Ax.json".to_owned(), true));
        #[cfg(unix)]
        assert_eq!(uri("/abs/x.json"), ("file:///abs/x.json".to_owned(), false));
        #[cfg(windows)]
        assert_eq!(
            uri(r"C:\x\y.json"),
            ("file:///C:/x/y.json".to_owned(), false)
        );
    }
}
