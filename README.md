# check-json5

## Links

- [GitHub (main)](https://github.com/simonvanlierde/check-json5)
- [GitLab (mirror)](https://gitlab.com/simonvanlierde/check-json5)

## Introduction

This is a pre-commit hook which verifies that `.json` files in a repository are valid [JSON5](https://json5.org/). The JSON5 format is similar to JSON, but it permits comments, trailing commas, and more. It is similar to the so-called "JSONC" (JSON with Comments) format, but JSON5 has an actual specification.

This hook is a drop-in replacement for the `check-json` hook from the [official pre-commit-hooks repository](https://pre-commit.com/hooks.html). A file succeeds when it can be loaded by the [json5 library](https://pypi.org/project/json5/). (In contrast, `check-json` uses the built-in [json library](https://docs.python.org/3/library/json.html).)

## Usage

In `.pre-commit-config.yaml` under the `repos:` section, add the following:

```yaml
- repo: https://github.com/simonvanlierde/check-json5
  rev: v1.1.0
  hooks:
  - id: check-json5
```

(The original `check-json` hook should probably be removed in case it is already included.)

## Credits

This project was adapted by Simon van Lierde from the original [check-json5](https://gitlab.com/bmares/check-json5) by Ben Mares, which itself was based on [@asottile and various contributors to the official pre-commit-hooks repository](https://github.com/pre-commit/pre-commit-hooks/commits/master/pre_commit_hooks/check_json.py).

I have updated this fork to use [uv](https://github.com/astral-sh/uv) for dependency management instead of Poetry.

## License

This project is published under the [MIT license](LICENSE).

It is based on [check-json5](https://gitlab.com/bmares/check-json5) by Ben Mares ([MIT license](https://gitlab.com/bmares/check-json5/-/blob/main/LICENSE)), which itself is adapted from the [pre-commit-hooks repository](https://github.com/pre-commit/pre-commit-hooks) by Anthony Sottile et al. ([MIT license](https://github.com/pre-commit/pre-commit-hooks/blob/main/LICENSE)).
