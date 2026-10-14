"""Conformance driver for the Python parsers. Usage: python driver.py <parser> < paths.

Prints "ready", then "<path> TAB <accept|reject|crash>" per path. See conformance/README.md.
"""

import json
import sys
from collections.abc import Callable
from pathlib import Path

import json5
import jsonlines
import pyjson5


class DuplicateKeyError(ValueError):
    """Raised by the object hook that turns duplicate detection on for the json module."""


def no_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
    """Build an object, refusing a key seen twice."""
    if len({key for key, _ in pairs}) != len(pairs):
        raise DuplicateKeyError
    return dict(pairs)


def read_jsonlines(path: str) -> None:
    """Read every line with the library's own file reader."""
    with jsonlines.open(path) as reader:
        for _ in reader:
            pass


# Each parser takes a path and the file's text, and raises one of its error types to reject.
PARSERS: dict[str, tuple[Callable[[str, str], object], tuple[type[Exception], ...]]] = {
    "py-json": (lambda _, text: json.loads(text), (json.JSONDecodeError,)),
    "py-json5": (lambda _, text: json5.loads(text), (ValueError,)),
    "py-pyjson5": (lambda _, text: pyjson5.loads(text), (pyjson5.Json5DecoderException,)),
    "py-jsonlines": (lambda path, _: read_jsonlines(path), (jsonlines.InvalidLineError,)),
    # Variants with duplicate detection on, for the duplicate-key table only.
    "py-json-dupes": (
        lambda _, text: json.loads(text, object_pairs_hook=no_duplicates),
        (json.JSONDecodeError, DuplicateKeyError),
    ),
    "py-json5-dupes": (lambda _, text: json5.loads(text, allow_duplicate_keys=False), (ValueError,)),
}


def verdict(path: str, parse: Callable[[str, str], object], errors: tuple[type[Exception], ...]) -> str:
    """Parse one file and name the outcome."""
    try:
        text = Path(path).read_bytes().decode("utf-8")
    except UnicodeDecodeError:
        return "reject"  # not UTF-8, so the parser never sees it
    try:
        parse(path, text)
    except errors:
        return "reject"
    except Exception:  # noqa: BLE001 - any other exception type is a crash by definition
        return "crash"
    return "accept"


def main() -> None:
    """Read paths from stdin and print one verdict per path."""
    parse, errors = PARSERS[sys.argv[1]]
    print("ready", flush=True)
    for line in sys.stdin:
        path = line.rstrip("\n")
        if path:
            print(f"{path}\t{verdict(path, parse, errors)}", flush=True)


if __name__ == "__main__":
    main()
