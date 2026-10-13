# /// script
# requires-python = ">=3.12"
# ///
"""Compare jsonvet with check-json5 1.4.1, file by file, in the two dialects 1.4 knows.

Usage: uv run scripts/differential.py [--cases N] [--seed S] [--jsonvet PATH]

It writes a corpus of random JSON-ish files plus the json5-tests suite to a temporary
directory, runs both tools over it in JSON5 mode and with --strict, and compares the verdict
for every file and, where both find no syntax error, the duplicate-key reports. Every difference
is put in a category; the ones the design accepts are listed in docs/design.md. The exit status
is 1 when a difference falls in no accepted category.
"""

import argparse
import random
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REFERENCE = ["uvx", "--quiet", "--python", "3.13", "--from", "check-json5==1.4.1", "check-json5"]
BATCH = 500  # files per run, to keep the command line short
EXAMPLES = 5  # differences shown per category

TOKENS = [
    "{", "}", "[", "]", ":", ",", ",", '"a"', '"b"', "'a'", "a", "$k", "1", "-2.5e3", "0x1F", ".5", "5.",
    "+1", "01", "NaN", "Infinity", "true", "null", "nul", "// c\n", "/* c */", "/*", "*/", "\n", "\r\n",
    " ", "\t", '"//"', '",]"', "\\", '"\\u0061"', '"\\q"', '"\\x41"', '"a\\\nb"', "é", "\u00a0", '"',
    "\u2028", "\ufeff",
]  # fmt: skip

# A JSON-shaped skeleton with a repeated key, so duplicates get compared on valid documents.
OBJECTS = [
    '{{"{a}": 1, "{b}": [{{"{a}": 2, "{a}": 3}}], "{a}": 4}}',
    "{{{a}: 1, // c\n  '{b}': {{x: 1, x: 2}},\n  {a}: 5,\n}}",
    '[{{"{a}": 1}}, {{"{b}": 2, "{b}": 3}}]',
]
KEYS = ["a", "b c", "ünï", "$x", "1st", 'q\\"', "\\u0061"]

LINE = re.compile(r"^(?P<file>[^:]+?):(?:(?P<line>\d+):(?P<col>\d+):)? ?(?P<rest>.*)$")


def corpus(directory: Path, cases: int, seed: int) -> list[Path]:
    """Write `cases` generated files and the json5-tests suite to `directory`."""
    rng = random.Random(seed)  # noqa: S311 - test data, not security
    files = []
    for i in range(cases):
        if i % 4 == 0:
            text = rng.choice(OBJECTS).format(a=rng.choice(KEYS), b=rng.choice(KEYS))
        else:
            text = "".join(rng.choice(TOKENS) for _ in range(rng.randint(1, 30)))
        path = directory / f"case-{i:05}.json"
        path.write_text(text, encoding="utf-8", newline="")
        files.append(path)
    suite = ROOT / "tests" / "json5-tests"
    for i, case in enumerate(sorted(suite.rglob("*"))):
        if case.suffix in {".json", ".json5", ".js", ".txt"}:
            path = directory / f"suite-{i:03}.json"
            shutil.copyfile(case, path)
            files.append(path)
    return files


def run(command: list[str], files: list[Path], cwd: Path) -> dict[str, list[tuple]]:
    """Problems per file name, as (line, col, kind, severity, text).

    A tool that cannot start also exits 1, but prints only to stderr; that stops the run.
    """
    found: dict[str, list[tuple]] = defaultdict(list)
    names = [f.name for f in files]
    for start in range(0, len(names), BATCH):
        out = subprocess.run(  # noqa: S603 - our own command line
            [*command, *names[start : start + BATCH]], cwd=cwd, capture_output=True, text=True, check=False
        )
        if out.returncode not in {0, 1} or (out.returncode == 1 and not out.stdout and out.stderr):
            sys.exit(f"{' '.join(command[:6])} failed with {out.returncode}: {out.stderr}")
        for line in out.stdout.splitlines():
            m = LINE.match(line)
            if not m or line.startswith("::"):
                continue
            rest = m["rest"]
            severity = "error"
            if (s := re.match(r"(error|warning)\[([\w-]+)\] (.*)", rest)) is not None:
                severity, rest = s[1], s[3]
            kind = "duplicate" if rest.startswith("duplicate key ") else "other"
            if kind == "duplicate":
                rest = rest.split(" (first at ")[0]
            found[m["file"]].append((m["line"], m["col"], kind, severity, rest))
    return found


