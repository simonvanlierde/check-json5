//! Properties that hold for every input in every dialect: no panic, every offset inside the
//! text and on a character boundary, and every location equal to a naive recount.

use jsonvet::{Config, Dialect, EventKind, Rule, check, parse};
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

/// A value as text, with the events it should parse into, written as short tokens.
type Piece = (String, Vec<String>);

/// What a dialect adds to JSON, for [`well_formed`].
#[derive(Clone, Copy)]
struct Syntax {
    comments: bool,
    trailing_commas: bool,
    json5: bool,
    /// JSON Lines: no line breaks inside a document.
    one_line: bool,
}

impl Syntax {
    fn of(dialect: Dialect) -> Syntax {
        let at_least = |d: &[Dialect]| d.contains(&dialect);
        Syntax {
            comments: at_least(&[Dialect::Jsonc, Dialect::Jwcc, Dialect::Json5]),
            trailing_commas: at_least(&[Dialect::Jwcc, Dialect::Json5]),
            json5: dialect == Dialect::Json5,
            one_line: dialect == Dialect::JsonLines,
        }
    }
}

/// Space between tokens: whitespace, and comments where the dialect allows them.
fn gap(syntax: Syntax) -> BoxedStrategy<&'static str> {
    let mut gaps = vec!["", " "];
    if !syntax.one_line {
        gaps.push("\n\t");
    }
    if syntax.comments {
        gaps.extend(["/* c */", "// c\n"]);
    }
    prop::sample::select(gaps).boxed()
}

/// One value the dialect accepts: nested arrays and objects, escaped and repeated keys,
/// and JSON5's own numbers, strings and keys.
fn value(syntax: Syntax) -> BoxedStrategy<Piece> {
    let mut numbers = vec!["0", "-1", "2.5e3", "-0.5E-2", "18446744073709551616"];
    let mut strings = vec![
        ("\"a\"", "a"),
        ("\"\"", ""),
        ("\"\\n\\u00e9\\/\"", "\n\u{e9}/"),
        ("\"é\\\"\"", "é\""),
        ("\"\\ud83d\\ude00\"", "\u{1F600}"),
    ];
    if syntax.json5 {
        numbers.extend(["+1", "0x1F", "-0XaB", ".5", "5.", "Infinity", "-NaN"]);
        strings.extend([("'q\"'", "q\""), ("'\\x41'", "A")]);
    }
    let mut keys = strings.clone();
    // Repeated keys, one of them spelled with an escape.
    keys.extend([("\"a\"", "a"), ("\"\\u0061\"", "a")]);
    if syntax.json5 {
        keys.extend([("id", "id"), ("$a\\u0062", "$ab")]);
    }
    let leaf = prop_oneof![
        prop::sample::select(vec!["null", "true", "false"])
            .prop_map(|word| (word.to_owned(), vec![word.to_owned()])),
        prop::sample::select(numbers).prop_map(|n| (n.to_owned(), vec![format!("n:{n}")])),
        prop::sample::select(strings)
            .prop_map(|(text, s)| (text.to_owned(), vec![format!("s:{s}")])),
    ];
    leaf.prop_recursive(4, 32, 4, move |inner| {
        let array = (
            prop::collection::vec((gap(syntax), inner.clone()), 0..4),
            any::<bool>(),
        )
            .prop_map(move |(items, trailing)| {
                let mut piece = ("[".to_owned(), vec!["[".to_owned()]);
                for (i, (gap, (text, tokens))) in items.iter().enumerate() {
                    if i > 0 {
                        piece.0.push(',');
                    }
                    piece.0.push_str(gap);
                    piece.0.push_str(text);
                    piece.1.extend(tokens.iter().cloned());
                }
                close(
                    &mut piece,
                    "]",
                    syntax.trailing_commas && trailing && !items.is_empty(),
                );
                piece
            });
        let object = (
            prop::collection::vec(
                (
                    gap(syntax),
                    prop::sample::select(keys.clone()),
                    gap(syntax),
                    inner,
                ),
                0..4,
            ),
            any::<bool>(),
        )
            .prop_map(move |(members, trailing)| {
                let mut piece = ("{".to_owned(), vec!["{".to_owned()]);
                for (i, (before, (key, decoded), after, (text, tokens))) in
                    members.iter().enumerate()
                {
                    if i > 0 {
                        piece.0.push(',');
                    }
                    piece
                        .0
                        .push_str(&format!("{before}{key}{after}:{after}{text}"));
                    piece.1.push(format!("k:{decoded}"));
                    piece.1.extend(tokens.iter().cloned());
                }
                close(
                    &mut piece,
                    "}",
                    syntax.trailing_commas && trailing && !members.is_empty(),
                );
                piece
            });
        prop_oneof![array, object]
    })
    .boxed()
}

