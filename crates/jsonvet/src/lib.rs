//! Checks files in the JSON family for syntax errors, duplicate keys and the rules of each
//! dialect: JSON, I-JSON, JSONC, JWCC (HuJSON), JSON5 and JSON Lines.
//!
//! ```
//! use jsonvet::{check, Config, Dialect, Rule};
//!
//! let text = "{\n  // port\n  \"port\": 80,\n  \"port\": 8080,\n}\n";
//! let report = check(text, &Config::new(Dialect::Json5));
//! let problem = &report.problems()[0];
//! assert_eq!(problem.rule(), Rule::DuplicateKey);
//! assert_eq!(problem.to_string(), "duplicate key $.port");
//! let at = report.location(problem.offset());
//! assert_eq!((at.line, at.column), (4, 3));
//! ```
//!
//! `check` never panics and has no error mode: the problems in the text are its output.

mod dialect;
mod parse;
mod path;
mod problem;
mod report;
mod rule;

pub use dialect::{Dialect, UnknownDialect};
pub use problem::Problem;
pub use report::{Location, Report};
pub use rule::{Config, Rule, Severity, UnknownRule};

/// Check one text and return every problem in it, in document order. Checking stops at the
/// first syntax error; problems found before it stay in the report.
pub fn check<'a>(text: &'a str, config: &Config) -> Report<'a> {
    Report::new(text, parse::check(text, config))
}
