"""Based on https://gitlab.com/bmares/check-json5/-/blob/db9b5c0f76dea9b0f28097b4421e4b40f39b2266/pypackages/pre_commit_hooks/check_json5.py."""

from __future__ import annotations

import json
import os
import sys

# The hook starts once per commit, so its imports are a large share of a run:
# the type-only ones load for the type checker alone.
TYPE_CHECKING = False
if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence
    from typing import Any


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
        argv: the filenames to check. If None, defaults to sys.argv[1:].

    Returns:
        An exit code indicating success (0) or failure (1).

    """
    retval = 0
    for filename in sys.argv[1:] if argv is None else argv:
        if not os.path.isfile(filename):  # noqa: PTH113
            print(f"{filename}: No such file")
            retval = 1
            continue
        try:
            # utf-8-sig transparently strips a leading BOM if present.
            with open(filename, encoding="utf-8-sig") as f:  # noqa: PTH123
                text = f.read()
        except (OSError, UnicodeDecodeError) as exc:
            print(f"{filename}: Failed to read ({exc})")
            retval = 1
            continue
        try:
            # Fast path: strict JSON parses at C speed and covers most files.
            # Fall back to the pure-Python json5 parser only for comments/trailing commas.
            try:
                json.loads(text, object_pairs_hook=raise_duplicate_keys)
            except json.JSONDecodeError:
                # Imported here so a run over strict JSON never pays json5's import time.
                # NOTE: a top-level `lazy import` does the same, once 3.15 is the floor.
                import json5  # noqa: PLC0415

                json5.loads(text, object_pairs_hook=raise_duplicate_keys)
        except ValueError as exc:
            print(f"{filename}: Failed to json decode ({exc})")
            retval = 1
    return retval


if __name__ == "__main__":
    raise SystemExit(main())
