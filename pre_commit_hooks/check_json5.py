"""Based on https://gitlab.com/bmares/check-json5/-/blob/db9b5c0f76dea9b0f28097b4421e4b40f39b2266/pypackages/pre_commit_hooks/check_json5.py."""

import argparse
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any

import json5


def raise_duplicate_keys(ordered_pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
    """Raise an error if there are duplicate keys in the JSON object.

    Args:
        ordered_pairs: List of key-value pairs from a JSON5 object.

    Returns:
        A dictionary constructed from the key-value pairs.

    Raises:
        ValueError: If a duplicate key is found.

    """
    d = {}
    for key, val in ordered_pairs:
        if key in d:
            msg = f"Duplicate key: {key}"
            raise ValueError(msg)
        d[key] = val
    return d


def main(argv: Sequence[str] | None = None) -> int:
    """Check JSON5 files for syntax errors and duplicate keys.

    Args:
        argv: list of command-line arguments. If None, defaults to sys.argv.

    Returns:
        An exit code indicating success (0) or failure (1).

    """
    parser = argparse.ArgumentParser()
    parser.add_argument("filenames", nargs="*", help="Filenames to check.")
    args = parser.parse_args(argv)
    retval = 0
    for filename in args.filenames:
        with Path(filename).open("rb") as f:
            try:
                json5.load(f, object_pairs_hook=raise_duplicate_keys)
            except ValueError as exc:
                print(f"{filename}: Failed to json decode ({exc})")
                retval = 1
    return retval


if __name__ == "__main__":
    raise SystemExit(main())
