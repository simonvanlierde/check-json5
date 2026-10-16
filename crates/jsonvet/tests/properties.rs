//! Properties that hold for every input in every dialect: no panic, every report keeping the
//! promises `common::report_violation` lists, and a text that passes in a narrower dialect
//! passing in the wider ones.

mod common;

use common::{nesting_violations, report_violation, strict};
use jsonvet::{Config, Dialect};
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
    fn reports_keep_their_promises(text in soup(), dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        for config in [Config::new(dialect), strict()] {
            prop_assert_eq!(report_violation(&text, &config), Ok(()));
        }
    }

    #[test]
    fn dialects_nest(text in soup()) {
        prop_assert_eq!(nesting_violations(&text), Vec::<String>::new());
    }
}
