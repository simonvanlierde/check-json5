# jsonvet 2.0 design

jsonvet checks files in the JSON family for syntax errors, duplicate keys and the rules each
dialect adds, and reports every problem with its file, line, column and rule. 2.0 is a rewrite of
check-json5 1.4 (Python) in Rust. This document records what 2.0 promises, how it is built, and
what has to be true before it ships.

## The contract with 1.4

jsonvet 2.0 replaces check-json5 1.4 in place. These stay the same:

- the pass or fail verdict for every file in the two dialects 1.4 knows (JSON5, and strict JSON
  through `--strict`);
- the exit code: 0 when every file passes, 1 when any fails, 2 on a usage error;
- what a duplicate-key report says: the position of the repeated key and its JSONPath;
- the meaning of `--strict GLOB`, which becomes an alias for `--dialect json:GLOB`, with
  `--strict '!GLOB'` mapping to `--dialect auto:GLOB`.

These change on purpose, and the changelog marks them **Breaking**:

- the wording of syntax errors (jsonvet has its own messages, not the json5 package's or
  Python's);
- the output line, which gains a severity and a rule id;
- the importable Python module, which is gone: the wheel brings only the commands;
- lines now end at every JSON5 line terminator, so positions differ from 1.4 in files that use a
  lone CR, U+2028 or U+2029;
- the hook also checks `.jsonl` and `.ndjson` files.

A syntax error points at the first character that cannot continue the document, or at the end of
the input. Where that differs from 1.4, the difference is listed with its reason below. Any
other difference is a bug.

### Known differences from 1.4

`scripts/differential.py` compares jsonvet with check-json5 1.4.1 file by file. Its first run,
against 1.4.0, found two bugs in 1.4.0, fixed in 1.4.1: strict mode passed `NaN` or `Infinity`
running on into other text (`[NaN-2.5e3`), and unquoted JSON5 keys were compared with their
`\u` escapes undecoded. Against 1.4.1, all 40,228 comparisons (20,114 files in each of
JSON5 and strict mode, 2026-10-13) agree. These differences are deliberate, and the corpus
can still meet them:

- Lines end at lone CR, U+2028 and U+2029 in jsonvet, so positions after one of them differ.
- Raw U+2028 and U+2029 inside strings pass in jsonvet, as the JSON5 spec allows; 1.4's JSON5
  path rejected them.
- Nesting deeper than 10,000 levels is a syntax error in jsonvet (`Config::max_depth` changes
  it). 1.4 stopped at 9,998 levels, or 9,994 in files with JSON5 syntax, so only files from 9,995
  to 10,000 levels deep get a different verdict.
- A `\u` escape in an unquoted key must stand for a character the key could hold literally, as
  ECMAScript 5.1 says; 1.4 passed any escape, so `{\u0301: 1}` passed there. Literal characters
  agree: both go by general category (letters to start a key, then marks, digits and connectors
  too), as the spec and the JS reference do, not by Unicode's `ID_Start`.
- Syntax-error wording, and some syntax-error positions, follow the rule above.

## Dialects

One parser covers six dialects. Each step in the chain only adds syntax:
JSON ⊂ JSONC ⊂ JWCC ⊂ JSON5. I-JSON is JSON with stricter content rules, and JSON Lines is one JSON
value per line.

| Dialect | Adds to JSON | Default for |
| --- | --- | --- |
| `json` | nothing (RFC 8259) | none; chosen with `--dialect` or `--strict` |
| `i-json` | stricter content: duplicate keys, lone surrogates, noncharacters and numbers out of a double's range are errors (RFC 7493) | none |
| `jsonc` | `//` and `/* */` comments | `.jsonc`, `.code-workspace`, `.code-snippets` |
| `jwcc` | comments and trailing commas (HuJSON) | `.hujson`, `.jwcc` |
| `json5` | the JSON5 1.0.0 grammar | `.json`, `.json5`, `.geojson`, `.jsonld`, `.webmanifest`, `.babelrc` |
| `jsonl` | one JSON value per line, no blank lines | `.jsonl`, `.ndjson` |

`.json` defaults to JSON5, as in 1.4, because most `.json` files that fail strict JSON are
config files with comments. `auto` means "the default for this file's extension".

Lines end at LF, CR, CRLF, U+2028 and U+2029, as the JSON5 spec defines them. Columns count
Unicode code points from the start of the line, starting at 1.

## Rules

Every problem belongs to a rule. Each rule has a default severity per dialect. `--warn`,
`--error` and `--ignore` change it, and an ignored rule is not checked at all.

| Rule | What it reports | Default |
| --- | --- | --- |
| `syntax` | the first syntax error; checking stops there | error, always |
| `duplicate-key` | a key repeated in the same object, with both positions and the JSONPath | error |
| `invalid-number` | a number outside a double's range, or an integer past ±(2⁵³ − 1) | error in `i-json`, off elsewhere |
| `invalid-code-point` | a lone surrogate or a noncharacter in a string | error in `i-json`, off elsewhere |
| `byte-order-mark` | a byte order mark at the start of the file | warning in `json`, `i-json`; error in `jsonl`; off elsewhere |
| `missing-final-newline` | a JSON Lines file without a final newline | warning in `jsonl` |

Warnings don't fail a run unless `--max-warnings N` is exceeded. In GitHub Actions each problem is
also printed as a `::warning` or `::error` annotation. `--format sarif` prints one SARIF 2.1.0
document instead, with no annotations; its JSON is written by hand, to keep serde_json out of the
binary.

Output:

```text
config.json:4:5: error[duplicate-key] duplicate key $.server.port (first at 2:5)
data.jsonl:12:1: error[syntax] expected a value, found end of line
```

## Configuration

jsonvet reads `jsonvet.toml`, or a `[tool.jsonvet]` table in `pyproject.toml`. It looks in the
working directory and its parents, and stops at the first directory that has a `.git` entry, so a
stray file above the repository is never used. `--config PATH` picks a file, `--no-config` skips
the search, and flags on the command line override the file.

```toml
rules = { byte-order-mark = "ignore", missing-final-newline = "error" }

# Checked in order; the last matching entry wins, as with --dialect.
dialects = [
  { files = ".vscode/*.json", dialect = "jsonc" },
  { files = "package.json", dialect = "json" },
]
```

A glob without `/` matches the file name at any depth, and one with `/` matches the whole path.
`*` also matches `/`.

## Architecture

```text
crates/jsonvet/        library: the parser, rules and positions. Depends on memchr and unicode-general-category.
crates/jsonvet-cli/    binaries jsonvet and check-json5: arguments, config, output. Built into the
                       PyPI wheels by maturin.
compat/check-json5/    the check-json5 package on PyPI: depends on jsonvet, nothing else
setup.py               placeholder that pre-commit installs: pins the jsonvet wheel
fuzz/  tests/  scripts/
```

### The library interface

```rust
pub fn check(text: &str, config: &Config) -> Report<'_>;

pub struct Config { /* dialect, rule overrides */ }
impl Config { pub fn new(dialect: Dialect) -> Self; pub fn rule(self, rule: Rule, severity: Severity) -> Self; }

#[non_exhaustive] pub enum Dialect { Json, IJson, Jsonc, Jwcc, Json5, JsonLines }
impl Dialect { pub fn for_path(path: &Path) -> Option<Dialect>; }

impl<'a> Report<'a> {
    pub fn problems(&self) -> &[Problem];               // document order
    pub fn location(&self, offset: usize) -> Location;  // line and column, computed on first use
}

pub struct Problem { /* rule, severity, kind, offset, related offset, JSONPath */ }
```

The text is `&str`: the caller has already checked UTF-8, so every offset falls on a character
boundary and columns are well defined. `check` never panics and has no error mode. Problems in the
file are its output.

Behind that interface:

- a single-pass byte scanner, with memchr to skip through comments, and a UTF-8 decode only for
  bytes from 0x80 up;
- an explicit stack of frames instead of recursion, so the depth limit is a setting, not the
  call stack's size. The frames also hold each object's keys for duplicate detection and give
  the JSONPath of any problem;
- keys compared by their decoded value, borrowed from the input unless they contain an escape;
- line starts found once with memchr, and only when the first location is asked for.

### Python packaging

pre-commit installs a `language: python` hook by running `pip install .` on the repository. The
root `setup.py` is a placeholder that depends on the exact released `jsonvet` wheel, so a hook
install downloads a prebuilt binary and never compiles Rust. The same pattern ships typos. The
wheels are built by maturin with `bindings = "bin"`, for Linux (glibc and musl, x86-64 and
arm64), macOS (x86-64 and arm64) and Windows (x64 and arm64).

## Testing

- **Conformance:** the json5-tests suite as a git submodule; each file's extension says whether
  it must pass or fail.
- **Corpus:** one file per case under `tests/corpus/<dialect>/`: cases from 1.4's tests, edge
  cases found while surveying other parsers, and every fuzz finding. `crates/jsonvet-cli/tests/corpus.rs`
  runs the binary on each, and one insta snapshot keeps every exit code and output line.
- **Differential:** `scripts/differential.py` runs the corpus through `check-json5==1.4.1`
  (Python 3.13) and jsonvet, in the two dialects 1.4 knows, and compares verdicts, positions,
  paths and exit codes.
- **Fuzzing:** cargo-fuzz targets for raw bytes (no panics, offsets on character boundaries),
  against serde_json (anything it accepts, jsonvet accepts as JSON) and against the json5 crate
  (anything jsonvet accepts as JSON5, json5 accepts). The last skips inputs that hit json5's own
  limits rather than the grammar's, each found by fuzzing: integers past 64 bits, lone surrogate
  escapes, U+2028 and U+2029 in strings, and identifier characters from Unicode releases newer
  than json5's tables.
- **Properties:** proptest, for example that a location matches a naive recount.

## Release bar for 2.0.0

1. json5-tests passes in every dialect it applies to.
2. The differential run shows no unexplained difference from 1.4.
3. Every fuzz target runs until an hour passes with no new coverage, and none crashes. Bugs
   so far turned up in a run's first minutes, and coverage flattens soon after, so more hours
   mostly retest covered paths. After the merge, the fuzz workflow runs after each parser change
   and weekly, on a corpus it keeps between runs.
4. The placeholder installs and runs under pre-commit and prek on Linux, macOS and Windows.
5. A committed hyperfine table against 1.4, including the large JSONC files where 1.4 comes close.
6. The README leads with the contract; the changelog lists every Breaking change.

A release publishes to PyPI and crates.io from `release.yml`, with trusted publishing on both.
The `check-json5` package uploads after the `jsonvet` wheels, and `jsonvet-cli` after the
`jsonvet` crate. crates.io only allows trusted publishing for a crate that already exists, so 2.0.0
went up by hand.

## Not in 2.0

JSON text sequences (RFC 7464) and Hjson. Hjson's unquoted strings need a tokenizer of their own.
