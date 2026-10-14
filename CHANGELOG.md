# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Bump the minor
version for a new option or output, the patch version for a fix.

## [Unreleased]

### Added

- `--format sarif` prints one SARIF 2.1.0 document instead of text lines, for GitHub code
  scanning. A duplicate key also points at its first copy, and a file that cannot be read is a
  result under the rule `unreadable`. Exit codes and `--max-warnings` work as before.
- A GitHub Action: `uses: simonvanlierde/jsonvet@v2` checks the repository's tracked JSON files,
  or the `files` you give it, and can write a SARIF file for code scanning with `sarif:`. It
  installs jsonvet from PyPI and runs on Linux, macOS and Windows runners.

### Changed

- Unquoted JSON5 keys follow Unicode 17, up from 16, so they may use the letters it added, such
  as the 4,298 ideographs of CJK Extension J.

### Fixed

- In JWCC (HuJSON) files, a `//` comment ends only at a line feed, as the JWCC text says. A lone
  carriage return no longer ends it, so `[1, // c` + CR + `2]` fails as it should.

## [2.0.0] - 2026-10-13

check-json5 is now **jsonvet**, rewritten in Rust. It gives the same pass or fail as 1.4.1 for
every file in JSON5 and strict mode. I ran both on 20,114 files in each mode, 40,228 runs, and
they agree on every one.

Upgrade: set `rev:` to the new tag. The repository moved to `simonvanlierde/jsonvet`, and the old
URL still works. The `check-json5` hook id, the `check-json5` command and `--strict` keep working
until 3.0. To change anything else, switch to the `jsonvet` hook id.

### Added

- Dialects: `json`, `i-json`, `jsonc`, `jwcc` (HuJSON), `json5` and `jsonl` (JSON Lines), picked
  by file extension or with `--dialect DIALECT[:GLOB]`. `.json` files stay JSON5 by default.
- Rules with ids and severities: `syntax`, `duplicate-key`, `invalid-number`,
  `invalid-code-point`, `byte-order-mark` and `missing-final-newline`, changed with `--error`,
  `--warn` and `--ignore`, and `--max-warnings N`.
- Settings in `jsonvet.toml`, or `[tool.jsonvet]` in `pyproject.toml`, found up to the
  repository root.
- A duplicate key's report gives the position of the first copy too.
- The hook also checks `.jsonl` and `.ndjson` files.
- The `jsonvet` Rust library and the `jsonvet-cli` crate.

### Changed

- **Breaking:** each output line gains the severity and rule, as in
  `file:4:5: error[duplicate-key] duplicate key $.a (first at 2:5)`. Syntax errors use jsonvet's
  own wording. GitHub annotations are `::error` or `::warning` by severity.
- **Breaking:** lines end at every JSON5 line terminator (LF, CR, CRLF, U+2028 and U+2029), so
  positions after a lone CR, U+2028 or U+2029 differ from 1.x.
- **Breaking:** the importable Python module is gone. The wheel brings the `jsonvet` and
  `check-json5` commands. The `check-json5` package on PyPI now only installs `jsonvet`.
- The hook installs a prebuilt binary, from wheels for Linux, macOS and Windows on x86-64 and
  arm64. It needs no Rust, and no longer starts Python on every run.
- Raw U+2028 and U+2029 inside strings pass in every JSON5 file, as the spec allows. 1.x
  rejected them in files that used more of JSON5 than comments and trailing commas.
- Nesting deeper than 10,000 levels is a syntax error, about where 1.4 stopped (9,998 levels).
  Library users can change it with `Config::max_depth`.
- Numbers of any length pass; 1.x capped integers at 1,000,000 digits.
- A `\u` escape in an unquoted JSON5 key fails when the character it stands for could not appear
  there literally, as in `{\u0301: 1}`. 1.x passed any escape.

## [1.4.1] - 2026-10-13

Upgrade: set `rev: v1.4.1`. Nothing else is needed.

### Fixed

- `--strict` passed a file where `NaN` or `Infinity` runs on into other text, such as
  `[NaN-2.5e3` or `Infinitynul }`. Those files now fail, with the constant's position.
- An unquoted JSON5 key written with a `\u` escape, such as `\u0061`, was compared with its
  escape still in place, so it did not match `a` or `'a'`, and its path showed the escape. Keys
  now compare by their decoded name.

