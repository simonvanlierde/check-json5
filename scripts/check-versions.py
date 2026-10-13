# /// script
# requires-python = ">=3.11"
# ///
"""Check that every place naming jsonvet's release version agrees.

The crates' version (Cargo.toml), the wheel pin in the hook placeholder (setup.py), and the
check-json5 compatibility package's version and pin (compat/check-json5/pyproject.toml) must
name the same release, or a hook install would pull another version than the tag.
"""

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PINS = 2  # setup.py and the compatibility package each pin jsonvet once


def pep440(cargo: str) -> str:
    """Cargo's `2.0.0-alpha.1` as Python's `2.0.0a1`."""
    for long, short in (("-alpha.", "a"), ("-beta.", "b"), ("-rc.", "rc")):
        cargo = cargo.replace(long, short)
    return cargo


def main() -> int:
    """Print each version and exit 1 when they differ."""
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    compat = tomllib.loads((ROOT / "compat" / "check-json5" / "pyproject.toml").read_text())["project"]
    pins = re.findall(r"jsonvet==([^\"']+)", (ROOT / "setup.py").read_text()) + [
        dep.split("==")[1] for dep in compat["dependencies"] if dep.startswith("jsonvet==")
    ]
    found = {"Cargo.toml": pep440(cargo), "compat version": compat["version"]}
    found.update({f"pin {i + 1}": pin for i, pin in enumerate(pins)})
    for where, version in found.items():
        print(f"{where:16} {version}")
    if len(pins) != PINS or len(set(found.values())) != 1:
        print("versions differ", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
