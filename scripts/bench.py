"""Time the hook on strict-JSON files with and without the stdlib json fast path.

Run with `uv run python scripts/bench.py`. Fixtures are generated into a temporary
directory, so nothing large is committed. Each timing is the best of 10 runs, and a full
run takes about two minutes: json5 alone needs several seconds per 2 MB parse.
"""

import json
import tempfile
import timeit
from pathlib import Path
from unittest import mock

from check_json5 import check_json5

SIZES = [1_000, 50_000, 500_000, 2_000_000]  # target bytes per fixture
REPEAT = 10  # runs per timing; the best one is reported


def make_json(target_bytes: int) -> str:
    """Build a deterministic, config-like JSON document of roughly `target_bytes`."""

    def record(i: int) -> dict:
        return {"id": i, "name": f"item-{i}", "enabled": i % 2 == 0, "weight": i * 0.25, "tags": ["a", "b"]}

    # Every record pretty-prints to about the same size, so one sample sets the count.
    count = max(1, target_bytes // len(json.dumps([record(10_000)], indent=2)))
    records = [record(i) for i in range(count)]
    return json.dumps(records, indent=2)


def best_ms(path: Path, repeat: int) -> float:
    """Return the fastest of `repeat` runs of the hook on `path`, in milliseconds."""
    return min(timeit.repeat(lambda: check_json5.main([str(path)]), number=1, repeat=repeat)) * 1000


def main() -> None:
    """Print a Markdown table of parse times per file size."""
    no_fast_path = mock.patch.object(check_json5.json, "loads", side_effect=json.JSONDecodeError("off", "", 0))
    print("| File size | Before (`json5` only) | After (fast path) | Speedup |")
    print("| --- | --- | --- | --- |")
    with tempfile.TemporaryDirectory() as tmp:
        for size in SIZES:
            path = Path(tmp) / f"{size}.json"
            path.write_text(make_json(size), encoding="utf-8")
            after = best_ms(path, repeat=REPEAT)
            with no_fast_path:
                before = best_ms(path, repeat=REPEAT)
            kb = path.stat().st_size / 1000
            print(f"| {kb:.0f} KB | {before:.1f} ms | {after:.2f} ms | ~{before / after:.0f}x |")


if __name__ == "__main__":
    main()
