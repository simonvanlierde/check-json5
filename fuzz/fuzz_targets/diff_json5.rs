//! Whatever jsonvet accepts as JSON5, the json5 crate (1.3.1) must accept too. Not the other way
//! round: json5 1.3.1 accepts some invalid input, such as `"\9"` and an unclosed `/*`.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

/// Whether the text has a `\uD800` to `\uDFFF` escape. JSON5 strings may hold lone surrogates,
/// as JavaScript strings may; json5 decodes into Rust strings, which cannot, and rejects them.
fn has_surrogate_escape(text: &str) -> bool {
    text.match_indices("\\u").any(|(at, _)| {
        text.get(at + 2..at + 6)
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .is_some_and(|unit| (0xD800..0xE000).contains(&unit))
    })
}

/// Whether the text has a character, literal or escaped, that jsonvet's Unicode tables call a
/// letter, mark, digit or connector and json5's older tables do not know, such as U+05C8 (Unicode
/// 17). json5 then rejects it anywhere in a key, even after a first letter.
fn has_newer_identifier_char(text: &str) -> bool {
    use unicode_general_category::GeneralCategory::{
        ConnectorPunctuation, DecimalNumber, LetterNumber, LowercaseLetter, ModifierLetter,
        NonspacingMark, OtherLetter, SpacingMark, TitlecaseLetter, UppercaseLetter,
    };
    let escaped = text.match_indices("\\u").filter_map(|(at, _)| {
        let hex = text.get(at + 2..at + 6)?;
        char::from_u32(u32::from_str_radix(hex, 16).ok()?)
    });
    text.chars()
        .chain(escaped)
        .filter(|c| {
            !c.is_ascii()
                && matches!(
                    unicode_general_category::get_general_category(*c),
                    UppercaseLetter
                        | LowercaseLetter
                        | TitlecaseLetter
                        | ModifierLetter
                        | OtherLetter
                        | LetterNumber
                        | NonspacingMark
                        | SpacingMark
                        | DecimalNumber
                        | ConnectorPunctuation
                )
        })
        .any(|c| json5::from_str::<serde::de::IgnoredAny>(&format!("{{a{c}: 1}}")).is_err())
}

fuzz_target!(|text: &str| {
    let config = Config::new(Dialect::Json5).rule(Rule::DuplicateKey, Severity::Off);
    // The json5 crate counts U+2028 and U+2029 in strings as errors, which the spec allows.
    if text.contains(['\u{2028}', '\u{2029}'])
        || has_surrogate_escape(text)
        || has_newer_identifier_char(text)
        || !check(text, &config).passed()
    {
        return;
    }
    if let Err(e) = json5::from_str::<serde::de::IgnoredAny>(text) {
        // JSON5 numbers have no size limit, but json5's serde path needs integers to fit 64 bits.
        let message = e.to_string();
        if message.contains("to fit in target type")
            || message.starts_with("overflow parsing number")
        {
            return;
        }
        panic!("jsonvet accepts {text:?} as JSON5, json5 1.3.1 rejects it: {e}");
    }
});
