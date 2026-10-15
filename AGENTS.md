# AGENTS.md

A Herdr plugin (`herdr-plugin.toml`, id `dagadbm`) written in Rust. It is macOS only. One binary runs each feature by name: `dagadbm <feature> [args]`.

## Layout

- `src/main.rs` reads the feature name and passes the remaining arguments to that feature.
- `src/<feature>/` holds all of one feature's code and assets. A feature never imports from another feature. Code that two features need moves to `src/shared/`, which does not exist yet.
- `config.example.toml` has one section per feature and lists every key with its default. `notify` embeds it with `include_str!`. Keep it in sync with the defaults in `src/notify/config.rs`; a test checks that both parse to the same values.
- `docs/adr/` holds one ADR per decision. Add a new ADR, or update the existing one, when a decision changes.

## Herdr facts

- Herdr's `config.toml` rejects unknown keys, so plugin settings live in `$HERDR_PLUGIN_CONFIG_DIR/config.toml`.
- Event data comes from `HERDR_PLUGIN_EVENT` and `HERDR_PLUGIN_EVENT_JSON`. The plugin runs the `herdr` CLI (`HERDR_BIN_PATH`) for workspace, tab, pane and worktree details.
- Template tokens use Herdr's names only: `{workspace}`, `{tab}`, `{agent}` and so on, plus `$name` for pane metadata. Do not invent tokens (ADR 5).
- `alerter` refuses an empty `--message`.

## Workflow

- `cargo fmt && cargo clippy --all-targets && cargo test` must be clean before committing.
- Integration tests in `tests/` stub `herdr`, `git`, `alerter` and `lsappinfo` on `PATH`. Never let a test call the real tools.
- Local development: `cargo build --release && herdr plugin link .`. Linking does not run the build, and Herdr refuses `plugin install` over a linked plugin.
- Use `herdr plugin log list --plugin dagadbm` to see hook output and errors.

## Updating the installed plugin

The plugin is installed from GitHub, as in `~/dotfiles/herdr-update.sh`. Push to `main` first, then reinstall:

```sh
herdr plugin uninstall dagadbm/herdr-dagadbm || true
herdr plugin install --yes dagadbm/herdr-dagadbm
herdr plugin list   # the pinned commit must match `git rev-parse HEAD`
```

`~/dotfiles/herdr-update.sh` does the same and also reinstalls the Claude integration. Use `herdr plugin link .` only for local development.