def failed(problems: list[tuple]) -> bool:
    """Whether any problem is an error."""
    return any(p[3] == "error" for p in problems)


def duplicates(problems: list[tuple]) -> list[tuple]:
    """The duplicate-key reports, as (line, col, kind, path)."""
    return sorted((*p[:3], p[4]) for p in problems if p[2] == "duplicate")


def classify(text: str, ours: list[tuple], theirs: list[tuple]) -> str:
    """Why the two tools disagree, as an accepted category or `unexplained`."""
    if "\u2028" in text or "\u2029" in text:
        return "accepted: U+2028/U+2029 (line terminators in jsonvet, and allowed in strings by the spec)"
    if "\r" in text.replace("\r\n", ""):
        return "accepted: lone CR ends a line in jsonvet"
    if failed(ours) != failed(theirs):
        return "unexplained: verdict"
    return "unexplained: duplicate reports"


def compare(mode: str, files: list[Path], cwd: Path, jsonvet: str, extra: list[str]) -> Counter:
    """Run both tools on `files` and print how their reports differ, by category."""
    ours = run([jsonvet, "--no-config", *extra], files, cwd)
    theirs = run([*REFERENCE, *extra], files, cwd)
    tally: Counter = Counter()
    examples: dict[str, list[str]] = defaultdict(list)
    for path in files:
        a, b = ours.get(path.name, []), theirs.get(path.name, [])
        if failed(a) == failed(b):
            syntax_free = not any(p[2] == "other" and p[3] == "error" for p in a + b)
            if not syntax_free or duplicates(a) == duplicates(b):
                tally["same"] += 1
                continue
        text = path.read_text(encoding="utf-8")
        category = classify(text, a, b)
        tally[category] += 1
        if len(examples[category]) < EXAMPLES:
            examples[category].append(f"{path.name}: {text!r}\n      jsonvet: {a}\n      1.4:     {b}")
    print(f"\n== {mode}: {len(files)} files")
    for category, n in tally.most_common():
        print(f"{n:6}  {category}")
    for category, items in examples.items():
        shown = items if category.startswith("unexplained") else items[:2]
        print(f"\n-- {category}")
        print("\n".join(shown))
    return tally


def check_reference(cwd: Path) -> None:
    """Fetch the reference fresh, and check it passes a valid file and fails an invalid one.

    Otherwise a broken install would read as a file with no problems.
    """
    (cwd / "probe-ok.json").write_text("{}")
    (cwd / "probe-bad.json").write_text("[1,]")
    refresh = [REFERENCE[0], "--refresh-package", "check-json5", *REFERENCE[1:]]
    ok = subprocess.run(  # noqa: S603 - our own command line
        [*refresh, "--strict", "*", "probe-ok.json"], cwd=cwd, capture_output=True, text=True, check=False
    )
    bad = subprocess.run(  # noqa: S603 - our own command line
        [*REFERENCE, "--strict", "*", "probe-bad.json"], cwd=cwd, capture_output=True, text=True, check=False
    )
    if ok.returncode != 0 or bad.returncode != 1 or "probe-bad.json" not in bad.stdout:
        sys.exit(f"the reference does not work: {ok.stderr or bad.stderr or bad.stdout}")
    for probe in ("probe-ok.json", "probe-bad.json"):
        (cwd / probe).unlink()


def main() -> int:
    """Run the comparison in both modes; 1 when a difference is unexplained."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cases", type=int, default=3000)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--jsonvet", default=str(ROOT / "target" / "release" / "jsonvet"))
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="jsonvet-differential-") as tmp:
        cwd = Path(tmp)
        check_reference(cwd)
        files = corpus(cwd, args.cases, args.seed)
        totals = compare("json5", files, cwd, args.jsonvet, [])
        totals += compare("strict", files, cwd, args.jsonvet, ["--strict", "*"])
    unexplained = sum(n for c, n in totals.items() if c.startswith("unexplained"))
    print(f"\nunexplained differences: {unexplained}")
    return 1 if unexplained else 0


if __name__ == "__main__":
    sys.exit(main())
