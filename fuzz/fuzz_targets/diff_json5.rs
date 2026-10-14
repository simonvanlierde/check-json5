//! jsonvet and the json5 crate (1.3.2) agree on what is JSON5, both ways, apart from json5's own
//! limits.
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

/// Whether the text has a character, literal or escaped, that jsonvet's Unicode 16 tables leave
/// unassigned, such as U+3308C, which json5's Unicode 17 tables call a letter (CJK Extension J).
fn has_unassigned_char(text: &str) -> bool {
    let escaped = text.match_indices("\\u").filter_map(|(at, _)| {
        let hex = text.get(at + 2..at + 6)?;
        char::from_u32(u32::from_str_radix(hex, 16).ok()?)
    });
    text.chars().chain(escaped).any(|c| {
        unicode_general_category::get_general_category(c)
            == unicode_general_category::GeneralCategory::Unassigned
    })
}

fuzz_target!(|text: &str| {
    let config = Config::new(Dialect::Json5).rule(Rule::DuplicateKey, Severity::Off);
    let report = check(text, &config);
    match json5::from_str::<serde::de::IgnoredAny>(text) {
        Ok(_) => assert!(
            report.passed() || has_unassigned_char(text),
            "json5 accepts {text:?}, jsonvet reports {:?}",
            report.problems()
        ),
        Err(e) if report.passed() => {
            // json5 counts U+2028 and U+2029 in strings as errors, which the spec allows. JSON5
            // numbers have no size limit, but json5's serde path needs integers to fit 64 bits.
            let message = e.to_string();
            let limit = text.contains(['\u{2028}', '\u{2029}'])
                || has_surrogate_escape(text)
                || has_newer_identifier_char(text)
                || message.contains("to fit in target type")
                || message.starts_with("overflow parsing number");
            assert!(limit, "jsonvet accepts {text:?} as JSON5, json5 rejects it: {e}");
        }
        Err(_) => {}
    }
});
