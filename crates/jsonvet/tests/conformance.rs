//! The json5-tests suite (a git submodule at tests/json5-tests). Each file's extension says
//! what must happen: `.json` passes as JSON and JSON5, `.json5` passes as JSON5 only, and
//! `.js` and `.txt` fail as JSON5. The dialects nest, so a case that passes in a narrower one
//! must pass in every wider one.

#![allow(
    clippy::expect_used,
    reason = "a missing submodule or unreadable case should fail the test loudly"
)]

use std::fs;
use std::path::{Path, PathBuf};

mod common;

use jsonvet::{Config, Dialect, Rule, Severity, check};

/// The number of cases in the pinned submodule. A different count means a partial checkout or
/// an upstream change that needs a look.
const CASES: usize = 114;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/json5-tests")
}

fn suite() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir)
            .expect("the json5-tests submodule is checked out: run `git submodule update --init`")
        {
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

/// Cases the suite labels wrongly, with the reason. Reported as json5/json5-tests#4. The list
/// is a data file so conformance/report.py reads the same one.
fn mislabeled() -> Vec<(String, String)> {
    let list = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/json5-tests-mislabeled.tsv");
    fs::read_to_string(list)
        .expect("tests/json5-tests-mislabeled.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (case, why) = line.split_once('\t').expect("a path, a tab, a reason");
            (case.to_owned(), why.to_owned())
        })
        .collect()
}

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
    assert_eq!(
        files.len(),
        CASES,
        "wrong number of cases; is the submodule checked out (`git submodule update --init`)?"
    );
    let mislabeled = mislabeled();
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
        let waived = mislabeled.iter().any(|(case, _)| path.ends_with(case));
        if as_json != expected_json && !waived {
            wrong.push(format!("{} as json: passed={as_json}", path.display()));
        }
        let text = fs::read_to_string(path).expect("UTF-8 test file");
        for violation in common::nesting_violations(&text) {
            wrong.push(format!("{}: {violation}", path.display()));
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

/// A waiver that no longer matches a case, or whose case now behaves as labeled, is stale.
#[test]
fn mislabeled_entries_are_still_needed() {
    let mislabeled = mislabeled();
    for (case, why) in mislabeled {
        let path = root().join(&case);
        assert!(
            path.is_file(),
            "{case} is listed as mislabeled but is not in the suite"
        );
        assert!(
            passes(&path, Dialect::Json5),
            "{case} should pass as json5 ({why})"
        );
        assert!(
            !passes(&path, Dialect::Json),
            "{case} should fail as json ({why}); drop it from the list if the suite is fixed"
        );
    }
}
