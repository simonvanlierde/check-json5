"""Run the jsonvet binary, so `python -m jsonvet` works like the command."""

import os
import subprocess
import sys
import sysconfig
from pathlib import Path


def find_binary(name: str = "jsonvet") -> str:
    """Return the path of a binary this wheel installed.

    It is in the scripts directory of the environment that holds this package. That is usually
    the running interpreter's, but not when uv links a cached environment into a temporary one
    (`uv run --with`), or for `pip install --target`. So look next to the package too.
    """
    exe = name + (".exe" if sys.platform == "win32" else "")
    site_packages = Path(__file__).resolve().parent.parent
    candidates = [Path(sysconfig.get_path("scripts")) / exe]
    candidates.extend(
        Path(sysconfig.get_path("scripts", scheme)) / exe
        for scheme in ("posix_user", "nt_user", "osx_framework_user")
        if scheme in sysconfig.get_scheme_names()
    )
    candidates += [
        site_packages.parents[2] / "bin" / exe,  # <env>/lib/pythonX.Y/site-packages
        site_packages.parent.parent / "Scripts" / exe,  # <env>\Lib\site-packages on Windows
        site_packages / "bin" / exe,  # pip install --target
    ]
    for path in candidates:
        if path.is_file():
            return str(path)
    raise FileNotFoundError(exe)


def main() -> None:
    """Replace this process with the binary, passing the arguments through."""
    binary = find_binary()
    if sys.platform == "win32":
        sys.exit(subprocess.call([binary, *sys.argv[1:]]))  # noqa: S603 - our own binary, the user's own arguments
    os.execv(binary, [binary, *sys.argv[1:]])  # noqa: S606 - same


if __name__ == "__main__":
    main()
