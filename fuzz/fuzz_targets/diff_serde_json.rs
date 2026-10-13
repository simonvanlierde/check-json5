//! jsonvet and serde_json agree on what is JSON, both ways, apart from serde_json's own limits.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    // serde_json keeps the last of a duplicate key, so duplicates are not a syntax question.
    let config = Config::new(Dialect::Json).rule(Rule::DuplicateKey, Severity::Off);
    let report = check(text, &config);
    match serde_json::from_str::<serde::de::IgnoredAny>(text) {
        Ok(_) => assert!(
            report.passed(),
            "serde_json accepts {text:?}, jsonvet reports {:?}",
            report.problems()
        ),
        Err(e) if report.passed() => {
            // A byte order mark is only a warning in jsonvet. serde_json stops at 128 levels
            // of nesting, and at numbers past a double's range, which JSON allows.
            let message = e.to_string();
            let limit = text.starts_with('\u{FEFF}')
                || message.starts_with("recursion limit exceeded")
                || message.starts_with("number out of range")
                || message.contains("surrogate");
            assert!(limit, "jsonvet accepts {text:?} as JSON, serde_json rejects it: {e}");
        }
        Err(_) => {}
    }
});
