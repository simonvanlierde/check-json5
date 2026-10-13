use std::fmt;
use std::str::FromStr;

use crate::Dialect;

/// What jsonvet checks. Every problem belongs to one rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Rule {
    /// The first syntax error. Checking stops there, and its severity is always error.
    Syntax,
    /// A key repeated in the same object.
    DuplicateKey,
    /// A number outside a double's range, or an integer past ±(2⁵³ − 1).
    InvalidNumber,
    /// A lone surrogate or a noncharacter in a string.
    InvalidCodePoint,
    /// A byte order mark at the start of the file.
    ByteOrderMark,
    /// A JSON Lines file that does not end with a newline.
    MissingFinalNewline,
}

impl Rule {
    /// Every rule, in the order the docs list them.
    pub const ALL: [Rule; 6] = [
        Rule::Syntax,
        Rule::DuplicateKey,
        Rule::InvalidNumber,
        Rule::InvalidCodePoint,
        Rule::ByteOrderMark,
        Rule::MissingFinalNewline,
    ];

    /// The rule's id, such as `duplicate-key`.
    pub fn id(self) -> &'static str {
        match self {
            Rule::Syntax => "syntax",
            Rule::DuplicateKey => "duplicate-key",
            Rule::InvalidNumber => "invalid-number",
            Rule::InvalidCodePoint => "invalid-code-point",
            Rule::ByteOrderMark => "byte-order-mark",
            Rule::MissingFinalNewline => "missing-final-newline",
        }
    }

    /// The rule's severity when nothing overrides it.
    pub fn default_severity(self, dialect: Dialect) -> Severity {
        match (self, dialect) {
            (Rule::Syntax | Rule::DuplicateKey, _)
            | (Rule::InvalidNumber | Rule::InvalidCodePoint, Dialect::IJson)
            | (Rule::ByteOrderMark, Dialect::JsonLines) => Severity::Error,
            (Rule::ByteOrderMark, Dialect::Json | Dialect::IJson)
            | (Rule::MissingFinalNewline, Dialect::JsonLines) => Severity::Warning,
            _ => Severity::Off,
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// The error for a rule id jsonvet does not know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownRule(pub String);

impl fmt::Display for UnknownRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids: Vec<&str> = Rule::ALL.iter().map(|r| r.id()).collect();
        write!(
            f,
            "unknown rule `{}`; expected one of {}",
            self.0,
            ids.join(", ")
        )
    }
}

impl std::error::Error for UnknownRule {}

impl FromStr for Rule {
    type Err = UnknownRule;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Rule::ALL
            .into_iter()
            .find(|r| r.id() == s)
            .ok_or_else(|| UnknownRule(s.to_owned()))
    }
}

/// How much a problem matters. An ignored (`Off`) rule is not checked at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Not checked.
    Off,
    /// Reported, but does not fail the run unless a warning limit is set.
    Warning,
    /// Reported, and fails the run.
    Error,
}

impl Severity {
    /// The name used in output and config files: `off`, `warning` or `error`.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Off => "off",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    /// True unless the rule is `Off`, so its problems get checked and reported.
    pub(crate) fn is_on(self) -> bool {
        self != Severity::Off
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What to check: a dialect, and any rules whose severity differs from the dialect's default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    dialect: Dialect,
    overrides: [Option<Severity>; Rule::ALL.len()],
}

impl Config {
    /// The dialect's defaults.
    pub fn new(dialect: Dialect) -> Self {
        Config {
            dialect,
            overrides: [None; Rule::ALL.len()],
        }
    }

    /// Set one rule's severity. `Rule::Syntax` stays an error whatever is set here.
    #[must_use]
    pub fn rule(mut self, rule: Rule, severity: Severity) -> Self {
        self.overrides[rule as usize] = Some(severity);
        self
    }

    /// The dialect being checked.
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// A rule's severity under this config.
    pub fn severity(&self, rule: Rule) -> Severity {
        if rule == Rule::Syntax {
            return Severity::Error;
        }
        self.overrides[rule as usize].unwrap_or_else(|| rule.default_severity(self.dialect))
    }
}
