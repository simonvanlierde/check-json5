//! Any text, every dialect: no panic, every offset inside the text and on a character boundary,
//! every location equal to a naive recount, problems in offset order, `passed` meaning no error,
//! every problem printable, and a text that passes in a narrower dialect passing in the wider
//! ones. One more JSON5 run reaches what the defaults never do: the number check, which is off by
//! default, and the depth limit, which 4 KB inputs cannot hit.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

#[path = "../../crates/jsonvet/tests/common/mod.rs"]
mod common;

fuzz_target!(|text: &str| {
    let strict = Config::new(Dialect::Json5)
        .rule(Rule::InvalidNumber, Severity::Error)
        .max_depth(8);
    let configs: Vec<Config> = Dialect::ALL.iter().map(|&d| Config::new(d)).collect();
    for config in configs.iter().chain([&strict]) {
        let dialect = config.dialect();
        let report = check(text, config);
        let mut last = 0;
        for problem in report.problems() {
            assert!(problem.offset() >= last, "{dialect}: problems out of order");
            last = problem.offset();
            for offset in std::iter::once(problem.offset()).chain(problem.related_offset()) {
                assert!(
                    offset <= text.len(),
                    "{dialect}: offset {offset} past the end"
                );
                assert!(
                    text.is_char_boundary(offset),
                    "{dialect}: offset {offset} inside a character"
                );
                let at = report.location(offset);
                assert_eq!(
                    (at.line, at.column),
                    common::recount(text, offset),
                    "{dialect}: wrong location for offset {offset}"
                );
            }
            // The text output and the sarif/GitHub formats are built from this.
            assert!(!problem.to_string().is_empty());
        }
        assert_eq!(
            report.passed(),
            report
                .problems()
                .iter()
                .all(|p| p.severity() != Severity::Error),
            "{dialect}: `passed` disagrees with the problems"
        );
    }
    let broken = common::nesting_violations(text);
    assert!(broken.is_empty(), "{text:?}: {broken:?}");
});
