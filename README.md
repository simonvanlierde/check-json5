# check-json5

[![CI](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml/badge.svg)](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/simonvanlierde/check-json5/branch/main/graph/badge.svg)](https://codecov.io/gh/simonvanlierde/check-json5)
[![Python 3.10+](https://img.shields.io/badge/python-3.10%2B-blue)](https://www.python.org/downloads/)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](https://github.com/pre-commit/pre-commit)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![release](https://img.shields.io/github/v/release/simonvanlierde/check-json5)](https://github.com/simonvanlierde/check-json5/releases/latest)

A [pre-commit](https://pre-commit.com/) hook that checks whether `.json` files parse as
[JSON5](https://json5.org/) (JSON with comments, trailing commas, and more). It replaces the
standard `check-json` hook, which rejects any comment, and it still parses ordinary JSON with the
fast C parser.

- Every failure names its place, as `file:line:col: message`, so editors and terminals link
  straight to it. A duplicate key also gets its path, and every repeat in a file is listed:

  ```text
  config.json:4:5: duplicate key $.server.port
  config.json:9:3: duplicate key $.plugins[2].name
  ```

- On GitHub Actions, each failure is also an annotation on the line of the pull request diff.
- Accepts `NaN` and `Infinity` (both parsers allow them) unless `--strict` covers the file.

Source on [GitHub](https://github.com/simonvanlierde/check-json5), mirrored on
[GitLab](https://gitlab.com/simonvanlierde/check-json5). Release notes are in the
[CHANGELOG](CHANGELOG.md).

## Usage

In `.pre-commit-config.yaml`, under `repos:`, add:

```yaml
- repo: https://github.com/simonvanlierde/check-json5
  rev: v1.4.0
  hooks:
    - id: check-json5
```

If you already use the original `check-json` hook, remove it. A file passes when it parses as
JSON5 and has no duplicate keys. The hook picks files by name: `.json`, `.jsonc`, `.json5`,
`.geojson`, `.jsonld`, Tailscale's `.hujson` and `.jwcc`, VS Code's `.code-workspace` and
`.code-snippets`, `.webmanifest`, and `.babelrc`. `check-json` picks them by file type instead,
so after the swap files such as `.ipynb` notebooks are no longer checked. If you need them, add
them with `files:`.

Some tools read their JSON files with a strict parser: a comment in `package.json` passes a JSON5
check and then breaks `npm`. Name those files with `--strict` to hold them to strict JSON (no
comments, trailing commas or `NaN`). Each pattern matches the file name at any depth, or the whole
path when it contains a `/`. `*` also matches across `/`. A leading `!` exempts files, and the
last matching pattern wins:

```yaml
    - id: check-json5
      args: [--strict, "*.json", --strict, "!tsconfig*.json", --strict, "!.vscode/*"]
```

The hook is also on PyPI as a command: `uvx check-json5 --strict package.json config/*.json`.

## Performance

Strict JSON, and JSON that adds only comments and trailing commas, go through the C-speed stdlib
`json` parser. Only files that use more of JSON5 (single quotes, unquoted keys, hex numbers) fall
back to the pure-Python [`json5`](https://pypi.org/project/json5/) parser, which is slow on large
files. Every path finds duplicate keys.

Time for one hook call per file (strict JSON, taking the fast path), best of 10 runs of
`uv run python scripts/bench.py` on 2026-10-11 (Intel i9-14900KF, Linux, Python 3.13.7, json5 0.15.0,
load average 1.5):

| File size | Before (`json5` only) | After (fast path) | Speedup |
| --------- | --------------------- | ----------------- | ------- |
| 1 KB      | 2.5 ms                | 0.02 ms           | ~100×   |
| 47 KB     | 123 ms                | 0.19 ms           | ~660×   |
| 483 KB    | 1.3 s                 | 1.8 ms            | ~750×   |
| 2 MB      | 5.4 s                 | 7.7 ms            | ~700×   |

Timings vary between runs by up to 2× on a loaded machine. The 1 KB fast-path time is close to
the timer's resolution, so its speed-up ranged from ~80× to ~100× across runs.

`bash scripts/bench.sh --jsonc` (needs [hyperfine](https://github.com/sharkdp/hyperfine)) times
whole hook runs on files with only comments and trailing commas, such as most `tsconfig.json` and
`.vscode/settings.json` files, against the `json5` parser alone (2026-10-13, Intel i9-14900KF,
Linux, Python 3.13, json5 0.15.0, one pinned core, load average 2.5):

| JSONC file | `json5` alone | Hook   | Speedup |
| ---------- | ------------- | ------ | ------- |
| 50 KB      | 193 ms        | 18 ms  | ~11×    |
| 500 KB     | 2.1 s         | 46 ms  | ~45×    |
| 2 MB       | 7.8 s         | 141 ms | ~56×    |

About 12 ms of each run is Python starting up. Files that need the `json5` parser are no faster.
On tiny strict-JSON files the saving disappears next to that startup cost. It matters on large
files and on repositories with many JSONC files. Without `--jsonc`, `scripts/bench.sh` times
strict-JSON files of 50 KB, 500 KB and 2 MB, and `--quick` times the 50 KB file once.

## Development

CI runs the lint, format, type and test commands below, with coverage reported to Codecov. The
last two install and run the hooks in your own checkout:

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

I (Simon van Lierde) adapted this hook from the original
[check-json5](https://gitlab.com/bmares/check-json5) by Ben Mares, which is based on the
[pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks) repository by Anthony Sottile and
contributors. This version moves dependency management from Poetry to uv and adds the fast-path
parser and benchmark above.

## License

[MIT](LICENSE), like the upstream projects it is based on
([Ben Mares](https://gitlab.com/bmares/check-json5/-/blob/main/LICENSE),
[pre-commit-hooks](https://github.com/pre-commit/pre-commit-hooks/blob/main/LICENSE)).
