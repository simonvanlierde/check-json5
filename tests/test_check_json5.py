"""Tests for the check-json5 pre-commit hook."""

import json
import subprocess
import sys
from pathlib import Path

from pre_commit_hooks.check_json5 import main


def write(tmp_path: Path, content: str) -> str:
    """Write content to a temp .json5 file and return its path."""
    f = tmp_path / "f.json5"
    f.write_text(content)
    return str(f)


def test_valid_json5_passes(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{a: 1, /* comment */ b: 2,}")]) == 0


def test_invalid_syntax_fails(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{a: 1, b:}")]) == 1


def test_duplicate_keys_fail(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{a: 1, a: 2}")]) == 1


def test_nested_duplicate_keys_fail(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{outer: {a: 1, a: 2}}")]) == 1


def test_strict_json_fast_path_passes(tmp_path: Path) -> None:
    # Strict JSON takes the stdlib fast path, never reaching json5.
    assert main([write(tmp_path, '{"a": 1, "b": 2}')]) == 0


def test_strict_json_run_skips_slow_imports(tmp_path: Path) -> None:
    # A fresh interpreter: this one has them loaded already. Each costs a few ms per hook run.
    f = write(tmp_path, '{"a": 1}')
    code = (
        "import json, sys; before = set(sys.modules); from pre_commit_hooks.check_json5 import main; "
        f"main([{f!r}]); print(json.dumps(sorted(set(sys.modules) - before)))"
    )
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=True).stdout  # noqa: S603
    loaded = set(json.loads(out))
    assert not loaded & {"argparse", "json5", "pathlib", "typing"}, loaded


def test_strict_json_duplicate_keys_fail(tmp_path: Path) -> None:
    # Dup keys must fail on the fast path, not fall back to json5.
    assert main([write(tmp_path, '{"a": 1, "a": 2}')]) == 1


def test_non_utf8_file_fails_cleanly(tmp_path: Path) -> None:
    # An undecodable file fails that file, not the whole run.
    f = tmp_path / "f.json5"
    f.write_bytes(b'{"a": "\xff\xfe"}')
    assert main([str(f)]) == 1


def test_missing_file_fails(tmp_path: Path) -> None:
    assert main([str(tmp_path / "nope.json5")]) == 1


def test_no_files_passes() -> None:
    assert main([]) == 0
