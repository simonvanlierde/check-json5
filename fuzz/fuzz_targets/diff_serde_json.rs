//! jsonvet and serde_json agree on what is JSON, both ways, apart from a byte order mark.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    // serde_json keeps the last of a duplicate key, so duplicates are not a syntax question.
    // NOTE: serde_json has no depth limit when it skips a value, so neither does jsonvet here.
    let config = Config::new(Dialect::Json)
        .rule(Rule::DuplicateKey, Severity::Off)
        .max_depth(usize::MAX);
    let report = check(text, &config);
    match serde_json::from_str::<serde::de::IgnoredAny>(text) {
        Ok(_) => assert!(
            report.passed(),
            "serde_json accepts {text:?}, jsonvet reports {:?}",
            report.problems()
        ),
        Err(e) if report.passed() => {
            // A byte order mark is only a warning in jsonvet; serde_json rejects it, and the
            // text without it must then be fine, so nothing else hides behind it.
            let bom_only = text
                .strip_prefix('\u{FEFF}')
                .is_some_and(|rest| serde_json::from_str::<serde::de::IgnoredAny>(rest).is_ok());
            assert!(
                bom_only,
                "jsonvet accepts {text:?} as JSON, serde_json rejects it: {e}"
            );
        }
        Err(_) => {}
    }
});
