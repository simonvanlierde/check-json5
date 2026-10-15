# Developer tasks. Each recipe runs the command CI runs for the same step (.github/workflows/).
# Tool versions: mise.toml.

# Where cargo puts its output, for the recipes that run the release binary.
target := env("CARGO_TARGET_DIR", "target")

default:
    @just --list

# Once per clone: the json5-tests submodule and the git hooks (commit and push).
setup:
    git submodule update --init
    prek install

# ci.yml `lint`, apart from the generated-page diff (see `matrix-check`).
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    cargo deny check
    prek run --all-files ruff-check
    prek run --all-files ruff-format
    uv run conformance/run.py --check

# ci.yml `test`. nextest skips doctests, so they run on their own.
test:
    cargo nextest run --workspace
    cargo test --workspace --doc

# Everything a pull request needs to pass, except the other operating systems.
check: lint test
    uv run scripts/check-versions.py

# ci.yml `msrv`: the oldest Rust the crates support (`rust-version` in Cargo.toml).
msrv:
    cargo +1.88 check --workspace --lib --bins --locked

# Re-run the conformance matrix (needs docker), then render docs/conformance.md and the README block.
matrix:
    cargo build --release -p jsonvet-cli
    uv run conformance/run.py --jsonvet {{ target }}/release/jsonvet
    uv run conformance/report.py

# Refresh the matrix's lock files after you change a parser pin, then run `just matrix`.
matrix-lock:
    uv run conformance/run.py --lock

# ci.yml `lint`: the rendered page is up to date. Fails on uncommitted edits to those two files.
matrix-check:
    uv run conformance/report.py
    git diff --exit-code docs/conformance.md README.md

# Fuzz one target for 20 minutes, as fuzz.yml does: check, diff_serde_json or diff_json5.
fuzz TARGET:
    cd fuzz && cargo +nightly fuzz run -O {{ TARGET }} -- -max_total_time=1200 -print_final_stats=1

# Mutation testing over both crates; about 8 minutes.
mutants:
    cargo mutants --test-workspace=true

# Time jsonvet against check-json5 1.4.1 (needs hyperfine).
bench:
    cargo build --release -p jsonvet-cli
    uv run scripts/bench.py --jsonvet {{ target }}/release/jsonvet
