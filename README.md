# check-json5

[![CI](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml/badge.svg)](https://github.com/simonvanlierde/check-json5/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/simonvanlierde/check-json5/branch/main/graph/badge.svg)](https://codecov.io/gh/simonvanlierde/check-json5)
[![Python 3.10+](https://img.shields.io/badge/python-3.10%2B-blue)](https://www.python.org/downloads/)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white)](https://github.com/pre-commit/pre-commit)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

## Links

- [GitHub (main)](https://github.com/simonvanlierde/check-json5)
- [GitLab (mirror)](https://gitlab.com/simonvanlierde/check-json5)

## Introduction

This is a pre-commit hook which verifies that `.json` files in a repository are valid [JSON5](https://json5.org/). The JSON5 format is similar to JSON, but it permits comments, trailing commas, and more. It is similar to the so-called "JSONC" (JSON with Comments) format, but JSON5 has an actual specification.

This hook is a drop-in replacement for the `check-json` hook from the [official pre-commit-hooks repository](https://pre-commit.com/hooks.html). A file succeeds when it parses as JSON5 and has no duplicate keys. Strict-JSON files are parsed with the fast built-in [json library](https://docs.python.org/3/library/json.html); only files that use JSON5 features (comments, trailing commas) fall back to the pure-Python [json5 library](https://pypi.org/project/json5/). (In contrast, `check-json` uses only the `json` library and rejects comments outright.)

## Usage

In `.pre-commit-config.yaml` under the `repos:` section, add the following:

```yaml
- repo: https://github.com/simonvanlierde/check-json5
  rev: v1.1.0
  hooks:
  - id: check-json5
```

(The original `check-json` hook should probably be removed in case it is already included.)

## Performance

The pure-Python `json5` parser is slow on large files. Since most `.json` files are strict JSON, the hook tries the C-speed stdlib `json` parser first and only falls back to `json5` when a file actually uses comments or trailing commas. Duplicate-key detection works on both paths.

Parse time per file (strict JSON, taking the fast path):

| File size | Before (`json5` only) | After (fast path) | Speedup |
| --------- | --------------------- | ----------------- | ------- |
| ~1 KB config | 7 ms | <0.1 ms | ~600× |
| ~50 KB | 235 ms | 0.4 ms | ~645× |
| ~500 KB | 2.4 s | 4 ms | ~575× |
| ~2 MB | 10.4 s | 21 ms | ~500× |

Files that use JSON5 syntax still go through the `json5` parser and are unchanged. For tiny config files the absolute saving is negligible next to Python's own startup cost — the win matters when you lint large JSON files.

## Credits

This project was adapted by Simon van Lierde from the original [check-json5](https://gitlab.com/bmares/check-json5) by Ben Mares, which itself was based on [@asottile and various contributors to the official pre-commit-hooks repository](https://github.com/pre-commit/pre-commit-hooks/commits/master/pre_commit_hooks/check_json.py).

I have updated this fork to use [uv](https://github.com/astral-sh/uv) for dependency management instead of Poetry.

## License

This project is published under the [MIT license](LICENSE).

It is based on [check-json5](https://gitlab.com/bmares/check-json5) by Ben Mares ([MIT license](https://gitlab.com/bmares/check-json5/-/blob/main/LICENSE)), which itself is adapted from the [pre-commit-hooks repository](https://github.com/pre-commit/pre-commit-hooks) by Anthony Sottile et al. ([MIT license](https://github.com/pre-commit/pre-commit-hooks/blob/main/LICENSE)).
