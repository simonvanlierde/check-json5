//! What to check for each file: the dialect patterns and rule severities from the config file
//! and the command line, in that order, so the command line wins.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jsonvet::{Config, Dialect, Rule, Severity};
use serde::Deserialize;

use crate::glob;

/// A dialect for files matching a pattern, or for every file when the pattern is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DialectPattern {
    pub(crate) files: Option<String>,
    /// `None` means `auto`: the default for the file's extension.
    pub(crate) dialect: Option<Dialect>,
}

impl DialectPattern {
    /// Parse `DIALECT` or `DIALECT:GLOB`, where `DIALECT` may be `auto`.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        let (name, files) = match value.split_once(':') {
            Some((name, files)) => (name, Some(files.to_owned())),
            None => (value, None),
        };
        Ok(DialectPattern {
            files,
            dialect: parse_dialect(name)?,
        })
    }
}

fn parse_dialect(name: &str) -> Result<Option<Dialect>, String> {
    if name == "auto" {
        return Ok(None);
    }
    name.parse()
        .map(Some)
        .map_err(|e: jsonvet::UnknownDialect| format!("{e}, or auto"))
}

pub(crate) fn parse_severity(name: &str) -> Result<Severity, String> {
    match name {
        "error" => Ok(Severity::Error),
        "warning" | "warn" => Ok(Severity::Warning),
        "ignore" | "off" => Ok(Severity::Off),
        _ => Err(format!(
            "unknown severity `{name}`; expected error, warning or ignore"
        )),
    }
}

/// Dialect patterns and rule severities, merged from every source in order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Settings {
    pub(crate) dialects: Vec<DialectPattern>,
    pub(crate) rules: Vec<(Rule, Severity)>,
}

impl Settings {
    /// Add `later` after `self`, so `later` wins where both say something.
    pub(crate) fn overridden_by(mut self, later: Settings) -> Settings {
        self.dialects.extend(later.dialects);
        self.rules.extend(later.rules);
        self
    }

    /// The checking config for one file: the last matching dialect pattern wins.
    pub(crate) fn config_for(&self, path: &str) -> Config {
        let auto = || Dialect::for_path(Path::new(path)).unwrap_or(Dialect::Json5);
        let dialect = self
            .dialects
            .iter()
            .rev()
            .find(|rule| rule.files.as_deref().is_none_or(|g| glob::matches(g, path)))
            .and_then(|rule| rule.dialect)
            .unwrap_or_else(auto);
        self.rules
            .iter()
            .fold(Config::new(dialect), |config, &(rule, severity)| {
                config.rule(rule, severity)
            })
    }
}

/// The `jsonvet.toml` file, or the `[tool.jsonvet]` table of a `pyproject.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSettings {
    #[serde(default)]
    rules: BTreeMap<String, String>,
    #[serde(default)]
    dialects: Vec<FileDialectEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDialectEntry {
    files: String,
    dialect: String,
}

#[derive(Deserialize)]
struct PyProject {
    tool: Option<PyProjectTool>,
}

#[derive(Deserialize)]
struct PyProjectTool {
    jsonvet: Option<FileSettings>,
}

/// The config file for a run started in `start`: the first `jsonvet.toml`, or `pyproject.toml`
/// with a `[tool.jsonvet]` table, in `start` and its parents. The search stops at the first
/// directory with a `.git` entry, so a file above the repository is never used.
pub(crate) fn discover(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let own = dir.join("jsonvet.toml");
        if own.is_file() {
            return Some(own);
        }
        let pyproject = dir.join("pyproject.toml");
        if fs::read_to_string(&pyproject).is_ok_and(|text| has_jsonvet_table(&text)) {
            return Some(pyproject);
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

/// Whether a `pyproject.toml` has a `[tool.jsonvet]` table, valid or not: `load` then reports
/// what is wrong with it, rather than the search skipping a table with a typo.
fn has_jsonvet_table(text: &str) -> bool {
    match toml::from_str::<toml::Table>(text) {
        Ok(table) => table
            .get("tool")
            .and_then(|tool| tool.get("jsonvet"))
            .is_some(),
        Err(_) => text.contains("[tool.jsonvet"),
    }
}

/// Read a config file.
pub(crate) fn load(path: &Path) -> Result<Settings, String> {
    let in_file = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let text = fs::read_to_string(path).map_err(|e| in_file(&e))?;
    let at = |e: toml::de::Error| in_file(&e.message());
    let file = if path.file_name().is_some_and(|n| n == "pyproject.toml") {
        toml::from_str::<PyProject>(&text)
            .map_err(at)?
            .tool
            .and_then(|t| t.jsonvet)
            .ok_or_else(|| in_file(&"no [tool.jsonvet] table"))?
    } else {
        toml::from_str::<FileSettings>(&text).map_err(at)?
    };
    let mut settings = Settings::default();
    for (id, severity) in &file.rules {
        let rule: Rule = id.parse().map_err(|e| in_file(&e))?;
        let severity = parse_severity(severity).map_err(|e| in_file(&e))?;
        settings.rules.push((rule, severity));
    }
    for entry in file.dialects {
        let dialect = parse_dialect(&entry.dialect).map_err(|e| in_file(&e))?;
        settings.dialects.push(DialectPattern {
            files: Some(entry.files),
            dialect,
        });
    }
    Ok(settings)
}
