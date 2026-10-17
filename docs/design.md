# jsonvet design

jsonvet checks files in the JSON family for syntax errors, duplicate keys and the rules each
dialect adds, and reports every problem with its file, line, column and rule. 2.0 was a rewrite of
check-json5 1.4 (Python) in Rust. This document records what jsonvet promises, how it is built, and
the bar it had to meet before it shipped. Items marked *since 3.0* changed after 2.0.0.

## The contract with 1.4

jsonvet 2.0 replaces check-json5 1.4 in place. These stay the same:

- the pass or fail verdict for every file 2.0 checks in the two dialects 1.4 knows: JSON5 (the
  default for `.json`, `.json5`, `.geojson`, `.jsonld`, `.webmanifest` and `.babelrc` files, and
  for any other extension) and strict JSON through `--strict`;
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
- *since 3.0*, a duplicate-key JSONPath longer than 256 bytes is shortened: jsonvet keeps the
  segments that fit in 128 bytes at each end, with `…` between them, so a path is at most 259
  bytes, where 1.4 and 2.0 printed the whole path;
- *since 3.0*, a missing file whose name starts with `--`, such as `--x`, is an unknown option
  and exits 2, where 1.4.1 and 2.0 exit 1;
- the hook also checks `.jsonl` and `.ndjson` files;
- `.jsonc`, `.code-workspace` and `.code-snippets` files are checked as JSONC, and `.hujson` and
  `.jwcc` files as JWCC. 1.4 read every file as JSON5, so JSON5-only syntax in those files, such
  as `{'a': 1, b: 2}`, passed in 1.4 and fails now. `--dialect json5:GLOB`, or a `dialects` entry
  in the config file, keeps the old verdict.

A syntax error points at the first character that cannot continue the document, or at the end of
the input. Where that differs from 1.4, the difference is listed with its reason below. Any
other difference is a bug.

### Known differences from 1.4

`scripts/differential.py` compares jsonvet with check-json5 1.4.1 file by file. Its first run,
against 1.4.0, found two bugs in 1.4.0, fixed in 1.4.1: strict mode passed `NaN` or `Infinity`
running on into other text (`[NaN-2.5e3`), and unquoted JSON5 keys were compared with their
`\u` escapes undecoded. Against 1.4.1, on 20,184 files in each of JSON5 and strict mode (20,000
generated, the 114 json5-tests cases and the 70 corpus files; `--cases 20000 --seed 7`,
2026-10-15), 40,368 runs in all, every verdict and duplicate-key report matches except 385 runs on
383 files, each below: 381 files whose unquoted key starts with a `\u` escape that cannot start a
key (in JSON5 mode), the depth-10,000 file, and a file with a syntax error after a duplicate key
(in both modes). These differences are deliberate, and the corpus can still meet them:

- Lines end at lone CR, U+2028 and U+2029 in jsonvet, so positions after one of them differ.
- Raw U+2028 and U+2029 inside strings pass in jsonvet, as the JSON5 spec allows; 1.4's JSON5
  path rejected them.
- Nesting deeper than 10,000 levels is a syntax error in jsonvet (`Config::max_depth` changes
  it). 1.4 stopped at 9,998 levels, or 9,994 in files with JSON5 syntax, so only files from 9,995
  to 10,000 levels deep get a different verdict.
- A `\u` escape in an unquoted key must stand for a character the key could hold literally, as
  ECMAScript 5.1 says; 1.4 passed any escape, so `{\u0301: 1}` passed there, and `{\ud800: 1}`
  passed too, though a lone surrogate is no character at all. Literal characters
  agree: both go by general category (letters to start a key, then marks, digits and connectors
  too), as the spec and the JS reference do, not by Unicode's `ID_Start`.
- After a duplicate key, 1.4 stops at the duplicates; jsonvet goes on and also reports a syntax
  error later in the file. The verdict is the same.
- Syntax-error wording, and some syntax-error positions, follow the rule above.

## Dialects

One parser covers six dialects. Each step in the chain adds syntax:
JSON, JSONC, JWCC, JSON5. A file that passes in one passes in the next, with one exception: a
`//` comment ends at a different place in each. JWCC ends it at LF only, JSONC also at a lone CR,
and JSON5 also at U+2028 and U+2029. So `new-lines/comment-cr.json5` passes as JSONC and fails as
JWCC. I-JSON is JSON with stricter content rules, and JSON Lines is one JSON value per line.

