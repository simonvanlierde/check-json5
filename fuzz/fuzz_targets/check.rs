//! Any text, every dialect: no panic, every report keeping the promises
//! `common::report_violation` lists, and a text that passes in a narrower dialect passing in the
//! wider ones. One more run with `common::strict` reaches the number check, which is off by
//! default, and the depth limit, which 4 KB inputs cannot hit.
#![no_main]

use jsonvet::{Config, Dialect};
use libfuzzer_sys::fuzz_target;

#[path = "../../crates/jsonvet/tests/common/mod.rs"]
mod common;

fuzz_target!(|text: &str| {
    let configs = Dialect::ALL.iter().copied().map(Config::new);
    for config in configs.chain([common::strict()]) {
        if let Err(broken) = common::report_violation(text, &config) {
            panic!("{text:?}: {broken}");
        }
    }
    let broken = common::nesting_violations(text);
    assert!(broken.is_empty(), "{text:?}: {broken:?}");
});
