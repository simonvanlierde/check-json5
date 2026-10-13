//! Properties that hold for every input in every dialect: no panic, every offset inside the
//! text and on a character boundary, and every location equal to a naive recount.

use jsonvet::{Config, Dialect, check};
use proptest::prelude::*;

/// JSON-ish token soup, so most inputs get past the first byte.
fn soup() -> impl Strategy<Value = String> {
    let tokens = prop::sample::select(vec![
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
    prop::collection::vec(tokens, 0..40).prop_map(|t| t.concat())
}

/// Line and column the slow way: count terminators and characters from the start.
fn recount(text: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column) = (1, 1);
    // A CR followed by LF is one line end, even when the offset points at the LF.
    for (i, c) in text.char_indices().take_while(|&(i, _)| i < offset) {
        match c {
            '\r' if text[i + 1..].starts_with('\n') => column += 1,
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => (line, column) = (line + 1, 1),
            _ => column += 1,
        }
    }
    (line, column)
}

proptest! {
    // 4000 cases by default; PROPTEST_CASES sets more for a long run.
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(4000)
    ))]

    #[test]
    fn offsets_are_valid_and_locations_agree(text in soup(), dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        let report = check(&text, &Config::new(dialect));
        for problem in report.problems() {
            for offset in std::iter::once(problem.offset()).chain(problem.related_offset()) {
                prop_assert!(offset <= text.len());
                prop_assert!(text.is_char_boundary(offset));
                let at = report.location(offset);
                prop_assert_eq!((at.line, at.column), recount(&text, offset));
            }
        }
    }

    #[test]
    fn arbitrary_text_never_panics(text in "\\PC{0,64}", dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        let _ = check(&text, &Config::new(dialect));
    }
}
