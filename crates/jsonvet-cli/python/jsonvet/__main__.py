"""Run the jsonvet binary, so `python -m jsonvet` works like the command."""

import os
import subprocess
import sys
import sysconfig


def find_binary(name: str = "jsonvet") -> str:
    """Return the path of a binary installed by this wheel, next to the interpreter's scripts."""
    exe = name + (".exe" if sys.platform == "win32" else "")
    for scheme in (sysconfig.get_default_scheme(), "posix_user", "nt_user"):
        try:
            path = os.path.join(sysconfig.get_path("scripts", scheme), exe)
        except KeyError:
            continue
        if os.path.isfile(path):
            return path
    raise FileNotFoundError(exe)


if __name__ == "__main__":
    binary = find_binary()
    if sys.platform == "win32":
        sys.exit(subprocess.call([binary, *sys.argv[1:]]))
    os.execv(binary, [binary, *sys.argv[1:]])
