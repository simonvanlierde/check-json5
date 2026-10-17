//! SARIF 2.1.0 output, for GitHub code scanning and other tools that read static analysis
//! results. The JSON is written by hand: the CLI keeps its dependencies few.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::Path;

use jsonvet::{Location, Rule, Severity};

/// A file that could not be read gets its own rule, so it shows as an alert. GitHub code scanning
/// lists tool notifications only on its tool status page, where a failing file is easy to miss.
pub(crate) const UNREADABLE: &str = "unreadable";

/// One result, as a JSON object. `related` is a second position with its message, such as a
/// duplicate key's first copy.
pub(crate) fn result(
    file: &OsStr,
    rule: &str,
    severity: Severity,
    message: &str,
    at: Location,
    related: Option<(Location, &str)>,
) -> String {
    let mut json = format!(
        r#"{{"ruleId":{},"level":"{}","message":{{"text":{}}},"locations":[{{{}}}]"#,
        string(rule),
        level(severity),
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

/// The level of a reported problem, the same word in SARIF and in a GitHub annotation: `error`
/// or `warning`.
pub(crate) fn level(severity: Severity) -> &'static str {
    if severity == Severity::Error {
        "error"
    } else {
        "warning"
    }
}

/// The whole document: one run holding every result.
pub(crate) fn document(results: &[String]) -> String {
    let rules: Vec<String> = Rule::ALL
        .iter()
        .map(|rule| (rule.id(), rule.description()))
        .chain([(
            UNREADABLE,
            "a file that is not a regular file, could not be read, or is not UTF-8",
        )])
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
            // Relative URIs name `%SRCROOT%`, which SARIF wants declared. It has no `uri`: the
            // working directory's absolute path would make the output differ by machine, and
            // GitHub code scanning resolves it against the checkout.
            r#""originalUriBaseIds":{{"%SRCROOT%":{{"description":{{"text":"#,
            r#""the working directory jsonvet ran in"}}}}}},"#,
            // jsonvet counts columns in code points; SARIF's default is UTF-16 code units.
            r#""columnKind":"unicodeCodePoints","results":[{}]}}]}}"#
        ),
        env!("CARGO_PKG_VERSION"),
        rules.join(","),
        results.join(",")
    )
}

/// The `physicalLocation` member of a location object.
fn physical(file: &OsStr, at: Location) -> String {
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
fn uri(file: &OsStr) -> (String, bool) {
    let absolute = Path::new(file).is_absolute();
    // The name's own bytes, so a name that is not UTF-8 still points at its file.
    let path = file.as_encoded_bytes();
    let mut uri = String::new();
    if absolute {
        // A Windows path starts with its drive letter, which needs a `/` before it.
        uri.push_str(if path.first() == Some(&b'/') {
            "file://"
        } else {
            "file:///"
        });
    }
    for &byte in path {
        let byte = if cfg!(windows) && byte == b'\\' {
            b'/'
        } else {
            byte
        };
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
fn string(value: &str) -> String {
    let mut json = String::with_capacity(value.len() + 2);
    json.push('"');
    for c in value.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            // GitHub Actions reads `##[command]` anywhere in a log line, the SARIF printed
            // there included; `\u005b` keeps the value and breaks the match.
            '[' if json.ends_with("##") => json.push_str("\\u005b"),
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
    use std::ffi::OsStr;

    use super::{string, uri};

    #[test]
    fn strings_escape_quotes_backslashes_and_control_characters() {
        assert_eq!(
            string("a\"b\\c\n\u{1}\u{7f}\u{e9}"),
            "\"a\\\"b\\\\c\\u000a\\u0001\u{7f}\u{e9}\""
        );
        assert_eq!(string("a b"), "\"a b\"");
        assert_eq!(string("##[add-mask]"), "\"##\\u005badd-mask]\"");
    }

    #[test]
    fn relative_paths_stay_relative_and_odd_bytes_are_encoded() {
        assert_eq!(
            uri(OsStr::new("a/b c#1.json")),
            ("a/b%20c%231.json".to_owned(), true)
        );
        assert_eq!(uri(OsStr::new("c:x.json")), ("c%3Ax.json".to_owned(), true));
        #[cfg(unix)]
        assert_eq!(
            uri(OsStr::new("a\\b.json")),
            ("a%5Cb.json".to_owned(), true)
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(
                uri(OsStr::from_bytes(b"\xe9.json")),
                ("%E9.json".to_owned(), true)
            );
        }
        #[cfg(unix)]
        assert_eq!(
            uri(OsStr::new("/abs/a b:c.json")),
            ("file:///abs/a%20b:c.json".to_owned(), false)
        );
        #[cfg(windows)]
        assert_eq!(
            uri(OsStr::new(r"C:\x\y.json")),
            ("file:///C:/x/y.json".to_owned(), false)
        );
    }
}
