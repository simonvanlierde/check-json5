//! Properties that hold for every input in every dialect: no panic, every offset inside the
//! text and on a character boundary, every location equal to a naive recount, and a text that
//! passes in a narrower dialect passing in the wider ones.

mod common;

use common::{nesting_violations, recount};
use jsonvet::{Config, Dialect, Rule, Severity, check};
use proptest::prelude::*;

/// JSON-ish token soup, so most inputs get past the first byte, mixed with arbitrary text for
/// the rest.
fn soup() -> impl Strategy<Value = String> {
    let tokens = prop::sample::select(vec![
        // Whole values, so some inputs are valid in the narrower dialects.
        "[]",
        "[1]",
        "[1,]",
        "{\"a\":1}",
        "{\"a\":1,}",
        "{",
        "}",
        "[",
        "]",
        ":",
        ",",
        "\"a\"",
        "'b'",
        "c",
        "1",
        "-2.5e3",
        "0x1F",
        ".5",
        "NaN",
        "true",
        "null",
        "// x\n",
        "/* y */",
        "/*",
        "\"",
        "\\",
        "\\u0061",
        "\n",
        "\r\n",
        "\r",
        " ",
        "\t",
        "\u{2028}",
        "\u{A0}",
        "\u{FEFF}",
        "é",
        "😀",
        "\u{1}",
        "\"\\ud800\"",
    ]);
    // Runs deep enough to reach the depth limit and long enough to overflow a number's range.
    let runs = prop::sample::select(vec![
        "[".repeat(300),
        "{\"a\":".repeat(150),
        "[".repeat(12_000),
        "9".repeat(400),
        "1".repeat(20),
        format!("0.{}", "0".repeat(400)),
        format!("1e{}", "9".repeat(30)),
    ]);
    let token = prop_oneof![
        20 => tokens.prop_map(String::from),
        1 => runs,
    ];
    let soup = prop::collection::vec(token, 0..40).prop_map(|t| t.concat());
    prop_oneof![4 => soup, 1 => "\\PC{0,64}"]
}

proptest! {
    // 4000 cases by default; PROPTEST_CASES sets more for a long run.
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(4000)
    ))]

    #[test]
    fn offsets_are_valid_and_locations_agree(text in soup(), dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        // The same config as the fuzz target's extra run: the number check, which is off by
        // default, and a depth limit the soup can reach.
        let strict = Config::new(Dialect::Json5)
            .rule(Rule::InvalidNumber, Severity::Error)
            .max_depth(8);
        for config in [Config::new(dialect), strict] {
            let report = check(&text, &config);
            for problem in report.problems() {
                for offset in std::iter::once(problem.offset()).chain(problem.related_offset()) {
                    prop_assert!(offset <= text.len());
                    prop_assert!(text.is_char_boundary(offset));
                    let at = report.location(offset);
                    prop_assert_eq!((at.line, at.column), recount(&text, offset));
                }
            }
            // Again backwards: `location` counts from its last answer when that is nearer.
            for problem in report.problems().iter().rev() {
                let at = report.location(problem.offset());
                prop_assert_eq!((at.line, at.column), recount(&text, problem.offset()));
            }
        }
    }

    #[test]
    fn dialects_nest(text in soup()) {
        prop_assert_eq!(nesting_violations(&text), Vec::<String>::new());
    }
}
