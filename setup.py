"""Placeholder that pre-commit installs for the hook: it pins the prebuilt jsonvet wheel.

pre-commit runs `pip install .` on a hook repository. Without this file that would build the
Rust project from source. The release workflow keeps the pinned version equal to the tag.
"""

from setuptools import setup

setup(name="pre_commit_placeholder_package", version="0.0.0", install_requires=["jsonvet==2.0.0a0"])
