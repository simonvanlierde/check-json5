use std::fmt;
use std::path::Path;
use std::str::FromStr;

/// A member of the JSON family. Each one only adds syntax to the one before it, in the order
/// JSON, JSONC, JWCC, JSON5; I-JSON is JSON with stricter content rules, and JSON Lines is one
/// JSON value per line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Dialect {
    /// JSON as RFC 8259 defines it.
    Json,
    /// I-JSON (RFC 7493): JSON whose duplicate keys, lone surrogates, noncharacters and
    /// out-of-range numbers are errors.
    IJson,
    /// JSON with `//` and `/* */` comments.
    Jsonc,
    /// JSON with comments and trailing commas, also called HuJSON.
    Jwcc,
    /// JSON5 1.0.0.
    Json5,
    /// JSON Lines: one JSON value per line, also called NDJSON.
    JsonLines,
}

impl Dialect {
    /// Every dialect, in the order the docs list them.
    pub const ALL: [Dialect; 6] = [
        Dialect::Json,
        Dialect::IJson,
        Dialect::Jsonc,
        Dialect::Jwcc,
        Dialect::Json5,
        Dialect::JsonLines,
    ];

    /// The name used on the command line and in config files, such as `jsonc`.
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Json => "json",
            Dialect::IJson => "i-json",
            Dialect::Jsonc => "jsonc",
            Dialect::Jwcc => "jwcc",
            Dialect::Json5 => "json5",
            Dialect::JsonLines => "jsonl",
        }
    }

    /// The default dialect for a file, from its name, or `None` for a file jsonvet does not
    /// check by default. `.json` files default to JSON5, because most `.json` files that fail
    /// as JSON are config files with comments.
    pub fn for_path(path: &Path) -> Option<Dialect> {
        let name = path.file_name()?.to_str()?;
        if name == ".babelrc" {
            return Some(Dialect::Json5);
        }
        let (_, ext) = name.rsplit_once('.')?;
        match ext {
            "json" | "json5" | "geojson" | "jsonld" | "webmanifest" => Some(Dialect::Json5),
            "jsonc" | "code-workspace" | "code-snippets" => Some(Dialect::Jsonc),
            "hujson" | "jwcc" => Some(Dialect::Jwcc),
            "jsonl" | "ndjson" => Some(Dialect::JsonLines),
            _ => None,
        }
    }

    pub(crate) fn comments(self) -> bool {
        matches!(self, Dialect::Jsonc | Dialect::Jwcc | Dialect::Json5)
    }

    pub(crate) fn trailing_commas(self) -> bool {
        matches!(self, Dialect::Jwcc | Dialect::Json5)
    }

    /// Whether a `//` comment ends only at `\n`. JWCC's text says `\r` bytes are irrelevant.
    pub(crate) fn line_comment_ends_only_at_lf(self) -> bool {
        self == Dialect::Jwcc
    }

    pub(crate) fn json5(self) -> bool {
        self == Dialect::Json5
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The error for a dialect name jsonvet does not know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownDialect(pub String);

impl fmt::Display for UnknownDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Dialect::ALL.iter().map(|d| d.name()).collect();
        write!(
            f,
            "unknown dialect `{}`; expected one of {}",
            self.0,
            names.join(", ")
        )
    }
}

impl std::error::Error for UnknownDialect {}

impl FromStr for Dialect {
    type Err = UnknownDialect;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ndjson" | "json-lines" => Ok(Dialect::JsonLines),
            "hujson" => Ok(Dialect::Jwcc),
            "ijson" => Ok(Dialect::IJson),
            _ => Dialect::ALL
                .into_iter()
                .find(|d| d.name() == s)
                .ok_or_else(|| UnknownDialect(s.to_owned())),
        }
    }
}
