//! Any text, every dialect: no panic, every offset inside the text and on a character boundary,
//! and every location computable. One more JSON5 run reaches what the defaults never do: the
//! number check, which is off by default, and the depth limit, which 4 KB inputs cannot hit.
#![no_main]

use jsonvet::{Config, Dialect, Rule, Severity, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let strict = Config::new(Dialect::Json5)
        .rule(Rule::InvalidNumber, Severity::Error)
        .max_depth(8);
    let configs: Vec<Config> = Dialect::ALL.iter().copied().map(Config::new).collect();
    for config in configs.iter().chain([&strict]) {
        let dialect = config.dialect();
        let report = check(text, config);
        for problem in report.problems() {
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
                assert!(at.line >= 1 && at.column >= 1);
            }
        }
    }
});
