"""Tests for the check-json5 pre-commit hook."""

import json
import subprocess
import sys
from pathlib import Path

import pytest

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


@pytest.mark.parametrize(
    ("name", "content", "expected"),
    [
        ("utf8_bom", b'\xef\xbb\xbf{"a": 1}', 0),
        ("empty_file", b"", 1),
        ("whitespace_only", b"  \n ", 1),
        ("top_level_scalar", b"42", 0),
        ("nan_infinity", b'{"a": NaN, "b": Infinity, "c": -Infinity}', 0),
        ("duplicate_keys_in_array_of_objects", b'[{"a": 1, "a": 2}]', 1),
        ("duplicate_keys_on_json5_fallback", b'// comment\n{"a": 1, "a": 2}', 1),
    ],
)
def test_edge_cases(tmp_path: Path, name: str, content: bytes, expected: int) -> None:
    f = tmp_path / f"{name}.json"
    f.write_bytes(content)
    assert main([str(f)]) == expected


def test_int_over_4300_digits_passes(tmp_path: Path) -> None:
    assert main([write(tmp_path, '{"n": ' + "1" * 5000 + "}")]) == 0


@pytest.mark.parametrize("prefix", ["", "// comment\n"], ids=["fast_path", "json5_fallback"])
def test_nesting_past_recursion_limit_fails_only_that_file(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], prefix: str
) -> None:
    deep = tmp_path / "deep.json"
    deep.write_text(prefix + "[" * 100_000 + "]" * 100_000)
    bad = tmp_path / "bad.json"
    bad.write_text("{a:}")
    assert main([str(deep), str(bad)]) == 1
    out = capsys.readouterr().out
    assert f"{deep}: Failed" in out
    assert f"{bad}: Failed" in out


def test_mixed_pass_and_fail_returns_1(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{a: 1}"), write(tmp_path, "{a:}")]) == 1
