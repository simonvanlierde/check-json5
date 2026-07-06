"""Tests for the check-json5 pre-commit hook."""

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
