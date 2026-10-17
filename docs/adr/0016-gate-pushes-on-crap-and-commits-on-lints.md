# 16. Gate pushes on CRAP and commits on lints

Date: 2026-10-17

## Status

Accepted

## Context

Clippy's `cognitive_complexity` lint caps nesting-heavy functions, but it ignores branch count and test coverage. A function with many branches passes it, and it passes whether or not a test reaches it. The CRAP score (`CC² × (1 − coverage)³ + CC`, from `cargo-crap`) weighs cyclomatic complexity by coverage, so a complex function must be well tested. CRAP needs an LCOV file, and only a test run produces one. Tests are too slow to run on every commit.

## Decision

- `scripts/pre-commit.sh` runs `cargo fmt --check` and `cargo clippy --all-targets`. The pre-commit hook calls it.
- `scripts/pre-push.sh` runs fmt, clippy, the tests under `cargo llvm-cov` (saving `target/lcov.info`), then `cargo crap --fail-above`. The pre-push hook calls it. It stands alone and does not call `pre-commit.sh`.
- The threshold is 30 (`.cargo-crap.toml`). Fix a failure by splitting the function or testing it, not by raising the threshold.
- The hook files in `.cargo-husky/hooks/` only run the matching script. A CI job calls the same scripts.

## Consequences

- Commits stay fast. Complexity regressions surface at push, not at commit.
- fmt and clippy appear in both scripts.
- `cargo-llvm-cov` and `cargo-crap` must be installed (`cargo install cargo-llvm-cov cargo-crap --locked`). The script names the missing tool.
- Editing a hook file does not update `.git/hooks` on its own. Run `cargo clean -p cargo-husky && cargo test --no-run` to reinstall.
