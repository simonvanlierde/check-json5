//! Behaviour across the dialects, as one reviewed snapshot: each case shows every problem with
//! its position, severity, rule and message.

use std::fmt::Write;

use jsonvet::{Config, Dialect, check};

fn render(dialect: Dialect, text: &str) -> String {
    let report = check(text, &Config::new(dialect));
    let mut out = format!("{dialect:<6} {text:?}\n");
    if report.problems().is_empty() {
        out.push_str("       ok\n");
    }
    for problem in report.problems() {
        let at = report.location(problem.offset());
        let _ = write!(
            out,
            "       {}:{} {}[{}] {problem}",
            at.line,
            at.column,
            problem.severity(),
            problem.rule()
        );
        if let Some(related) = problem.related_offset() {
            let first = report.location(related);
            let _ = write!(out, " (from {}:{})", first.line, first.column);
        }
        out.push('\n');
    }
    out
}

fn snapshot(cases: &[(Dialect, &str)]) -> String {
    cases.iter().map(|(d, t)| render(*d, t)).collect()
}

#[test]
fn syntax_in_every_dialect() {
    use Dialect::{Json, Json5, Jsonc, Jwcc};
    insta::assert_snapshot!(snapshot(&[
        // Empty documents, containers and trailing commas.
        (Json, ""),
        (Json, "   "),
        (Json, r#"{"a": 1}"#),
        (Json, "[1, 2,]"),
        (Jwcc, "[1, 2,]"),
        (Json, r#"{"a": 1,}"#),
        (Json, "[,]"),
        (Json5, "[,]"),
        (Json5, "[1,,]"),
        (Json5, "{,}"),
        // Unquoted keys and single quotes.
        (Json, "{a: 1}"),
        (Json5, "{a: 1, $b: 2, _c: 3, ünï: 4}"),
        // Identifier characters go by general category, as in ECMAScript 5.1, not by Unicode's
        // ID_Start and ID_Continue: U+1885 is a mark, U+00B7 punctuation, U+0E33 and U+2E2F letters.
        (Json5, "{\u{1885}: 1}"),
        (Json5, "{\\u1885: 1}"),
        (Json5, "{a\u{1885}: 1}"),
        (Json5, "{a\u{B7}: 1}"),
        (Json5, "{\u{E33}: 1, \u{2E2F}: 2}"),
        (Json, "['x']"),
        (Json5, "['x', \"y\"]"),
        // Comments.
        (Json, "// c\n1"),
        (Jsonc, "// c\n1"),
        (Jsonc, "/* open"),
        // Missing separators, trailing text and literals.
        (Json, "[1 2]"),
        (Json, r#"{"a" 1}"#),
        (Json, r#"{"a":1 "b":2}"#),
        (Json, "1 2"),
        (Json, "[1]x"),
        (Json, "nul"),
        (Json, "nux"),
        (Json, "nullx"),
        // Numbers.
        (Json, "NaN"),
        (Json, "-Infinity"),
        (Json5, "[NaN, +Infinity, -Infinity, +NaN]"),
        (Json, "01"),
        (Json, "1."),
        (Json, ".5"),
        (Json, "+1"),
        (Json, "0x1F"),
        (Json5, "[0x1F, -0x10, .5, 5., +1, 1e3, 1E-3]"),
        (Json5, "0x"),
        (Json5, "1e"),
        (Json5, "."),
        (Json, "-"),
        // Strings and escapes.
        (Json, "\"abc"),
        (Json, "\"a\nb\""),
        (Json, "\"a\tb\""),
        (Json5, "\"a\tb\""),
        (Json, r#""\q""#),
        (Json5, r#""\q""#),
        (Json5, r#""\1""#),
        (Json5, r#""\9""#),
        (Json5, r#""\01""#),
        (Json5, r#""\0""#),
        (Json5, r#""\x4""#),
        (Json, r#""\u12x4""#),
        (Json5, "\"a\\\nb\""),
        // JSON5's extra line terminators and whitespace.
        (Json5, "\"\u{2028}\""),
        (Json5, "[1, // c\u{2028} 2]"),
        (Jsonc, "[1, // c\u{2028} 2]"),
        (Json5, "\u{A0}[\u{3000}1\u{FEFF}]\u{2029}"),
        (Json, "[\u{A0}1]"),
    ]));
}

#[test]
fn duplicate_keys() {
    use Dialect::{IJson, Json, Json5};
    insta::assert_snapshot!(snapshot(&[
        (Json, r#"{"a": 1, "a": 2}"#),
        (
            Json,
            "{\n  \"x\": {\"a\": 1, \"\\u0061\": 2},\n  \"l\": [{\"k\": 1}, {\"k\": 2, \"k\": 3}],\n  \"x\": 0\n}"
        ),
        (Json5, "{a: 1, 'a': 2, \"b c\": {d: 1, d: 2}, \\u0061: 3}"),
        (Json5, "{\"ünicode\": 1, \"ünicode\": 2}"),
        // Lone surrogates stay distinct keys, as in Python; U+FFFD is not one of them.
        (
            Json,
            r#"{"\ud800": 1, "\udbff": 2, "\ufffd": 3, "a\udc00": 4}"#
        ),
        (
            Json,
            r#"{"\ud800": 1, "\uD800": 2, "\ud83d\ude00": 3, "😀": 4}"#
        ),
        (IJson, r#"{"a": 1, "a": 2}"#),
        (Json, r#"[{"a": 1, "a": 2}, {"\x": 0}]"#),
    ]));
}

#[test]
fn i_json_and_json_lines() {
    use Dialect::{IJson, Json, JsonLines};
    insta::assert_snapshot!(snapshot(&[
        (
            IJson,
            "[9007199254740991, 9007199254740992, -9007199254740993, 1e400, 1.5]"
        ),
        (Json, "[9007199254740992, 1e400]"),
        (IJson, r#"["\ud800", "\ud83d\ude00", "\ufdd0", "﷐"]"#),
        (IJson, "\u{FEFF}{}"),
        (Json, "\u{FEFF}{}"),
        (JsonLines, ""),
        (JsonLines, "{\"a\": 1}\n[2]\n3\n"),
        (JsonLines, "{\"a\": 1}\n[2]"),
        (JsonLines, "{\"a\": 1}\n\n[2]\n"),
        (JsonLines, "{\"a\": 1}\n   \n"),
        (JsonLines, "{\"a\":\n1}\n"),
        (JsonLines, "1 2\n"),
        (JsonLines, "{\"a\": 1, \"a\": 2}\r\n[3]\r\n"),
        (JsonLines, "\u{FEFF}{}\n"),
    ]));
}

#[test]
fn positions_follow_json5_line_terminators() {
    use Dialect::Json5;
    insta::assert_snapshot!(snapshot(&[
        (Json5, "{a: 1,\r\n a: 2}"),
        (Json5, "{a: 1,\r a: 2}"),
        (Json5, "{a: 1,\u{2028} a: 2}"),
        (Json5, "{\"é😀\": 1, \"é😀\": 2}"),
    ]));
}

#[test]
fn hex_numbers_overflow_exactly_where_a_double_does() {
    use jsonvet::{Rule, Severity};
    let config = Config::new(Dialect::Json5).rule(Rule::InvalidNumber, Severity::Error);
    let message = |hex: String| {
        let text = format!("0x{hex}");
        let report = check(&text, &config);
        report.problems().first().map(ToString::to_string)
    };
    let overflow = Some("number overflows a double".to_owned());
    let unsafe_integer =
        Some("integer beyond ±(2^53 − 1) cannot be represented exactly".to_owned());
    assert_eq!(
        message(format!("{}C{}", "F".repeat(13), "0".repeat(242))),
        overflow
    );
    assert_eq!(
        message(format!("{}b{}", "f".repeat(13), "f".repeat(242))),
        unsafe_integer
    );
    assert_eq!(message(format!("1{}", "0".repeat(256))), overflow);
    assert_eq!(message(format!("00{}", "F".repeat(14))), unsafe_integer);
    assert_eq!(message("1FFFFFFFFFFFFF".to_owned()), None);
}

#[test]
fn nesting_stops_at_the_depth_limit() {
    let nested = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let config = Config::new(Dialect::Json);
    assert!(check(&nested(Config::DEFAULT_MAX_DEPTH), &config).passed());
    let too_deep = nested(Config::DEFAULT_MAX_DEPTH + 1);
    let report = check(&too_deep, &config);
    let problem = &report.problems()[0];
    assert_eq!(problem.offset(), Config::DEFAULT_MAX_DEPTH);
    assert_eq!(problem.to_string(), "nested deeper than 10000 levels");
    let shallow = Config::new(Dialect::Json5).max_depth(2);
    assert!(check("{a: [1]}", &shallow).passed());
    assert!(!check("{a: [{}]}", &shallow).passed());
}
