"""Tests for the check-json5 pre-commit hook."""

import json
import subprocess
import sys
from pathlib import Path

import pytest

from check_json5 import check_json5 as check_json5_module
from check_json5.check_json5 import _check_json, _check_json5, _escape, _strip_jsonc, check, is_strict, main


@pytest.fixture(autouse=True)
def _no_github_actions(monkeypatch: pytest.MonkeyPatch) -> None:
    # CI sets it, and its annotation lines would break the exact-output assertions.
    monkeypatch.delenv("GITHUB_ACTIONS", raising=False)


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


@pytest.mark.parametrize(
    "content",
    ['{"a": 1}', '// c\n{"a": [1, 2,], /* c */}', '// c\n{"a": 1, "a": 2}'],
    ids=["json", "jsonc", "jsonc-duplicate"],
)
def test_json_and_jsonc_runs_skip_slow_imports(tmp_path: Path, content: str) -> None:
    # A fresh interpreter: this one has them loaded already. Each costs a few ms per hook run.
    f = write(tmp_path, content)
    code = (
        "import json, sys; before = set(sys.modules); from check_json5.check_json5 import main; "
        f"main([{f!r}]); print(json.dumps(sorted(set(sys.modules) - before)))"
    )
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=True).stdout  # noqa: S603
    loaded = set(json.loads(out.splitlines()[-1]))  # after any problems the hook printed
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
    # Python 3.14's C parser no longer recurses on arrays, so the deep file may pass there.
    assert f"{deep}:" not in out or f"{deep}: nested too deeply to check" in out
    assert f"{bad}:1:4: Unexpected" in out


def test_mixed_pass_and_fail_returns_1(tmp_path: Path) -> None:
    assert main([write(tmp_path, "{a: 1}"), write(tmp_path, "{a:}")]) == 1


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("// c\n{\n  a: 1,\n  b: ,\n}", [(4, 6, 'Unexpected ","')]),
        ("", [(None, None, "Empty strings are not legal JSON5")]),
        ('{"a": 1,\n "a": 2}', [(2, 2, "duplicate key $.a")]),
        # Nested, in arrays, through an escape, and each one reported.
        (
            '{"x": {"a": 1, "\\u0061": 2},\n "l": [{"k": 1}, {"k": 2, "k": 3}],\n "x": 0}',
            [(1, 16, "duplicate key $.x.a"), (2, 27, "duplicate key $.l[1].k"), (3, 2, "duplicate key $.x")],
        ),
        # A key-like string or comment is not a key.
        ('{"s": "\\"a\\": 1, // no", /* "s": */ "s": 2}', [(1, 37, "duplicate key $.s")]),
        (
            "// c\n{a: 1, 'a': 2, \"b c\": {d: 1, d: 2}}",
            [(2, 8, "duplicate key $.a"), (2, 30, 'duplicate key $["b c"].d')],
        ),
    ],
)
def test_problems_carry_their_position(text: str, expected: list) -> None:
    assert check(text) == expected


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ('{"a": 1}', []),
        ("{a: 1}", [(1, 2, "not strict JSON: Expecting property name enclosed in double quotes")]),
        ("// c\n{}", [(1, 1, "not strict JSON: Expecting value")]),
        (
            '{"a": NaN,\n "b": [-Infinity]}',
            [(1, 7, "NaN is not valid in strict JSON"), (2, 8, "-Infinity is not valid in strict JSON")],
        ),
        ('{"a": 1, "a": 2}', [(1, 10, "duplicate key $.a")]),
    ],
)
def test_strict_mode(text: str, expected: list) -> None:
    assert check(text, strict=True) == expected


@pytest.mark.parametrize(
    ("filename", "patterns", "expected"),
    [
        ("a/package.json", [], False),
        ("a/package.json", ["package.json"], True),
        ("a/b/x.json", ["*.json"], True),
        ("a/tsconfig.json", ["*.json", "!tsconfig*.json"], False),
        ("a/tsconfig.json", ["!tsconfig*.json", "*.json"], True),
        ("src/x.json", ["src/*.json"], True),
        ("lib/src/x.json", ["src/*.json"], False),
    ],
)
def test_is_strict(filename: str, patterns: list[str], *, expected: bool) -> None:
    assert is_strict(filename, patterns) is expected


