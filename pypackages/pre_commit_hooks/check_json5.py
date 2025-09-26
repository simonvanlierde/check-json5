# Source: <https://github.com/pre-commit/pre-commit-hooks/blob/a2cdab0afed18f1b2b073c3f7b68f109fd228d04/pre_commit_hooks/check_json.py>  # noqa: E501
from __future__ import annotations

import argparse
from typing import TYPE_CHECKING, Any

import json5

if TYPE_CHECKING:
    from collections.abc import Sequence


def raise_duplicate_keys(
    ordered_pairs: list[tuple[str, Any]],
) -> dict[str, Any]:
    d = {}
    for key, val in ordered_pairs:
        if key in d:
            raise ValueError(f'Duplicate key: {key}')
        d[key] = val
    return d


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('filenames', nargs='*', help='Filenames to check.')
    args = parser.parse_args(argv)

    retval = 0
    for filename in args.filenames:
        with open(filename, 'rb') as f:
            try:
                json5.load(f, object_pairs_hook=raise_duplicate_keys)
            except ValueError as exc:
                print(f'{filename}: Failed to json decode ({exc})')
                retval = 1
    return retval


if __name__ == '__main__':
    raise SystemExit(main())
