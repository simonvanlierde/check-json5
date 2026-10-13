# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Bump the minor
version for a new option or output, the patch version for a fix.

## [Unreleased]

### Added

- The hook also runs on Tailscale's `.hujson` and `.jwcc`, VS Code's `.code-workspace` and
  `.code-snippets`, `.webmanifest` and `.babelrc` files: all JSON, some with comments.

### Changed

- The hook checks JSON with only comments and trailing commas, such as `tsconfig.json`, with the
  C-speed `json` parser instead of `json5`. Whole hook runs are 11× faster on a 50 KB file and
  56× faster on a 2 MB one. Files that use more of JSON5 still go through `json5`.

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

[Unreleased]: https://github.com/simonvanlierde/check-json5/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/simonvanlierde/check-json5/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/simonvanlierde/check-json5/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/simonvanlierde/check-json5/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/simonvanlierde/check-json5/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/simonvanlierde/check-json5/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/simonvanlierde/check-json5/releases/tag/v1.0.0
