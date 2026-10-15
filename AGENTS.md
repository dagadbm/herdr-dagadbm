# AGENTS.md

A Herdr plugin (`herdr-plugin.toml`, id `dagadbm`) written in Rust. It is macOS only because it depends on `alerter` and `lsappinfo`. One binary runs each feature by name: `dagadbm <feature> [args]`.

## Layout

- `src/main.rs` reads the feature name and passes the remaining arguments to that feature.
- `src/<feature>/` holds all of one feature's code and assets. Each feature imports only its own code and `src/shared/`. Code that two features need goes in `src/shared/`.
- `config.example.toml` has one section per feature and lists every key with its default. It must match the defaults in `src/notify/config.rs`; a test enforces this.
- `docs/adr/` holds one ADR per decision. Add a new ADR, or update the existing one, when a decision changes.

## Adding a feature

1. Create `src/<feature>/` with a `run(args)` entry point.
2. Register it in `src/main.rs`: the `mod` line, the `match` arm and the `USAGE` list.
3. Add its section to `config.example.toml`.
4. Add its hooks to `herdr-plugin.toml` under a `# --- <feature> ---` comment.
5. Add an integration test in `tests/<feature>_cli_test.rs`.

The feature is done when `dagadbm <feature> --help` works and the Workflow checks pass.

## Herdr facts

- Herdr's `config.toml` rejects unknown keys, so plugin settings live in `$HERDR_PLUGIN_CONFIG_DIR/config.toml`.
- Event data comes from `HERDR_PLUGIN_EVENT` and `HERDR_PLUGIN_EVENT_JSON`. The plugin runs the `herdr` CLI (`HERDR_BIN_PATH`) for workspace, tab, pane and worktree details.
- Template tokens use only Herdr's names: `{workspace}`, `{tab}`, `{agent}` and so on, plus `$name` for pane metadata (ADR 5).
- `alerter` refuses an empty `--message`.

## Workflow

- `cargo fmt && cargo clippy --all-targets && cargo test` must be clean before committing.
- Integration tests in `tests/` run only against stubs of `herdr`, `git`, `alerter` and `lsappinfo` on `PATH`. A test must never call the real tools.
- Local development: `cargo build --release && herdr plugin link .`. Linking does not run the build, and Herdr refuses `plugin install` over a linked plugin.
- Use `herdr plugin log list --plugin dagadbm` to see hook output and errors.

## Updating the installed plugin

The installed plugin comes from GitHub, so push to `main` first. Then run `~/dotfiles/herdr-update.sh`, which reinstalls the plugin and the Claude integration. Afterwards, check that the commit pinned in `herdr plugin list` matches `git rev-parse HEAD`.
