#!/bin/sh
# Formatting, lints, the tests (run under coverage, saved to
# target/lcov.info) and the CRAP gate. Run by the pre-push hook; CI can call it too.
# Needs: cargo install cargo-llvm-cov cargo-crap --locked
set -eu

cd "$(git rev-parse --show-toplevel)"

for tool in llvm-cov crap; do
  command -v "cargo-$tool" >/dev/null 2>&1 || {
    echo "missing cargo-$tool: cargo install cargo-llvm-cov cargo-crap --locked" >&2
    exit 2
  }
done

echo "pre-push: cargo fmt --check"
cargo fmt --check

echo "pre-push: cargo clippy --all-targets"
cargo clippy --all-targets --quiet

echo "pre-push: tests with coverage"
cargo llvm-cov --quiet --lcov --output-path target/lcov.info

echo "pre-push: CRAP gate"
cargo crap --lcov target/lcov.info --fail-above

echo "pre-push: all checks passed"
