//! Whatever serde_json accepts as JSON, jsonvet must accept as JSON.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    // serde_json keeps the last of a duplicate key, so duplicates are not a syntax question.
    if serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok() {
        let config = Config::new(Dialect::Json).rule(Rule::DuplicateKey, Severity::Off);
        let report = check(text, &config);
        assert!(
            report.passed(),
            "serde_json accepts {text:?}, jsonvet reports {:?}",
            report.problems()
        );
    }
});
