//! Whatever jsonvet accepts as JSON5, the json5 crate (1.3.1) must accept too. Not the other way
//! round: json5 1.3.1 accepts some invalid input, such as `"\\9"` and an unclosed `/*`.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let config = Config::new(Dialect::Json5).rule(Rule::DuplicateKey, Severity::Off);
    // The json5 crate counts U+2028 and U+2029 in strings as errors, which the spec allows.
    if text.contains(['\u{2028}', '\u{2029}']) || !check(text, &config).passed() {
        return;
    }
    if let Err(e) = json5::from_str::<serde::de::IgnoredAny>(text) {
        // JSON5 integers have no size limit, but json5's serde path needs one to fit 64 bits.
        if e.to_string()
            .starts_with("number too large to fit in target type")
        {
            return;
        }
        panic!("jsonvet accepts {text:?} as JSON5, json5 1.3.1 rejects it: {e}");
    }
});
