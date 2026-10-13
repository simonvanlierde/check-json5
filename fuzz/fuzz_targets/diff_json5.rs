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
        // Limits of json5's serde path, not of the JSON5 grammar: integers must fit 64 bits, and
        // strings must be valid Rust strings, so a lone surrogate escape like `\uD800` fails.
        let message = e.to_string();
        if message.contains("to fit in target type")
            || message.starts_with("overflow parsing number")
            || message.starts_with("unpaired surrogate")
        {
            return;
        }
        panic!("jsonvet accepts {text:?} as JSON5, json5 1.3.1 rejects it: {e}");
    }
});
