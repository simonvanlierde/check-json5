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
mod event;
mod parse;
mod path;
mod problem;
mod report;
mod rule;

pub use dialect::{Dialect, UnknownDialect};
pub use event::{Event, EventKind};
pub use problem::Problem;
pub use report::{Location, Report};
pub use rule::{Config, Rule, Severity, UnknownRule};

/// Check one text and return every problem in it, in document order. Checking stops at the
/// first syntax error; problems found before it stay in the report.
pub fn check<'a>(text: &'a str, config: &Config) -> Report<'a> {
    Report::new(text, parse::check(text, config, false).0)
}

/// Check one text as [`check`] does, and also return its values as events, unless the text has
/// a syntax error.
///
/// ```
/// use jsonvet::{parse, Config, Dialect, EventKind};
///
/// let (report, events) = parse("{a: 0x10} // c", &Config::new(Dialect::Json5));
/// assert!(report.passed());
/// let kinds: Vec<EventKind> = events.unwrap().into_iter().map(|e| e.kind).collect();
/// assert_eq!(kinds[2], EventKind::Number("0x10"));
/// ```
pub fn parse<'a>(text: &'a str, config: &Config) -> (Report<'a>, Option<Vec<Event<'a>>>) {
    let (problems, events) = parse::check(text, config, true);
    let syntax_error = problems.iter().any(|p| p.rule() == Rule::Syntax);
    (
        Report::new(text, problems),
        (!syntax_error).then_some(events),
    )
}
