#!/bin/sh
# Formatting and lints. Run by the pre-commit hook; CI can call it too.
set -eu

cd "$(git rev-parse --show-toplevel)"

echo "pre-commit: cargo fmt --check"
cargo fmt --check

echo "pre-commit: cargo clippy --all-targets"
cargo clippy --all-targets --quiet

echo "pre-commit: all checks passed"
