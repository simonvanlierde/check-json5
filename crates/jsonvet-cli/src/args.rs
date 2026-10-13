//! The command line.

use std::ffi::OsString;
use std::path::PathBuf;

use jsonvet::Rule;
use lexopt::prelude::*;

use crate::settings::{DialectRule, Settings, parse_severity};

pub(crate) const HELP: &str = "\
Check JSON, I-JSON, JSONC, JWCC (HuJSON), JSON5 and JSON Lines files for syntax errors,
duplicate keys and the rules of each dialect.

Usage: jsonvet [OPTIONS] [FILE]...

Options:
      --dialect DIALECT[:GLOB]  Check files matching GLOB (every file, without one) as DIALECT:
                                json, i-json, jsonc, jwcc, json5, jsonl, or auto for the file
                                extension's default. Repeatable; the last match wins.
      --strict GLOB             Same as --dialect json:GLOB; --strict '!GLOB' means auto:GLOB.
      --error RULE              Report RULE as an error. Repeatable, as are the next two.
      --warn RULE               Report RULE as a warning.
      --ignore RULE             Do not check RULE.
      --max-warnings N          Fail when there are more than N warnings.
      --config FILE             Read settings from FILE instead of searching for one.
      --no-config               Do not read a settings file.
  -h, --help                    Print this help.
  -V, --version                 Print the version.

Rules: syntax, duplicate-key, invalid-number, invalid-code-point, byte-order-mark,
missing-final-newline.

Settings come from jsonvet.toml, or [tool.jsonvet] in pyproject.toml, found in the working
directory or a parent up to the repository root. Options on the command line win.

Exit status: 0 when no file has an error, 1 when one does, 2 for a usage error.";

/// Where settings come from besides the command line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfigSource {
    Search,
    File(PathBuf),
    None,
}

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) files: Vec<OsString>,
    pub(crate) settings: Settings,
    pub(crate) max_warnings: Option<usize>,
    pub(crate) config: ConfigSource,
}

#[derive(Debug)]
pub(crate) enum Command {
    Check(Args),
    Help,
    Version,
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut parser = lexopt::Parser::from_args(args);
    let mut files = Vec::new();
    let mut settings = Settings::default();
    let mut max_warnings = None;
    let mut config = ConfigSource::Search;
    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        match arg {
            Long("dialect") => settings
                .dialects
                .push(DialectRule::parse(&value(&mut parser)?)?),
            Long("strict") => {
                let glob = value(&mut parser)?;
                settings.dialects.push(match glob.strip_prefix('!') {
                    Some(glob) => DialectRule::parse(&format!("auto:{glob}"))?,
                    None => DialectRule::parse(&format!("json:{glob}"))?,
                });
            }
            Long(severity @ ("error" | "warn" | "ignore")) => {
                let severity = parse_severity(severity)?;
                let rule: Rule = value(&mut parser)?.parse().map_err(|e| format!("{e}"))?;
                settings.rules.push((rule, severity));
            }
            Long("max-warnings") => {
                let n = value(&mut parser)?;
                max_warnings = Some(
                    n.parse()
                        .map_err(|_| format!("--max-warnings takes a number, not `{n}`"))?,
                );
            }
            Long("config") => config = ConfigSource::File(PathBuf::from(value(&mut parser)?)),
            Long("no-config") => config = ConfigSource::None,
            Value(file) => files.push(file),
            // pre-commit puts file names after the options with no `--`, so a name like
            // `-x.json`, `-h.json` or `--y.json` is a file, not an option.
            Short(c) => {
                let rest = rest(&mut parser);
                match (c, rest.is_empty()) {
                    ('h', true) => return Ok(Command::Help),
                    ('V', true) => return Ok(Command::Version),
                    _ => files.push(joined(&format!("-{c}"), "", &rest)),
                }
            }
            Long(name) => {
                let name = name.to_owned();
                let rest = rest(&mut parser);
                match (name.as_str(), rest.is_empty()) {
                    ("help", true) => return Ok(Command::Help),
                    ("version", true) => return Ok(Command::Version),
                    (_, true) => files.push(format!("--{name}").into()),
                    _ => files.push(joined(&format!("--{name}"), "=", &rest)),
                }
            }
        }
    }
    Ok(Command::Check(Args {
        files,
        settings,
        max_warnings,
        config,
    }))
}

fn value(parser: &mut lexopt::Parser) -> Result<String, String> {
    parser
        .value()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|v| format!("not valid UTF-8: {}", v.to_string_lossy()))
}

/// The rest of the current argument after an unknown option: the remaining letters of
/// `-dash.json`, or the value of `--name=value`.
fn rest(parser: &mut lexopt::Parser) -> OsString {
    parser.optional_value().unwrap_or_default()
}

fn joined(option: &str, separator: &str, rest: &OsString) -> OsString {
    let mut name = OsString::from(option);
    name.push(separator);
    name.push(rest);
    name
}

#[cfg(test)]
mod tests {
    use super::{Command, ConfigSource, parse};
    use jsonvet::{Rule, Severity};

    fn check(args: &[&str]) -> super::Args {
        match parse(args.iter().map(Into::into)) {
            Ok(Command::Check(args)) => args,
            other => panic!("expected a check, got {other:?}"),
        }
    }

    #[test]
    fn files_that_look_like_options_stay_files() {
        let args = check(&[
            "-dash.json",
            "-h.json",
            "--help=x",
            "--x.json",
            "--",
            "--y.json",
            "a.json",
        ]);
        assert_eq!(
            args.files,
            [
                "-dash.json",
                "-h.json",
                "--help=x",
                "--x.json",
                "--y.json",
                "a.json"
            ]
        );
        assert!(matches!(
            parse(["a.json", "-h"].iter().map(Into::into)),
            Ok(Command::Help)
        ));
        assert!(matches!(
            parse(["--version"].iter().map(Into::into)),
            Ok(Command::Version)
        ));
    }

    #[test]
    fn strict_is_an_alias_for_dialect_json() {
        let args = check(&["--strict", "*.json", "--strict", "!tsconfig.json"]);
        let config = args.settings.config_for("a/package.json");
        assert_eq!(config.dialect(), jsonvet::Dialect::Json);
        assert_eq!(
            args.settings.config_for("tsconfig.json").dialect(),
            jsonvet::Dialect::Json5
        );
    }

    #[test]
    fn options_parse() {
        let args = check(&[
            "--warn=duplicate-key",
            "--max-warnings",
            "3",
            "--no-config",
            "x.jsonc",
        ]);
        assert_eq!(
            args.settings.rules,
            [(Rule::DuplicateKey, Severity::Warning)]
        );
        assert_eq!(args.max_warnings, Some(3));
        assert_eq!(args.config, ConfigSource::None);
        assert!(parse(["--error", "nope"].iter().map(Into::into)).is_err());
        assert!(parse(["--dialect", "yaml"].iter().map(Into::into)).is_err());
        assert!(parse(["--strict"].iter().map(Into::into)).is_err());
    }
}
