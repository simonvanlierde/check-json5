//! Any text, every dialect: no panic, every offset inside the text and on a character boundary,
//! and every location computable.
#![no_main]

use jsonvet::{Config, Dialect, check};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    for dialect in Dialect::ALL {
        let report = check(text, &Config::new(dialect));
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