def test_strict_flag_applies_per_file(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    (tmp_path / "package.json").write_text('{"a": 1, // c\n}')
    (tmp_path / "tsconfig.json").write_text('{"a": 1, // c\n}')
    files = [str(tmp_path / "package.json"), str(tmp_path / "tsconfig.json")]
    assert main(["--strict", "*.json", "--strict", "!tsconfig.json", *files]) == 1
    out = capsys.readouterr().out
    assert "package.json:1:10: not strict JSON" in out
    assert "tsconfig.json" not in out


def test_output_is_file_line_col(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    f = write(tmp_path, "{a:}")
    assert main([f]) == 1
    assert capsys.readouterr().out == f'{f}:1:4: Unexpected "}}"\n'


def test_github_annotations(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("GITHUB_ACTIONS", "true")
    f = tmp_path / "a,b:c.json"
    f.write_text('{"a": 1, "a": 2}')
    missing = tmp_path / "gone.json"
    assert main([str(f), str(missing)]) == 1
    lines = capsys.readouterr().out.splitlines()
    escaped = str(f).replace(":", "%3A").replace(",", "%2C")
    assert lines[1] == f"::error file={escaped},line=1,col=10,title=check-json5::duplicate key $.a"
    assert lines[3] == f"::error file={missing},title=check-json5::No such file"


def test_message_escaping_for_annotations() -> None:
    assert _escape("50% a\nb") == "50%25 a%0Ab"
    assert _escape("a:b,c", prop=True) == "a%3Ab%2Cc"


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ('{"a": 1, // c\n "b": [1, 2,], /* c\n c */}', '{"a": 1,     \n "b": [1, 2 ]      \n     }'),
        # Only a comma after a value goes.
        ("[,]", "[,]"),
        ("[1,,]", "[1,,]"),
        ("{,}", "{,}"),
        # Strings keep what looks like a comment or a trailing comma.
        ('{"u": "http://x", "s": "a,]"}', '{"u": "http://x", "s": "a,]"}'),
        ("['//', 1,]", "['//', 1 ]"),
        # An unclosed comment stays, so the JSON parse fails.
        ('{"a": 1} /* open', '{"a": 1} /* open'),
    ],
)
def test_strip_jsonc(text: str, expected: str) -> None:
    assert _strip_jsonc(text) == expected


def test_jsonc_duplicate_keys_carry_their_position() -> None:
    assert check('// c\n{"a": 1,\n "a": 2,}') == [(3, 2, "duplicate key $.a")]


def test_jsonc_shortcut_never_changes_a_verdict() -> None:
    # Random token soup, mostly invalid: the shortcut must agree with the strict-JSON-then-json5
    # path everywhere. Raw U+2028/U+2029 in strings are left out: JSON and the JSON5 spec allow
    # them, the json5 package rejects them, and the shortcut sides with the spec.
    import random  # noqa: PLC0415

    def without_shortcut(text: str) -> list:
        problems = _check_json(text, strict=False)
        return _check_json5(text) if problems is None else problems

    pieces = ["{", "}", "[", "]", ":", ",", ",", '"a"', '"b"', "'a'", "a", "1", "-1.5", "0x1F", "NaN", "true", "null",
              "// c\n", "//", "/* c */", "/*", "*/", "\n", "\r", " ", '"//"', '",]"', "\\", '"a\\\n"', '"']  # fmt: skip
    rng = random.Random(5)  # noqa: S311 - a fixed seed for a repeatable test, not security
    for _ in range(5000):
        text = "".join(rng.choice(pieces) for _ in range(rng.randint(1, 20)))
        assert bool(check(text)) == bool(without_shortcut(text)), text


def test_line_comment_ends_at_every_json5_line_terminator() -> None:
    # The code after U+2028 is not part of the comment, so its error is found.
    assert check('{"a": 1} // c\u2028 x')
    assert check('{"a": 1} // c\u2028') == []


# Generous for a loaded CI runner; the quadratic version needed minutes.
LINEAR_TIME_LIMIT_S = 5


@pytest.mark.parametrize("opener", ["/* ", '"\\', "'\\"], ids=["comment", "double", "single"])
def test_unclosed_openers_take_linear_time(opener: str) -> None:
    # Each opener used to rescan to the end of the text: 80,000 took minutes, now well under a second.
    import time  # noqa: PLC0415

    text = '{"a": 1} ' + opener * 80_000
    start = time.perf_counter()
    assert check(text)
    assert time.perf_counter() - start < LINEAR_TIME_LIMIT_S


def test_unclosed_comment_that_ends_in_its_own_star_slash_stays() -> None:
    assert _strip_jsonc('{"a":1} /*/') == '{"a":1} /*/'
    assert check('{"a":1} /*/')


def test_duplicate_then_undecodable_string(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    # The scan reads past the first repeat, where a key may not decode; that once crashed the run.
    dup = tmp_path / "dup.json"
    dup.write_text('[{"a": 1, "a": 2}, {"\\x": 0}]')
    bad = tmp_path / "bad.json"
    bad.write_text("{a:}")
    assert main([str(dup), str(bad)]) == 1
    out = capsys.readouterr().out
    assert f"{dup}:1:11: duplicate key $[0].a" in out
    assert f"{bad}:1:4: Unexpected" in out


def test_file_name_cannot_start_a_workflow_command(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("GITHUB_ACTIONS", "true")
    assert main([str(tmp_path / "a\n::warning::x.json")]) == 1
    plain, annotation = capsys.readouterr().out.splitlines()
    assert plain.endswith("a\\n::warning::x.json: No such file")
    assert annotation.startswith("::error file=")


def test_github_annotations_need_the_true_value(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("GITHUB_ACTIONS", "false")
    assert main([str(tmp_path / "gone.json")]) == 1
    assert "::error" not in capsys.readouterr().out


def test_annotation_message_keeps_colons_and_commas() -> None:
    assert _escape("a\rb") == "a%0Db"
    assert _escape("not strict JSON: x, y") == "not strict JSON: x, y"


def test_many_duplicates_take_linear_time() -> None:
    # Each position once rescanned the text before it: 200,000 repeats took 42 s in 1.3.0.
    import time  # noqa: PLC0415

    n = 200_000
    text = "{" + ",\n".join(['"a": 1'] * n) + "}"
    start = time.perf_counter()
    problems = check(text)
    assert time.perf_counter() - start < LINEAR_TIME_LIMIT_S
    assert len(problems) == n - 1
    assert problems[-1] == (n, 1, "duplicate key $.a")


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        # A bare key directly against a comment.
        ("{a/*x*/: 1, a: 2}", [(1, 13, "duplicate key $.a")]),
        ("{$a: 1, $a: 2}", [(1, 9, "duplicate key $.$a")]),
        # CRLF line ends count once.
        ('{"a": 1,\r\n"a": 2}', [(2, 1, "duplicate key $.a")]),
    ],
)
def test_more_duplicate_positions(text: str, expected: list) -> None:
    assert check(text) == expected


def test_strict_mode_rejects_bare_infinity() -> None:
    assert check('{"a": Infinity}', strict=True) == [(1, 7, "Infinity is not valid in strict JSON")]


def test_strict_file_may_start_with_a_bom(tmp_path: Path) -> None:
    # Strict mode never falls back to json5, so only the utf-8-sig read accepts the BOM.
    f = tmp_path / "a.json"
    f.write_bytes(b'\xef\xbb\xbf{"a": 1}')
    assert main(["--strict", "*", str(f)]) == 0


@pytest.mark.parametrize(
    ("filename", "patterns", "expected"),
    [
        ("src/a.json", ["*.json", "!src/*.json"], False),
        ("A.JSON", ["*.json"], False),
        ("./sub/a.json", ["sub/*.json"], True),
    ],
)
def test_is_strict_edge_cases(filename: str, patterns: list[str], *, expected: bool) -> None:
    assert is_strict(filename, patterns) is expected


def test_unlocated_duplicate_still_fails(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(check_json5_module, "_duplicates", lambda *_: [])
    assert check('{"a": 1, "a": 2}') == [(None, None, "duplicate key 'a'")]


def test_unreadable_file_fails(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    f = write(tmp_path, "{}")

    def refuse(*_args: object, **_kwargs: object) -> None:
        raise PermissionError(13, "Permission denied")

    monkeypatch.setattr("builtins.open", refuse)
    assert main([f]) == 1
    assert "Failed to read" in capsys.readouterr().out


def test_recursion_error_fails_only_that_file(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    # Python 3.14's C parser no longer recurses, so force the error to test the handler everywhere.
    def deep(*_args: object, **_kwargs: object) -> None:
        raise RecursionError

    monkeypatch.setattr(check_json5_module, "check", deep)
    f = write(tmp_path, "[]")
    assert main([f]) == 1
    assert f"{f}: nested too deeply to check" in capsys.readouterr().out
