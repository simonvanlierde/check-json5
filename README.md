# check-json5

[![CI](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml/badge.svg)](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/simonvanlierde/check-json5/branch/main/graph/badge.svg)](https://codecov.io/gh/simonvanlierde/check-json5)
[![Python 3.10+](https://img.shields.io/badge/python-3.10%2B-blue)](https://www.python.org/downloads/)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](https://github.com/pre-commit/pre-commit)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A [pre-commit](https://pre-commit.com/) hook that checks whether `.json` files parse as
[JSON5](https://json5.org/) (JSON with comments, trailing commas, and more). A drop-in replacement
for the standard `check-json` hook that also accepts JSON5, without giving up the speed of the C
parser on ordinary JSON.

## Features

- Accepts JSON5 syntax (comments, trailing commas) that the standard `check-json` hook rejects.
- Fast path: strict JSON is parsed with the C-speed stdlib `json` parser, falling back to the
  pure-Python `json5` parser only when a file actually uses JSON5 features (~500× faster on large
  strict-JSON files; see [Performance](#performance)).
- Duplicate-key detection on both paths.
- Drop-in replacement for `check-json`.

## Links

- 🐙 [GitHub (main)](https://github.com/simonvanlierde/check-json5)
- 🦊 [GitLab (mirror)](https://gitlab.com/simonvanlierde/check-json5)

## Usage

In `.pre-commit-config.yaml`, under `repos:`, add:

```yaml
- repo: https://github.com/simonvanlierde/check-json5
  rev: v1.2.0
  hooks:
    - id: check-json5
```

Remove the original `check-json` hook if it is already included. A file passes when it parses as
JSON5 and has no duplicate keys.

## Introduction

JSON5 is similar to JSON but permits comments, trailing commas, and more. It resembles "JSONC" (JSON
with Comments) but, unlike JSONC, has an actual specification. Many config files in the wild are
really JSON5, so the standard `check-json` hook, which rejects comments outright, flags them as
invalid. This hook accepts them.

Strict-JSON files are parsed with the fast built-in [`json`](https://docs.python.org/3/library/json.html)
library; only files that use JSON5 features fall back to the pure-Python
[`json5`](https://pypi.org/project/json5/) library.

## Performance

The pure-Python `json5` parser is slow on large files. Since most `.json` files are strict JSON, the
hook tries the C-speed stdlib `json` parser first and only falls back to `json5` when a file uses
comments or trailing commas. Duplicate-key detection works on both paths.

Parse time per file (strict JSON, taking the fast path):

| File size    | Before (`json5` only) | After (fast path) | Speedup |
| ------------ | --------------------- | ----------------- | ------- |
| ~1 KB config | 7 ms                  | <0.1 ms           | ~600×   |
| ~50 KB       | 235 ms                | 0.4 ms            | ~645×   |
| ~500 KB      | 2.4 s                 | 4 ms              | ~575×   |
| ~2 MB        | 10.4 s                | 21 ms             | ~500×   |

Files that use JSON5 syntax still go through the `json5` parser and are unchanged. For tiny config
files the absolute saving is negligible next to Python's own startup cost; the win matters on large
JSON files.

## Development

The project uses [uv](https://github.com/astral-sh/uv) for dependency management, `pytest` for
tests, `pre-commit` for local checks, and GitHub Actions for CI with coverage reported to Codecov.

```bash
uv sync                 # install dependencies
uv run pytest           # run the test suite
uv run pytest --cov     # tests with coverage
pre-commit run -a       # run the configured hooks locally
```

<!-- Verify these commands against the repo's actual justfile / scripts and adjust if needed. -->

## Credits

Adapted by Simon van Lierde from the original [check-json5](https://gitlab.com/bmares/check-json5) by
Ben Mares, itself based on the
[pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks) repository by Anthony Sottile and
contributors. This version switches dependency management from Poetry to uv and adds the fast-path
parser and benchmark above.

## License

[MIT](LICENSE), consistent with the upstream projects it is based on
([Ben Mares](https://gitlab.com/bmares/check-json5/-/blob/main/LICENSE),
[pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks/blob/main/LICENSE)).