The [README](../README.md#dialects) has the table of what each dialect adds and which extensions
default to it. A file with any other extension is checked as JSON5, and extensions match in lower
case only.

`.json` defaults to JSON5, as in 1.4, because most `.json` files that fail strict JSON are
config files with comments. `auto` means "the default for this file's extension".

Lines end at LF, CR, CRLF, U+2028 and U+2029, as the JSON5 spec defines them. Columns count
Unicode code points from the start of the line, starting at 1.

## Rules

Every problem belongs to a rule. Each rule has a default severity per dialect. `--warn`,
`--error` and `--ignore` change it, and an ignored rule is not checked at all.

The [README](../README.md#rules) lists the rules and their defaults. `syntax` is always an error,
and `duplicate-key` is an error in every dialect, not only `i-json`. `invalid-number` and `invalid-code-point` are the I-JSON rules: off
outside `i-json`.

Two things the I-JSON rules do not flag, and RFC 7493 leaves open: a number that underflows to
zero (`1e-400`), and an integer written with a fraction or an exponent past 2⁵³ (`9007199254740993.0`).
`i-json` reports an overflow, and an integer written as digits alone past ±(2⁵³ − 1).

Warnings don't fail a run unless `--max-warnings N` is exceeded. In GitHub Actions each problem is
also printed as a `::warning` or `::error` annotation. `--format sarif` prints one SARIF 2.1.0
document instead, with no annotations; its JSON is written by hand.

`--schema FILE` adds a seventh rule, `schema`, which belongs to the CLI: the library knows no
schemas. The library's `parse` returns the file's values as events, each with its byte offset;
the CLI folds them into `serde_json` values, validates those with the `jsonschema` crate (default
features off, so nothing is fetched), and maps each error's instance path, a JSON Pointer, back
to the offset of its value: one walk over the events of a document with errors finds every
error's value, with no pointer kept per value. The message names the value by JSONPath, through
the same path code as `duplicate-key`, so one file's problems use one path style; each pointer
segment counts as an index only where the value it steps into is an array, since a pointer
alone reads the same for `[0]` and `["0"]`. Schema errors are always errors: `--error`,
`--warn`, `--ignore` and the settings file do not take the id `schema`, and exit 2 on it as an
unknown rule. They follow the file's other problems; a file with a syntax error has no events,
so it gets none.

`--schema` is the cargo feature `schema`, on by default, so `cargo install jsonvet-cli` has it.
The PyPI wheels leave it out (`no-default-features` in `pyproject.toml`): jsonschema and its
regex engines take the binary from about 0.6 MB to about 5 MB, and every hook and Action install
downloads it. Without the feature, `--schema` still parses, so it exits 2 with a message naming
`cargo install jsonvet-cli` instead of reading the next argument as a file. One help text serves
both builds. CI tests and lints both builds.

jsonschema validates recursively, and its memory grows with the square of the depth when an error
sits deep under a recursive `$ref` (1.3 GB for a 50 KB file nested 10,000 levels). So with
`--schema` a file is read to 512 levels, not 10,000, and deeper is a syntax error. Only a run with
`--schema` starts a thread with a 64 MB stack for it; 512 levels against a recursive schema took
up to 2.2 MB in a debug build, more than Windows' 1 MB main thread. Without `--schema` jsonvet runs
on the main thread, as it did before. Folded values take about 23 times the file's size in memory.

The schema itself is read to 128 levels, and its local `$ref`s count too: each one jsonvet can
follow inside the file (a JSON Pointer, an anchor, or a URI naming an `$id` there) adds a level,
and a schema whose longest chain passes 128 is a usage error, placed at the `$ref` that crosses
the line. A `$ref` back to a schema on the way to it is recursion and adds nothing. Without the
check, 1,000 `allOf` and `$ref` hops overflow the 64 MB stack at once against a 512-level array.
jsonschema is also exponential in a schema where each level `$ref`s the next twice (24 levels
take 0.25 s, each level doubles it); no check covers that yet.

Schema regexes with back-references or lookaround run on fancy-regex, which backtracks; jsonvet
caps that at 5,000 steps per match, so `^(a|a)*\1b` over 5,000 strings takes under a second
instead of minutes. Over the cap, `pattern` reports an error, but `patternProperties` (and
`additionalProperties` beside it) count the key as not matching, because jsonschema 0.58 drops
the regex error there (`is_match(..).unwrap_or(false)`). A schema-side pre-check of every
`patternProperties` regex would close it; until then it is a known limit.

Output:

```text
config.json:4:5: error[duplicate-key] duplicate key $.server.port (first at 2:5)
data.jsonl:2:4: error[syntax] expected a value, found the end of the line
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

A pattern in a config file is relative to the directory that holds the file. With `data/*.json` in
the root's `jsonvet.toml`, `data/x.json` matches whether jsonvet runs in the root or in `data/`,
and whether the path is relative or absolute. A file outside that directory matches none of the
file's patterns. A pattern given with `--dialect` or `--strict` matches the path as typed, as in
1.4.

## Architecture

```text
crates/jsonvet/        library: the parser, rules and positions. Depends on memchr and unicode-properties.
crates/jsonvet-cli/    binaries jsonvet and check-json5: arguments, config, output, and --schema
                       (jsonschema, serde_json) behind the default feature `schema`. Built into
                       the PyPI wheels by maturin, without that feature (pyproject.toml).
compat/check-json5/    the check-json5 package on PyPI: depends on jsonvet, nothing else
setup.py               placeholder that pre-commit installs: pins the jsonvet wheel
action.yml             the GitHub Action: installs the jsonvet wheel from PyPI and runs it
fuzz/  tests/  scripts/
```

### The library interface

```rust
pub fn check(text: &str, config: &Config) -> Report<'_>;
pub fn parse(text: &str, config: &Config) -> (Report<'_>, Option<Vec<Event<'_>>>); // None after a syntax error

pub struct Config { /* dialect, rule overrides, depth limit */ }
impl Config {
    pub const DEFAULT_MAX_DEPTH: usize;  // 10,000
    pub fn new(dialect: Dialect) -> Self;
    pub fn rule(self, rule: Rule, severity: Severity) -> Self;
    pub fn max_depth(self, depth: usize) -> Self;
    pub fn dialect(&self) -> Dialect;
    pub fn severity(&self, rule: Rule) -> Severity;
}

#[non_exhaustive] pub enum Dialect { Json, IJson, Jsonc, Jwcc, Json5, JsonLines }
impl Dialect {
    pub const ALL: &'static [Dialect];
    pub const EXTENSIONS: &'static [(&'static str, Dialect)];  // default extensions, no dot
    pub const FILE_NAMES: &'static [(&'static str, Dialect)];  // default file names
    pub fn name(self) -> &'static str;                         // as on the command line, e.g. `jsonc`
    pub fn for_path(path: &Path) -> Option<Dialect>;
}

