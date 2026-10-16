# AGENTS.md

A Herdr plugin (`herdr-plugin.toml`, id `dagadbm`) written in Rust. It is macOS only because it depends on `alerter` and `lsappinfo`. One binary runs each feature by name: `dagadbm <feature> [args]`.

## Layout

- `src/main.rs` reads the feature name and passes the remaining arguments to that feature.
- `src/<feature>/` holds all of one feature's code and assets. A feature imports only its own code and `src/shared/`.
- `config.example.toml` has one section per feature and lists every key with its default.
- [docs/architecture.md](docs/architecture.md): how the plugin works, its invariants, its runtime state, and where to change what. Start here.
- [docs/glossary.md](docs/glossary.md): every term, its code identifiers, and Herdr's name for it.
- [docs/dev-guidelines.md](docs/dev-guidelines.md): recipes for common changes, code conventions, running locally and shipping.
- [docs/test-guidelines.md](docs/test-guidelines.md): unit and integration tests, stubs, the isolated test Herdr server, and the rule that tests never call real tools.
- [docs/adr/](docs/adr/): one ADR per decision. Add a new ADR, or update the existing one, when a decision changes.

## Commands

```sh
# Format, lint and test. All three must be clean before committing.
# The first `cargo test` in a clone installs the pre-push hook (cargo-husky, .cargo-husky/hooks/).
# Tests that need Herdr start their own isolated server (needs herdr installed).
cargo fmt && cargo clippy --all-targets && cargo test

# Regenerate the module dependency graph in docs/architecture.md.
docs/update.sh

# Build and try the checkout locally. Linking does not build; rebuild after each change.
cargo build --release && herdr plugin link .
herdr plugin log list --plugin dagadbm   # hook output and errors
herdr plugin unlink dagadbm              # before going back to the installed plugin

# Release: the installed plugin comes from GitHub.
git push origin main
~/dotfiles/herdr-update.sh               # reinstalls the plugin and the Claude integration
herdr plugin list                        # pinned commit must equal: git rev-parse HEAD
```

## Workflow

- `cargo fmt && cargo clippy --all-targets && cargo test` must be clean before committing.
- When modules or `use crate::` imports change (any reference from one module to another), run `docs/update.sh` and commit the result.
