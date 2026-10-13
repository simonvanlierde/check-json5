"""Based on https://gitlab.com/bmares/check-json5/-/blob/db9b5c0f76dea9b0f28097b4421e4b40f39b2266/pypackages/pre_commit_hooks/check_json5.py."""

from __future__ import annotations

import json
import os
import re
import sys

# The hook starts once per commit, so its imports are a large share of a run:
# the type-only ones load for the type checker alone. `re` is free: json imports it.
TYPE_CHECKING = False
if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Iterator, Sequence
    from typing import Any

    # (line, column, message); line and column are 1-based, or None when the parser gives no position.
    Problem = tuple[int | None, int | None, str]


class DuplicateKeyError(ValueError):
    """A JSON object repeats a key."""

    def __init__(self, key: str) -> None:
        """Keep the repeated key for the error message."""
        super().__init__(f"Duplicate key: {key}")
        self.key = key


class NonStandardConstantError(ValueError):
    """Strict JSON contains NaN, Infinity or -Infinity."""


def raise_duplicate_keys(ordered_pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
    """Build a dict from a JSON object's pairs, refusing a repeated key.

    Raises:
        DuplicateKeyError: If a key repeats.

    """
    d = {}
    for key, val in ordered_pairs:
        if key in d:
            raise DuplicateKeyError(key)
        d[key] = val
    return d


def _reject_constant(name: str) -> float:
    raise NonStandardConstantError(name)


# Only ever run over text a parser has looked at, so it can be loose: it skips what it
# does not recognise. Strings come before comments so a "//" inside one stays a string.
_TOKEN = re.compile(
    r"""(?P<str>"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')
      | (?P<skip>\s+|//[^\n]*|/\*.*?\*/)
      | (?P<punct>[{}\[\]:,])
      | (?P<bare>[^\s{}\[\]:,"'/]+)""",
    re.VERBOSE | re.DOTALL,
)
_IDENTIFIER = re.compile(r"[A-Za-z_$][\w$]*")
_JSON5_ERROR = re.compile(r"<string>:(\d+) (.*) at column (\d+)", re.DOTALL)


def _tokens(text: str) -> Iterator[tuple[str, str, int]]:
    for m in _TOKEN.finditer(text):
        if m.lastgroup != "skip":
            yield m.lastgroup or "", m.group(), m.start()


def _member(path: str, key: str) -> str:
    return f"{path}.{key}" if _IDENTIFIER.fullmatch(key) else f"{path}[{json.dumps(key)}]"


def _duplicates(text: str, decode: Callable[[str], Any]) -> list[tuple[int, str]]:
    """Offset and JSONPath of every key that repeats an earlier key of the same object."""
    found: list[tuple[int, str]] = []
    # One frame per open container: [bracket, its path, keys seen (objects), current key or index].
    stack: list[list[Any]] = []
    prev = ("", "", 0)
    for kind, tok, offset in _tokens(text):
        top = stack[-1] if stack else None
        if kind != "punct":
            pass
        elif tok in "{[":
            stack.append([tok, _child_path(top), set(), None if tok == "{" else 0])
        elif tok in "}]":
            if stack:
                stack.pop()
        elif top is None:
            pass
        elif tok == "," and top[0] == "[":
            top[3] += 1
        elif tok == ":" and top[0] == "{":
            key = decode(prev[1]) if prev[0] == "str" else prev[1]
            if key in top[2]:
                found.append((prev[2], _member(top[1], key)))
            top[2].add(key)
            top[3] = key
        prev = (kind, tok, offset)
    return found


def _child_path(parent: list[Any] | None) -> str:
    if parent is None:
        return "$"
    if parent[0] == "{":
        return _member(parent[1], parent[3] or "")
    return f"{parent[1]}[{parent[3]}]"


def _position(text: str, offset: int) -> tuple[int, int]:
    return text.count("\n", 0, offset) + 1, offset - text.rfind("\n", 0, offset)


def _duplicate_problems(text: str, decode: Callable[[str], Any], exc: DuplicateKeyError) -> list[Problem]:
    found = _duplicates(text, decode)
    if not found:  # the scanner missed what the parser saw: still fail, just without a place
        return [(None, None, f"duplicate key {exc.key!r}")]
    return [(*_position(text, offset), f"duplicate key {path}") for offset, path in found]


def check(text: str, *, strict: bool = False) -> list[Problem]:
    """Check one file's text: JSON5 syntax and duplicate keys, or strict JSON when `strict`.

    Returns:
        The problems found, empty when the text passes.

    Raises:
        RecursionError: If the text nests deeper than the parser can follow.

    """
    problems = _check_json(text, strict=strict)
    return _check_json5(text) if problems is None else problems


def _check_json(text: str, *, strict: bool) -> list[Problem] | None:
    """Problems as strict JSON, or None when the text is not strict JSON and may be JSON5."""
    try:
        # Fast path: strict JSON parses at C speed and covers most files.
        if strict:
            json.loads(text, object_pairs_hook=raise_duplicate_keys, parse_constant=_reject_constant)
        else:
            json.loads(text, object_pairs_hook=raise_duplicate_keys)
    except DuplicateKeyError as exc:
        return _duplicate_problems(text, json.loads, exc)
    except NonStandardConstantError:
        return [
            (*_position(text, offset), f"{tok} is not valid in strict JSON")
            for kind, tok, offset in _tokens(text)
            if kind == "bare" and tok in {"NaN", "Infinity", "-Infinity"}
        ]
    except json.JSONDecodeError as exc:
        return [(exc.lineno, exc.colno, f"not strict JSON: {exc.msg}")] if strict else None
    return []


def _check_json5(text: str) -> list[Problem]:
    # Imported here so a run over strict JSON never pays json5's import time.
    # NOTE: a top-level `lazy import` does the same, once 3.15 is the floor.
    import json5  # noqa: PLC0415

    # json5 re-raises an error from the hook as a plain ValueError, so keep the original here.
    duplicate: list[DuplicateKeyError] = []

    def hook(pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
        try:
            return raise_duplicate_keys(pairs)
        except DuplicateKeyError as exc:
            duplicate.append(exc)
            raise

    try:
        json5.loads(text, object_pairs_hook=hook)
    except ValueError as exc:
        if duplicate:
            return _duplicate_problems(text, json5.loads, duplicate[0])
        m = _JSON5_ERROR.fullmatch(str(exc))
        return [(int(m[1]), int(m[3]), m[2])] if m else [(None, None, str(exc))]
    return []


def is_strict(filename: str, patterns: Sequence[str]) -> bool:
    """Whether `filename` must be strict JSON: the last matching pattern decides.

    A pattern without a `/` matches the file name at any depth, one with a `/` the whole
    path, `*` matches across `/`, and a leading `!` exempts the files it matches.
    """
    if not patterns:
        return False
    from fnmatch import fnmatchcase  # noqa: PLC0415

    path = filename.replace(os.sep, "/")
    name = path.rsplit("/", 1)[-1]
    strict = False
    for pattern in patterns:
        bare = pattern.removeprefix("!")
        if fnmatchcase(path if "/" in bare else name, bare):
            strict = not pattern.startswith("!")
    return strict


def _escape(value: str, *, prop: bool = False) -> str:
    value = value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    return value.replace(":", "%3A").replace(",", "%2C") if prop else value


def report(filename: str, problem: Problem, *, annotate: bool = False) -> None:
    """Print one problem as `file:line:col: message`, plus a GitHub annotation when `annotate`."""
    line, col, msg = problem
    print(f"{filename}: {msg}" if line is None else f"{filename}:{line}:{col}: {msg}")
    if annotate:
        where = "" if line is None else f",line={line},col={col}"
        print(f"::error file={_escape(filename, prop=True)}{where},title=check-json5::{_escape(msg)}")


def main(argv: Sequence[str] | None = None) -> int:
    """Check JSON5 files for syntax errors and duplicate keys.

    Args:
        argv: options and filenames. If None, defaults to sys.argv[1:].

    Returns:
        An exit code indicating success (0) or failure (1).

    """
    args = sys.argv[1:] if argv is None else list(argv)
    strict_patterns: list[str] = []
    if any(a.startswith("-") for a in args):  # argparse costs a few ms, so only load it for options
        import argparse  # noqa: PLC0415

        parser = argparse.ArgumentParser(prog="check-json5", description="Check JSON5 files.")
        parser.add_argument(
            "--strict",
            action="append",
            default=[],
            metavar="GLOB",
            help="files matching GLOB must be strict JSON; repeat it, `!GLOB` exempts (last match wins)",
        )
        parser.add_argument("filenames", nargs="*")
        ns = parser.parse_args(args)
        args, strict_patterns = ns.filenames, ns.strict

    # Python caps int literals at 4300 digits (3.10.7+), which fails valid JSON with a long number.
    if hasattr(sys, "set_int_max_str_digits"):
        sys.set_int_max_str_digits(0)
    annotate = os.environ.get("GITHUB_ACTIONS") == "true"
    retval = 0
    for filename in args:
        if not os.path.isfile(filename):  # noqa: PTH113
            problems: list[Problem] = [(None, None, "No such file")]
        else:
            try:
                # utf-8-sig transparently strips a leading BOM if present.
                with open(filename, encoding="utf-8-sig") as f:  # noqa: PTH123
                    problems = check(f.read(), strict=is_strict(filename, strict_patterns))
            except (OSError, UnicodeDecodeError) as exc:
                problems = [(None, None, f"Failed to read ({exc})")]
            except RecursionError:  # fails this file instead of ending the run
                problems = [(None, None, "nested too deeply to check")]
        for problem in problems:
            report(filename, problem, annotate=annotate)
        retval |= bool(problems)
    return retval


if __name__ == "__main__":
    raise SystemExit(main())
