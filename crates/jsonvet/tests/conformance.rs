//! The json5-tests suite (a git submodule at tests/json5-tests). Each file's extension says
//! what must happen: `.json` passes as JSON and JSON5, `.json5` passes as JSON5 only, and
//! `.js` and `.txt` fail as JSON5.

#![allow(
    clippy::expect_used,
    reason = "a missing submodule or unreadable case should fail the test loudly"
)]

use std::fs;
use std::path::{Path, PathBuf};

use jsonvet::{Config, Dialect, Rule, Severity, check};

fn suite() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/json5-tests");
    let mut files = Vec::new();
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).expect("the json5-tests submodule is checked out") {
            let path = entry.expect("readable directory").path();
            if path.is_dir() {
                dirs.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("json" | "json5" | "js" | "txt")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Cases the suite labels wrongly, with the reason.
const MISLABELED: &[(&str, &str)] = &[(
    "comments/irregular-block-comment.json",
    "contains a block comment, so it is JSON5 but not JSON; the suite names it .json",
)];

/// The suite tests syntax only, so duplicate keys, which jsonvet reports by default, are off.
fn passes(path: &Path, dialect: Dialect) -> bool {
    let text = fs::read_to_string(path).expect("UTF-8 test file");
    check(
        &text,
        &Config::new(dialect).rule(Rule::DuplicateKey, Severity::Off),
    )
    .passed()
}

#[test]
fn json5_tests_suite() {
    let files = suite();
    assert!(
        files.len() > 100,
        "found only {} cases; is the submodule checked out?",
        files.len()
    );
    let mut wrong = Vec::new();
    for path in &files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let as_json5 = passes(path, Dialect::Json5);
        let as_json = passes(path, Dialect::Json);
        let expected_json5 = matches!(ext, "json" | "json5");
        let expected_json = ext == "json";
        if as_json5 != expected_json5 {
            wrong.push(format!("{} as json5: passed={as_json5}", path.display()));
        }
        let mislabeled = MISLABELED.iter().any(|(case, _)| path.ends_with(case));
        if as_json != expected_json && !mislabeled {
            wrong.push(format!("{} as json: passed={as_json}", path.display()));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases wrong:\n{}",
        wrong.len(),
        files.len(),
        wrong.join("\n")
    );
}