fn close(piece: &mut Piece, bracket: &str, trailing_comma: bool) {
    if trailing_comma {
        piece.0.push(',');
    }
    piece.0.push_str(bracket);
    piece.1.push("end".to_owned());
}

/// A whole text the dialect accepts, with its events as tokens: one value, or one per line in
/// JSON Lines.
fn well_formed() -> impl Strategy<Value = (Dialect, String, Vec<String>)> {
    prop::sample::select(Dialect::ALL.to_vec()).prop_flat_map(|dialect| {
        let syntax = Syntax::of(dialect);
        let count = if syntax.one_line { 1..4 } else { 1..2 };
        (
            prop::collection::vec((gap(syntax), value(syntax), gap(syntax)), count),
            Just(dialect),
        )
            .prop_map(|(documents, dialect)| {
                let mut text = Vec::new();
                let mut tokens = Vec::new();
                for (before, (value, events), after) in documents {
                    text.push(format!("{before}{value}{after}"));
                    tokens.extend(events);
                }
                (dialect, text.join("\n") + "\n", tokens)
            })
    })
}

/// An event as a [`Piece`] token.
fn token(kind: &EventKind<'_>) -> String {
    match kind {
        EventKind::ObjectStart => "{".to_owned(),
        EventKind::ArrayStart => "[".to_owned(),
        EventKind::End => "end".to_owned(),
        EventKind::Key(key) => format!("k:{key}"),
        EventKind::Null => "null".to_owned(),
        EventKind::Bool(b) => b.to_string(),
        EventKind::Number(n) => format!("n:{n}"),
        EventKind::String(s) => format!("s:{s}"),
        _ => "?".to_owned(),
    }
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
        // Again backwards: `location` counts from its last answer when that is nearer.
        for problem in report.problems().iter().rev() {
            let at = report.location(problem.offset());
            prop_assert_eq!((at.line, at.column), recount(&text, problem.offset()));
        }
    }

    #[test]
    fn parse_reports_as_check_does_and_its_events_balance(text in soup(), dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        let config = Config::new(dialect);
        let (report, events) = parse(&text, &config);
        let checked = check(&text, &config);
        prop_assert_eq!(report.problems(), checked.problems());
        let syntax_error = report.problems().iter().any(|p| p.rule() == Rule::Syntax);
        prop_assert_eq!(events.is_none(), syntax_error);
        let mut depth = 0usize;
        for event in events.unwrap_or_default() {
            prop_assert!(text.is_char_boundary(event.offset) && event.offset < text.len());
            match event.kind {
                EventKind::ObjectStart | EventKind::ArrayStart => depth += 1,
                EventKind::End => depth -= 1,
                _ => {}
            }
        }
        prop_assert_eq!(depth, 0);
    }

    #[test]
    fn well_formed_text_parses_into_the_events_it_was_built_from((dialect, text, expected) in well_formed()) {
        let config = Config::new(dialect);
        let (report, events) = parse(&text, &config);
        let checked = check(&text, &config);
        prop_assert_eq!(report.problems(), checked.problems());
        prop_assert!(events.is_some(), "a syntax error: {:?}", report.problems());
        let events = events.unwrap_or_default();
        let tokens: Vec<String> = events.iter().map(|event| token(&event.kind)).collect();
        prop_assert_eq!(tokens, expected);
        for event in &events {
            prop_assert!(text.is_char_boundary(event.offset) && event.offset < text.len());
        }
    }

    #[test]
    fn arbitrary_text_never_panics(text in "\\PC{0,64}", dialect in prop::sample::select(Dialect::ALL.to_vec())) {
        let _ = check(&text, &Config::new(dialect));
    }
}
