//! Checks shared by the integration tests and the fuzz target (which includes this file by path).
#![allow(
    dead_code,
    reason = "each test crate that includes this file uses only some of it"
)]

use jsonvet::{Config, Dialect, Rule, Severity, check};

/// Line and column the slow way: count terminators and characters from the start.
pub(crate) fn recount(text: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column) = (1, 1);
    // A CR followed by LF is one line end, even when the offset points at the LF.
    for (i, c) in text.char_indices().take_while(|&(i, _)| i < offset) {
        match c {
            '\r' if text[i + 1..].starts_with('\n') => column += 1,
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => (line, column) = (line + 1, 1),
            _ => column += 1,
        }
    }
    (line, column)
}

fn passes(text: &str, dialect: Dialect) -> bool {
    // Duplicate keys are not a syntax question and every dialect treats them the same.
    check(
        text,
        &Config::new(dialect).rule(Rule::DuplicateKey, Severity::Off),
    )
    .passed()
}

/// Where the dialects, which nest as JSON < JSONC < JWCC < JSON5, break that order: a text that
/// passes in a narrower dialect and fails in a wider one. I-JSON only adds rules to JSON, so
/// what passes there passes as JSON.
///
/// NOTE: JWCC ends a line comment at LF only, JSONC also at a lone CR, and JSON5 also at U+2028
/// and U+2029. A comment can swallow different text in each, so the chain between the comment
/// dialects is only checked on texts without those characters. JSON has no comments, so it is
/// below every dialect either way.
pub(crate) fn nesting_violations(text: &str) -> Vec<String> {
    let mut pairs = vec![
        (Dialect::IJson, Dialect::Json),
        (Dialect::Json, Dialect::Jsonc),
        (Dialect::Json, Dialect::Jwcc),
        (Dialect::Json, Dialect::Json5),
    ];
    if !text.contains(['\r', '\u{2028}', '\u{2029}']) {
        pairs.extend([
            (Dialect::Jsonc, Dialect::Jwcc),
            (Dialect::Jwcc, Dialect::Json5),
        ]);
    }
    pairs
        .into_iter()
        .filter(|&(narrow, wide)| passes(text, narrow) && !passes(text, wide))
        .map(|(narrow, wide)| format!("passes as {narrow} but not {wide}"))
        .collect()
}