## [1.4.0] - 2026-10-13

Upgrade: set `rev: v1.4.0`. Nothing else is needed.

### Added

- The hook also runs on Tailscale's `.hujson` and `.jwcc`, VS Code's `.code-workspace` and
  `.code-snippets`, `.webmanifest` and `.babelrc` files: all JSON, some with comments.

### Changed

- The hook checks JSON with only comments and trailing commas, such as `tsconfig.json`, with the
  C-speed `json` parser instead of `json5`. Whole hook runs are 11× faster on a 50 KB file and
  56× faster on a 2 MB one. Files that use more of JSON5 still go through `json5`.
- A number may have up to 1,000,000 digits. 1.3.0 lifted Python's limit of 4300 altogether,
  which let one file with a huge number stall a run. A longer number now fails its file with a
  message.

### Fixed

- A JSON5 file with comments and an unescaped U+2028 or U+2029 inside a string now passes, as
  the JSON5 spec allows. The `json5` package rejected it; strict JSON files already passed.
- A file with a duplicate key and an undecodable string after it crashed the run with a
  traceback and left the remaining files unchecked. It now reports the duplicate.
- A newline in a file name is printed as `\n`, so a file name can no longer inject a GitHub
  Actions workflow command into CI output.
- Checking a file with many duplicate keys took time that grew with the square of their
  number. 200,000 duplicates took 42 s; they now take under a second.
- `--strict` patterns match `./sub/a.json` like `sub/a.json`.

## [1.3.0] - 2026-10-13

Upgrade: set `rev: v1.3.0`. Nothing else is needed for the hook. Only a script that parses the
hook's output, or imports `pre_commit_hooks.check_json5`, needs a change (see Changed).

### Added

- Every failure prints as `file:line:col: message`, so editors and terminals link to it.
- A duplicate key reports its JSON path (`$.server.port`), and every repeated key in a file is
  listed, not only the first.
- On GitHub Actions, each failure is also an annotation on the pull request diff.
- `--strict GLOB` holds matching files to strict JSON: no comments, trailing commas or `NaN`.
  Repeat it to add patterns, and `!GLOB` exempts files.
- Published on PyPI, so `uvx check-json5 FILE...` works without pre-commit.

### Changed

- **Breaking:** the output format changed. `f.json: Failed to json decode (<string>:4
  Unexpected "," at column 8)` is now `f.json:4:8: Unexpected ","`.
- **Breaking:** the module moved from `pre_commit_hooks.check_json5` to
  `check_json5.check_json5`, so the PyPI package no longer collides with `pre-commit-hooks`. The
  `check-json5` command is unchanged.
- Tested on Python 3.15.

### Fixed

- A file nested deeper than Python's recursion limit crashed the run and left the remaining files
  unchecked. It now fails on its own.
- Valid JSON with an integer of more than 4300 digits failed.

## [1.2.1] - 2026-10-11

### Changed

- Faster start: a run over strict JSON no longer imports `json5`, `argparse`, `pathlib` or
  `typing`.

## [1.2.0] - 2026-07-06

### Added

- Strict JSON is parsed with the stdlib `json` parser first, falling back to `json5` only for
  files that use JSON5 features: about 500× faster on large files.
- The hook also runs on `.jsonc`, `.geojson` and `.jsonld` files.

## [1.1.0] - 2025-09-26

### Changed

- Packaging moved from Poetry to uv.

## [1.0.1] - 2025-12-26

### Changed

- No upper bound on the `json5` dependency.
- The hook also runs on `.json5` files.

## [1.0.0] - 2021-12-04

### Changed

- Renamed from `pre-commit-check-json5` to `check-json5`.

[Unreleased]: https://github.com/simonvanlierde/jsonvet/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/simonvanlierde/jsonvet/compare/v1.4.1...v2.0.0
[1.4.1]: https://github.com/simonvanlierde/jsonvet/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/simonvanlierde/jsonvet/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/simonvanlierde/jsonvet/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/simonvanlierde/jsonvet/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/simonvanlierde/jsonvet/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/simonvanlierde/jsonvet/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/simonvanlierde/jsonvet/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/simonvanlierde/jsonvet/releases/tag/v1.0.0