#[non_exhaustive] pub enum Rule { /* Syntax, DuplicateKey, ... */ }
impl Rule {
    pub const ALL: &'static [Rule];
    pub fn id(self) -> &'static str;
    pub fn description(self) -> &'static str;
    pub fn default_severity(self, dialect: Dialect) -> Severity;
}

#[non_exhaustive] pub enum Severity { Off, Warning, Error }  // ordered; name() gives `off`, `warning`, `error`
#[non_exhaustive] pub struct Location { pub line: usize, pub column: usize }
impl Location { pub const START: Location; }

impl<'a> Report<'a> {
    pub fn problems(&self) -> &[Problem];               // document order
    pub fn passed(&self) -> bool;                       // no problem is an error
    pub fn location(&self, offset: usize) -> Location;  // line and column, computed on first use
}

impl Problem {
    pub fn rule(&self) -> Rule;
    pub fn severity(&self) -> Severity;
    pub fn offset(&self) -> usize;
    pub fn related(&self) -> Option<Related>;  // the first copy of a duplicate key, or where an unterminated string or comment starts
    pub fn related_offset(&self) -> Option<usize>;  // related()'s offset
    pub fn path(&self) -> Option<&str>;        // the JSONPath of a duplicate key, at most 256 bytes and a `…`
    /* Display for the message */
}
#[non_exhaustive] pub struct Related { pub offset: usize, pub kind: RelatedKind }
#[non_exhaustive] pub enum RelatedKind { FirstOccurrence, Start }
```

`Rule`, `Severity`, `Dialect` and `Problem` implement `Display`, and `Rule` and `Dialect` parse
from their ids and names with `FromStr`. On failure, `UnknownRule::id()` and
`UnknownDialect::name()` give back the text that failed to parse. A `Report` is `Send` but not
`Sync`, because `location` caches line starts and character counts inside it.

The text is `&str`: the caller has already checked UTF-8, so every offset falls on a character
boundary and columns are well defined. `check` never panics and has no error mode. Problems in the
file are its output.

Behind that interface:

- a single-pass byte scanner, with memchr to skip through comments, and a UTF-8 decode only for
  bytes from 0x80 up;
- an explicit stack of frames instead of recursion, so the depth limit is a setting, not the
  call stack's size. The frames also hold each object's keys for duplicate detection and give
  the JSONPath of a duplicate key;
- keys compared by their decoded value, borrowed from the input unless they contain an escape;
- line starts found once with memchr, and only when the first location is asked for;
- in text that is not all ASCII, a count of the code points before every 4 KiB, built only as
  far as a lookup needs, so finding a column counts the characters of at most two chunks.

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
- **Fuzzing:** on a pull request that touches the library, each target runs for 30 seconds;
  cargo-fuzz targets for raw bytes (no panics, offsets on character boundaries),
  and against serde_json and the json5 crate, both ways. The json5 target skips inputs that hit
  json5's own limits rather than the grammar's, each found by fuzzing: integers past 64 bits,
  lone surrogate escapes, U+2028 and U+2029 in strings, and identifier characters from a Unicode
  release one side has and the other lacks.
- **API changes:** `cargo semver-checks` compares the library with its last release on every pull
  request. It reports and does not block, since 3.0 changes the public API on purpose.
- **Properties:** proptest, for example that a location matches a naive recount.
- **Mutations:** cargo-mutants over both crates with `test_workspace = true`, run weekly by
  `mutants.yml` and by hand (about 25 minutes). The mutants that survive change no behaviour, and
  `.cargo/mutants.toml` lists them, so any other survivor fails the weekly run:
  speed-only branches, OR of bits that never overlap, a match arm the fallback repeats, a
  Windows-only branch, a loop guard the loop body always stops first, a character-count
  checkpoint moved to the boundary before instead of after, the no-schema stub the run does not
  compile, and running every check on the big-stack thread.

## Release bar for 2.0.0 (met; kept as history)

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

## Dependencies and pins

Every version in the repository falls in one of five groups. `.github/renovate.json5` encodes
them.

- **Bumped monthly**, in one Renovate pull request after a release is 7 days old, with majors
  in their own: the workspace crates (ranges in `Cargo.toml`, versions in `Cargo.lock`, which
  is also refreshed for indirect dependencies), the fuzz crate's dependencies, GitHub Actions
  (pinned by commit, version in a comment), pre-commit hook revisions, and the CI tools: ruff,
  pre-commit and prek through `uvx`, the cargo tools through `install-action`, uv, and the
  maturin that builds the wheels. The fuzz crate pins json5 exactly, because it is the reference
  the `diff_json5` target compares with; a bump changes `fuzz/`, which runs the fuzz workflow.
  ruff is pinned in one place, the `rev` in `.pre-commit-config.yaml`, and CI runs its hooks.
  `check-json5` builds with `uv_build`, pinned to the same version as the uv CI uses, and Renovate
  moves the two together.
- **Bumped quarterly, with a re-run:** the conformance matrix's parsers, pinned exactly with
  their lock files, and its images, pinned by tag and digest. CI's `conformance/run.py --check`
  fails until `results/versions.json` matches the pins, so the pull request cannot merge until
  someone re-runs the matrix. The lock files are never refreshed on their own.
- **Fixed on purpose:** `check-json5==1.4.1` on Python 3.13 in `scripts/`, the reference the
  contract is checked against; `rust-version = "1.88"` and the MSRV job, raised by hand when a
  dependency or a language feature needs it; and jsonvet's own version in `setup.py` and
  `compat/`, which a release commit sets by hand and `scripts/check-versions.py` checks against
  the tag.
- **Pinned to the release:** the images and the Rust that build the release wheels, by digest and
  version, so a wheel does not change with an upstream image. This is an exception to the next
  item.
- **Floating on purpose:** Rust stable for lint and tests and nightly for fuzzing, so new lints
  and compiler changes show up at once, and the hosted runner images.

## Not supported

JSON text sequences (RFC 7464) and Hjson. Hjson's unquoted strings need a tokenizer of their own.
