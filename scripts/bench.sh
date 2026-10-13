#!/usr/bin/env bash
# Time the hook against the pure-Python json5 parser with hyperfine and print one Markdown table.
#
# Usage: bash scripts/bench.sh [--quick] [--jsonc]
#   --quick  50 KB file only, one run each (for CI)
#   --jsonc  the same files with a leading comment, saved as .jsonc (the JSONC fast path)
#
# Fixtures are plain JSON from make_json in scripts/bench.py, written to a temp dir.
# Each time includes Python startup, so it is the cost of one hook call per file.
set -euo pipefail

sizes=50000,500000,2000000
runs=(--warmup 1)
ext=json
for arg in "$@"; do
  case $arg in
  --quick)
    sizes=50000
    runs=(--runs 1)
    ;;
  --jsonc) ext=jsonc ;;
  *)
    echo "usage: $0 [--quick] [--jsonc]" >&2
    exit 2
    ;;
  esac
done

command -v hyperfine >/dev/null || {
  echo "hyperfine not found" >&2
  exit 127
}

cd "$(dirname "$0")/.."
py=$(uv run python -c 'import sys; print(sys.executable)')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

"$py" - "$tmp" "$sizes" "$ext" <<'EOF'
import sys
from pathlib import Path

sys.path.insert(0, "scripts")
from bench import make_json

ext = sys.argv[3]
prefix = "// a comment\n" if ext == "jsonc" else ""
for size in sys.argv[2].split(","):
    Path(sys.argv[1], f"{size}.{ext}").write_text(prefix + make_json(int(size)), encoding="utf-8")
EOF

# Short command lines, so the table's Command column stays readable.
cd "$tmp"
PATH="$(dirname "$py"):$PATH"
hyperfine "${runs[@]}" -N --style basic --export-markdown table.md -L size "$sizes" \
  "python -m check_json5.check_json5 {size}.$ext" \
  "python -c 'import json5,sys; json5.loads(open(sys.argv[1]).read())' {size}.$ext" \
  >&2
cat table.md
