# check-json5

[![CI](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml/badge.svg)](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/simonvanlierde/check-json5/branch/main/graph/badge.svg)](https://codecov.io/gh/simonvanlierde/check-json5)
[![Python 3.10+](https://img.shields.io/badge/python-3.10%2B-blue)](https://www.python.org/downloads/)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](https://github.com/pre-commit/pre-commit)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![release](https://img.shields.io/github/v/release/simonvanlierde/check-json5)](https://github.com/simonvanlierde/check-json5/releases/latest)

A [pre-commit](https://pre-commit.com/) hook that checks whether `.json` files parse as
[JSON5](https://json5.org/) (JSON with comments, trailing commas, and more). It replaces the
standard `check-json` hook, accepts JSON5 too, and keeps the speed of the C parser on ordinary
JSON.

## Features

- Accepts JSON5 syntax (comments, trailing commas) that the standard `check-json` hook rejects.
- Every failure names its place, as `file:line:col: message`, so editors and terminals link
  straight to it. A duplicate key also gets its path, and every repeat in a file is listed:

  ```text
  config.json:4:5: duplicate key $.server.port
  config.json:9:3: duplicate key $.plugins[2].name
  ```

- On GitHub Actions, each failure is also an annotation on the line of the pull request diff.
- `--strict GLOB` holds chosen files to strict JSON (no comments, trailing commas or `NaN`), for
  files that a JSON parser reads at runtime, such as `package.json`.
- Fast paths: the hook parses strict JSON, and JSON with only comments and trailing commas, with
  the C-speed stdlib `json` parser. Only files that use more of JSON5 (single quotes, unquoted
  keys, hex numbers) fall back to the pure-Python `json5` parser. See [Performance](#performance).
- Accepts `NaN` and `Infinity` (both parsers allow them) unless `--strict` covers the file.

## Links

- 🐙 [GitHub (main)](https://github.com/simonvanlierde/check-json5)
- 🦊 [GitLab (mirror)](https://gitlab.com/simonvanlierde/check-json5)

## Usage

In `.pre-commit-config.yaml`, under `repos:`, add:

```yaml
- repo: https://github.com/simonvanlierde/check-json5
  rev: v1.4.0
  hooks:
    - id: check-json5
```

Remove the original `check-json` hook if it is already included. A file passes when it parses as
JSON5 and has no duplicate keys. The hook picks files by name: `.json`, `.jsonc`, `.json5`,
`.geojson`, `.jsonld`, Tailscale's `.hujson` and `.jwcc`, VS Code's `.code-workspace` and
`.code-snippets`, `.webmanifest`, and `.babelrc`. `check-json` picks them by type instead, so
files such as `.ipynb` notebooks are no longer checked after the swap. Add them with `files:` if
you need them.

### Strict files

Some JSON files are read by a strict parser at runtime: a comment in `package.json` passes a JSON5
check and then breaks `npm`. Name those files with `--strict`. Each pattern matches the file name
at any depth, or the whole path when it contains a `/`. `*` also matches across `/`. A leading `!`
exempts files, and the last matching pattern wins:

```yaml
    - id: check-json5
      args: [--strict, "*.json", --strict, "!tsconfig*.json", --strict, "!.vscode/*"]
```

### Without pre-commit

The hook is also a command on PyPI:

```bash
uvx check-json5 --strict package.json config/*.json
```

## Introduction

JSON5 is similar to JSON but permits comments, trailing commas, and more. It resembles "JSONC" (JSON
with Comments) but, unlike JSONC, has an actual specification. Many config files in the wild are
really JSON5, so the standard `check-json` hook, which rejects comments outright, flags them as
invalid. This hook accepts them.

Strict-JSON files are parsed with the fast built-in [`json`](https://docs.python.org/3/library/json.html)
library, and so are files that add only comments and trailing commas. Only files that use more
of JSON5 fall back to the pure-Python [`json5`](https://pypi.org/project/json5/) library.

## Performance

The pure-Python `json5` parser is slow on large files. Most `.json` files are strict JSON, and
most of the rest add only comments and trailing commas. The hook parses both kinds with the
C-speed stdlib `json` parser. It falls back to `json5` only for files that use more of JSON5
(single quotes, unquoted keys, hex numbers). Duplicate-key detection works on every path.

Time for one hook call per file (strict JSON, taking the fast path), best of 10 runs of
`uv run python scripts/bench.py` on 2026-10-11 (Intel i9-14900KF, Linux, Python 3.13.7, json5 0.15.0,
load average 1.5):

| File size | Before (`json5` only) | After (fast path) | Speedup |
| --------- | --------------------- | ----------------- | ------- |
| 1 KB      | 2.5 ms                | 0.02 ms           | ~100×   |
| 47 KB     | 123 ms                | 0.19 ms           | ~660×   |
| 483 KB    | 1.3 s                 | 1.8 ms            | ~750×   |
| 2 MB      | 5.4 s                 | 7.7 ms            | ~700×   |

The script generates its fixture files in a temporary directory, so nothing large is committed.
Timings vary between runs by up to 2× on a loaded machine. The 1 KB fast-path time is close to
the timer's resolution, so its speed-up ranged from ~80× to ~100× across runs.

To time whole hook runs instead, run `bash scripts/bench.sh` (needs
[hyperfine](https://github.com/sharkdp/hyperfine)). It times the hook against the `json5` parser
on 50 KB, 500 KB and 2 MB files and prints one table; `--quick` times the 50 KB file once. Each
run includes about 12 ms of Python startup, so its speed-ups are smaller than the ones above.

Most `tsconfig.json` and `.vscode/settings.json` files use comments or trailing commas and
nothing else from JSON5. The hook turns their comments into spaces, drops their trailing commas,
and parses the result with the same C parser. `bash scripts/bench.sh --jsonc` times whole hook
runs on such files against the `json5` parser alone (2026-10-13, Intel i9-14900KF, Linux, Python
3.13, json5 0.15.0, one pinned core, load average 2.5):

| JSONC file | `json5` alone | Hook   | Speedup |
| ---------- | ------------- | ------ | ------- |
| 50 KB      | 193 ms        | 18 ms  | ~11×    |
| 500 KB     | 2.1 s         | 46 ms  | ~45×    |
| 2 MB       | 7.8 s         | 141 ms | ~56×    |

About 12 ms of each run is Python starting up. Files that need the `json5` parser are unchanged.
On tiny strict-JSON files the saving disappears next to that startup cost. It matters on large
files and on repositories with many JSONC files.

## Development

The project uses [uv](https://github.com/astral-sh/uv) for dependency management, `pytest` for
tests, `ruff` and `ty` for lint and types, `pre-commit` for local checks, and GitHub Actions for
CI with coverage reported to Codecov. These are the commands CI runs:

```bash
uv sync                                               # install dependencies
uv run ruff check --no-fix .                          # lint
uv run ruff format --check .                          # formatting
uv run ty check                                       # types
uv run pytest --cov=check_json5 --cov-fail-under=95   # tests with coverage
uv run pre-commit install                             # run the hooks on every commit
uv run pre-commit run -a                              # run the hooks over the whole tree
```

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
