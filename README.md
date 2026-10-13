# jsonvet

[![CI](https://github.com/simonvanlierde/jsonvet/actions/workflows/ci.yml/badge.svg)](https://github.com/simonvanlierde/jsonvet/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/simonvanlierde/jsonvet/branch/main/graph/badge.svg)](https://codecov.io/gh/simonvanlierde/jsonvet)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](https://github.com/pre-commit/pre-commit)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![release](https://img.shields.io/github/v/release/simonvanlierde/jsonvet)](https://github.com/simonvanlierde/jsonvet/releases/latest)

jsonvet checks JSON, I-JSON, JSONC, HuJSON (JWCC), JSON5 and JSON Lines files for syntax errors,
duplicate keys and the rules of each dialect. Every problem comes with its file, line, column and
rule, and a duplicate key with the JSONPath and position of both copies:

```text
config.json:4:5: error[duplicate-key] duplicate key $.server.port (first at 2:5)
package.json:3:1: error[syntax] comments are not allowed in json
events.jsonl:12:1: error[syntax] blank line; JSON Lines needs a value on every line
```

It is a pre-commit and prek hook, a command, and a Rust library. Until 2.0 it was check-json5, a
Python hook; 2.0 is a rewrite in Rust that keeps its verdicts (see
[Coming from check-json5](#coming-from-check-json5)).

## Use it as a hook

In `.pre-commit-config.yaml` (pre-commit or [prek](https://github.com/j178/prek)):

```yaml
- repo: https://github.com/simonvanlierde/jsonvet
  rev: v2.0.0
  hooks:
    - id: jsonvet
```

The hook installs a prebuilt binary from PyPI, so it needs no Rust. It checks files by name:
`.json`, `.jsonc`, `.json5`, `.jsonl`, `.ndjson`, `.geojson`, `.jsonld`, `.hujson`, `.jwcc`,
`.code-workspace`, `.code-snippets`, `.webmanifest` and `.babelrc`. In GitHub Actions each problem
also becomes an annotation on the pull request.

## Dialects

Each file is checked in the dialect its extension implies, and `--dialect` or the config file can
choose another. Each step in JSON, JSONC, JWCC, JSON5 only adds syntax to the one before.

| Dialect  | Adds to JSON                                                                 | Default for                                                        |
| -------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `json`   | nothing ([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259))                  | none; choose it with `--dialect` or `--strict`                     |
| `i-json` | stricter content ([RFC 7493](https://www.rfc-editor.org/rfc/rfc7493)): lone surrogates, noncharacters and numbers a double cannot hold are errors | none |
| `jsonc`  | `//` and `/* */` comments                                                    | `.jsonc`, `.code-workspace`, `.code-snippets`                      |
| `jwcc`   | comments and trailing commas ([HuJSON](https://github.com/tailscale/hujson)) | `.hujson`, `.jwcc`                                                 |
| `json5`  | the [JSON5 1.0.0](https://spec.json5.org/) grammar                            | `.json`, `.json5`, `.geojson`, `.jsonld`, `.webmanifest`, `.babelrc` |
| `jsonl`  | one JSON value per line ([JSON Lines](https://jsonlines.org/))               | `.jsonl`, `.ndjson`                                                |

`.json` files default to JSON5, because most `.json` files that fail as JSON are config files
with comments. Hold the ones a strict parser reads, such as `package.json`, to JSON with
`--dialect json:package.json`.

## Rules

| Rule                    | Reports                                                       | Default                                          |
| ----------------------- | ------------------------------------------------------------- | ------------------------------------------------ |
| `syntax`                | the first syntax error; checking stops there                  | error                                            |
| `duplicate-key`         | a key repeated in one object, with both positions and its path | error                                            |
| `invalid-number`        | a number a double cannot hold, or an integer past ±(2⁵³ − 1)   | error in `i-json`, off elsewhere                 |
| `invalid-code-point`    | a lone surrogate or a noncharacter in a string                | error in `i-json`, off elsewhere                 |
| `byte-order-mark`       | a byte order mark at the start of the file                    | warning in `json` and `i-json`, error in `jsonl` |
| `missing-final-newline` | a JSON Lines file that does not end with a newline            | warning in `jsonl`                               |

Warnings don't fail a run unless there are more than `--max-warnings N`.

## Options and configuration

```text
--dialect DIALECT[:GLOB]   check files matching GLOB (every file, without one) as DIALECT,
                           or as `auto`, the extension's default; the last match wins
--strict GLOB              the same as --dialect json:GLOB, kept from check-json5
--error RULE, --warn RULE, --ignore RULE
--max-warnings N
--config FILE, --no-config
```

A glob without `/` matches the file name at any depth, and one with `/` matches the whole path.
`*` also matches `/`. Paths are matched as given on the command line, so relative to the working
directory, even for globs from a config file; pre-commit runs from the repository root.

Settings can also live in `jsonvet.toml`, or in a `[tool.jsonvet]` table in `pyproject.toml`.
jsonvet looks in the working directory and its parents up to the repository root, and the command
line wins:

```toml
rules = { byte-order-mark = "ignore", missing-final-newline = "error" }

# Checked in order; the last matching entry wins.
dialects = [
  { files = ".vscode/*.json", dialect = "jsonc" },
  { files = "package.json", dialect = "json" },
]
```

Exit status: 0 when no file has an error, 1 when one does, 2 for a usage error.

## Other ways to run it

```bash
uvx jsonvet config/*.json        # from PyPI, without installing
pip install jsonvet              # the same binary, as a Python package
cargo install jsonvet-cli        # from source
```

The library is the `jsonvet` crate:

```rust
use jsonvet::{check, Config, Dialect};

let report = check("{\"a\": 1, \"a\": 2}", &Config::new(Dialect::Json));
for problem in report.problems() {
    let at = report.location(problem.offset());
    println!("{}:{}: {}[{}] {problem}", at.line, at.column, problem.severity(), problem.rule());
}
```

## Coming from check-json5

Bump `rev` to `v2.0.0`. The `check-json5` hook id, the `check-json5` command and `--strict` keep
working until 3.0, and the `check-json5` package on PyPI now installs jsonvet.

2.0 gives the same pass or fail as 1.4.1 for every file, the same exit codes and the same
duplicate-key paths. Comparing both on 20,114 generated files in JSON5 and in strict mode, 40,228
runs in all, they agree on every one. What changes, and why, is listed in the
[CHANGELOG](CHANGELOG.md) and in [docs/design.md](docs/design.md):

- each line gains the severity and rule, as in `error[duplicate-key]`, and syntax errors use
  jsonvet's own wording;
- the hook also checks `.jsonl` and `.ndjson` files;
- lines end at every JSON5 line terminator (LF, CR, CRLF, U+2028, U+2029), so positions after a
  lone CR or U+2028 differ;
- the importable Python module is gone; the wheel brings the `jsonvet` and `check-json5` commands.

## Speed

Median wall time of one run over the files, from `uv run scripts/bench.py` on 12 cores of a
Ryzen 7 H 255 under Linux. Both tools give the same verdict in every case:

| Case                       | check-json5 1.4.1 | jsonvet 2.0 | Speed-up |
| -------------------------- | ----------------: | ----------: | -------: |
| 1,100 config files         |             40 ms |     12.5 ms |       3x |
| 2.7 MB JSON                |             41 ms |      9.8 ms |       4x |
| 2.7 MB JSON, duplicate key |            330 ms |     11.6 ms |      29x |
| 120 KB JSONC               |             22 ms |      1.5 ms |      15x |
| 1.8 MB JSON5               |         11,641 ms |      8.3 ms |   1,398x |

1.4 sent JSON, and JSONC with its comments stripped, to Python's C parser, so those were
already fast. Any other JSON5 syntax went to a pure-Python parser, and so did the second pass
that finds a duplicate key's position. jsonvet does everything in one pass of one parser.

## How it is tested

- the [json5-tests](https://github.com/json5/json5-tests) suite, all 114 cases;
- a reviewed snapshot of edge cases in every dialect;
- property tests: no input makes it panic, every position lies on a character boundary and
  matches a naive recount;
- fuzzing with cargo-fuzz, against serde_json and the json5 crate as references;
- the file-by-file comparison with check-json5 1.4.1 above, in CI;
- CI on Linux, macOS and Windows, plus the oldest supported Rust (1.88).

## Development

```bash
cargo test --workspace                         # tests, including json5-tests (git submodule)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
uv run scripts/differential.py                 # compare with check-json5 1.4.1
uv run scripts/bench.py                        # time against check-json5 1.4.1
cargo +nightly fuzz run check                  # fuzzing, from fuzz/
```

Clone with `--recurse-submodules`, or run `git submodule update --init` for the test suite.

## Credits

I (Simon van Lierde) started this as a fork of [check-json5](https://gitlab.com/bmares/check-json5)
by Ben Mares, which is based on the [pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks)
repository by Anthony Sottile and contributors. 2.0 is a rewrite in Rust.

## License

[MIT](LICENSE), like the projects it started from
([Ben Mares](https://gitlab.com/bmares/check-json5/-/blob/main/LICENSE),
[pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks/blob/main/LICENSE)).
