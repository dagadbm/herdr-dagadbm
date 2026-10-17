# Development guidelines

How to make common changes, step by step, and the conventions the code follows. Read [architecture.md](architecture.md) first for the module map and the invariants. Testing is in [test-guidelines.md](test-guidelines.md). Terms are in [glossary.md](glossary.md).

## Checks before you commit

Run these from the repository root. All three must be clean:

```sh
cargo fmt && cargo clippy --all-targets && cargo test
```

When you add, remove or rename a module, or change a `use crate::` import, also run `docs/update.sh` and commit the changed `docs/architecture.md` (see [Refresh the dependency graph](#refresh-the-dependency-graph)).

When a decision changes, add an ADR in `docs/adr/` or update the one that records it. ADRs are numbered and named `<four-digit number>-<short-title>.md`.

## Run it locally

1. Build, then link the checkout as the installed plugin. Linking does not run the build, so build first (ADR 8):

   ```sh
   cargo build --release && herdr plugin link .
   ```

2. Herdr runs `target/release/dagadbm` for every hook (`herdr-plugin.toml`), so rebuild after each change. No relink is needed.
3. Read hook output and errors in the plugin log. stderr from the binary ends up there (ADR 4):

   ```sh
   herdr plugin log list --plugin dagadbm
   ```

4. Send a test notification for the focused pane: `herdr plugin action invoke dagadbm.notify-test`.
5. The config file is in `herdr plugin config-dir dagadbm`. The state files are in `$HERDR_PLUGIN_STATE_DIR` (see [architecture.md](architecture.md#runtime-state)).

Herdr refuses `herdr plugin install` over a linked plugin. Run `herdr plugin unlink dagadbm` first when you go back to the installed version (ADR 8).

## Ship it

The installed plugin comes from GitHub (ADR 8).

1. Push to `main`.
2. Run `~/dotfiles/herdr-update.sh`. It reinstalls the plugin and the Claude integration.
3. Check that the commit pinned in `herdr plugin list` matches `git rev-parse HEAD`.

## Recipes

Each recipe lists the files to touch, in order.

### Add a feature

1. **Create `src/<feature>/mod.rs`** with an entry point shaped like `run` in `src/notify/mod.rs`: it takes the remaining arguments and returns `Result<(), String>`. Parse arguments like `parse_cli_args` in `src/notify/cli.rs`, and support `-h`/`--help`.
2. **Add a prefix constant** for the sub-plugin name `herdr-dagadbm-<feature>`, like `PREFIX` in `src/notify/util.rs`. Following `notify`, put it in `src/<feature>/util.rs`. Build every file, folder, alerter group and temp directory name from it. When `HERDR_PLUGIN_STATE_DIR` is not set, use `<temp dir>/herdr-dagadbm-<feature>`, like `plugin_state_dir` in `src/notify/state.rs` (ADR 13).
3. **Register it in `src/main.rs`**: the `mod` line, the `match` arm in `run`, and a line in `USAGE`.
4. **Add its config section** `[<feature>]` to `config.example.toml`, with every key and its default, if the feature has settings. Load it in `src/<feature>/config.rs` from `$HERDR_PLUGIN_CONFIG_DIR/config.toml`, like `load` in `src/notify/config.rs`. Add a parity test there like `the_example_config_parses_to_the_defaults`, which reads the example through `include_str!` of `config.example.toml` (`EXAMPLE_CONFIG`). The loader must follow the same rules: every key optional, unknown keys ignored, a parse error reported on stderr with defaults applied (ADR 4). `notify`'s `ConfigFile` ignores other sections, so a new section does not disturb it. `--init-config` writes the whole `config.example.toml` (`EXAMPLE_CONFIG`), so it already includes the new section.
5. **Add its hooks to `herdr-plugin.toml`** under a `# --- <feature> ---` comment. Commands run `target/release/dagadbm <feature> ...`. Action ids start with `<feature>-` (ADR 1). An event that `notify` also handles gets its own `[[events]]` entry.
6. **Add `tests/<feature>_cli_test.rs`** that runs the binary against stubs ([test-guidelines.md](test-guidelines.md)).
7. **Run `docs/update.sh`.** It adds the feature's modules to the graph and reports any import of another feature's code.
8. **Move shared code to `src/shared/`** only when a second feature needs it. Shared code takes the prefix as an argument (ADR 1, ADR 13).
9. **Document it**: a section in `README.md` under Features, an ADR for any decision, and rows in the runtime state table of [architecture.md](architecture.md#runtime-state) for each file it creates.

The feature is done when `dagadbm <feature> --help` works and the [checks](#checks-before-you-commit) pass.

### Add a config key

For a key under `[notify]`:

1. **`src/notify/config.rs`**: add an `Option` field to `NotifySection` (or to `TemplateOverrides` for a per-status text, or `MarkerSection` for a marker). Add the resolved field to `NotifyConfig` and fill it in `NotifyConfig::from_section` with a default. Put the default in a `DEFAULT_*` constant if it is not trivial. Trim and lowercase status names, as the existing code does.
2. **The call site**, usually `run` in `src/notify/mod.rs`, reads the new field. An option that alerter receives goes into `AlerterOptions` and `alerter_focus_script` in `src/notify/script.rs`. If a re-post must reuse it, store it in `Record` in `src/notify/ledger.rs` too: `repost_closed_by_click` in `src/notify/outcome.rs` builds the re-post only from the record.
3. **`config.example.toml`**: add the key with its default and a one-line comment.
4. **The parity test** `the_example_config_parses_to_the_defaults` in `src/notify/config.rs`: compare the new field. Unknown keys are ignored, so a misspelled key would otherwise pass silently.
5. **A parsing test** in the `tests` module of `src/notify/config.rs`, and an integration test if the key changes what alerter receives.
6. **`README.md`** (Configure section) and ADR 7, or a new ADR.

### Change a default

This is the most frequent change in history (see [architecture.md](architecture.md#hotspots)). A default text lives in five places:

1. `DEFAULT_TITLE`, `DEFAULT_SUBTITLE`, `DEFAULT_MESSAGE` or `DEFAULT_STATUSES` in `src/notify/config.rs`.
2. `config.example.toml`.
3. The Configure section of `README.md`.
4. ADR 7.
5. Test assertions: `renders_the_default_layout_from_herdr_labels` and `missing_labels_render_empty_and_the_event_title_fills_in` in `src/notify/event.rs`, and the expected `--title=` and `--message=` lines in `tests/notify_cli_test.rs`.

A default state icon lives in `state_icon` in `src/notify/event.rs`, in `[notify.state_icons]` in `config.example.toml`, and in ADR 5 and ADR 7.

### Add a template token

Only add a token that Herdr itself defines (ADR 5).

1. **`TOKENS`** in `src/notify/template.rs`. `render` keeps any `{name}` that is not in this list as written.
2. **The value** in `token_values` in `src/notify/event.rs`. Take it from `StatusEvent` if the event carries it.
3. **A new lookup**, if the value needs one: add a field to `PaneLabels` in `src/notify/notification.rs`, and fill it in `pane_labels` in `src/notify/focus.rs` with `command_stdout_until` and the shared `deadline`. Parse the reply with `herdr_result`, so an unreadable reply becomes `None`.
4. **The test** `fills_every_token` in `src/notify/event.rs`.
5. **Docs**: the token comment in `config.example.toml`, the Tokens table in `README.md`, and the table in ADR 5.

### Add a terminal adapter

1. **Create `src/notify/terminal/<terminal>.rs`** with a unit struct that implements `TerminalAdapter` from `src/notify/terminal/mod.rs`:
   - `bundle_id` returns the app's bundle id, as stored in terminal bindings.
   - `focus_command` returns a `FocusCommand` that focuses the client's container, or `None`. Return `None` when a variable is missing or malformed: check ids character by character, as `Kitty` and `Wezterm` do. Return `None` for clients that only inherited the terminal's variables: check `TERM_PROGRAM`, as `Iterm2`, `Wezterm` and `Ghostty` do. A wrong command that succeeds stops the core from trying other clients (ADR 11).
   - Read the client's environment with `HerdrClient::env` and its directory from `cwd`.
2. **Register it in `src/notify/terminal/mod.rs`**: a `mod` line and an entry in `ADAPTERS`.
3. **Unit tests** in the new file: build clients with `HerdrClient::from_pairs` and `with_cwd`, and assert the exact `FocusCommand`. Add the bundle id to `finds_adapters_by_bundle_id` in `src/notify/terminal/mod.rs`. `adapter_bundle_ids_are_unique` covers the new entry by itself. Do not start the real terminal in a test.
4. **Docs**: the terminal list in ADR 11.
5. **Run `docs/update.sh`** for the new module.

### Add a hook or action

For `notify`:

1. **`herdr-plugin.toml`**: add the entry under `# --- notify ---`. An action id starts with `notify-` and its title with `Notify:`.
2. **`src/notify/cli.rs`**: add a `CliAction` variant and its flag in `parse_cli_args`. Add it to `print_usage` if users run it. Extend `parses_cli_actions`.
3. **`src/notify/mod.rs`**: handle the variant in the `match` at the top of `run`. Commands that are not the event path return there.
4. **A new event** runs the same `dagadbm notify` command. `run` tells events apart by `HERDR_PLUGIN_EVENT`: only `pane.focused` has its own branch, and every other event is parsed as a status event. A new event needs its own branch before `status_event_from_json`.
5. **`README.md`** (Actions section) for a user-facing action, and an integration test in `tests/notify_cli_test.rs`.

### Add a state file

1. Build the name with `prefixed` from `src/notify/util.rs`, or under a namespace like `namespace` in `src/notify/ledger.rs` (ADR 13). Put the pane in a name through `sanitize_group_id`.
2. Put the file in `plugin_state_dir()` from `src/notify/state.rs`.
3. If other processes read it while it changes, write a temp file and rename it into place, like `save_in` in `src/notify/ledger.rs`.
4. Give it a retention in `retention_for` in `src/notify/state.rs`. A file with no retention is never cleaned.
5. If it replaces an older name, teach `is_legacy_state_file` or `remove_legacy_state_in` to remove or rename the old one.
6. Add the file to the runtime state table in [architecture.md](architecture.md#runtime-state).

### Refresh the dependency graph

Run this after you add, remove or rename a module, or add or remove a `use crate::` import or another reference to a module. Editing code inside a module without changing which modules it uses does not change the graph.

```sh
docs/update.sh
```

It rewrites the block between `<!-- deps:start -->` and `<!-- deps:end -->` in `docs/architecture.md` and prints any feature-rule violation on stderr. It ignores test modules and comments. Running it twice gives no diff. A violation is only printed: the script still exits with 0 and writes the graph. Nothing in CI checks that the graph is current, so commit the result.

## Code conventions

These describe how the existing code is written. Follow them so new code reads the same.

### Errors

- Entry points return `Result<(), String>`. `main` in `src/main.rs` prints the error with a `dagadbm:` prefix and exits with a failure code.
- File and process helpers return `io::Result`. The caller turns the error into a sentence with `map_err` and `format!`, in the style "failed to <do something>: <error>" (see `run` in `src/notify/mod.rs`).
- A step that is not needed for the notification to go out is best-effort. Its result is dropped with `let _ =` or `.ok()`, and a comment says why. Examples: `ledger::save` and `prune_stale_workspace_bindings` in `run`, the cleanup at the start of `write_focus_script`. A failure there must never stop a notification.
- Missing information is `None`, not an error. A lookup that fails, times out or returns unreadable JSON gives `None`, and the token renders empty (`herdr_result` in `src/notify/focus.rs`).
- Removing a file that does not exist is not an error: match `io::ErrorKind::NotFound` and return `Ok` (for example `reset_notification_clearance` in `src/notify/state.rs`).
- No `unwrap` or `expect` outside tests.

### Config

- Read config only through `load` in `src/notify/config.rs`. It runs on every call, so nothing caches settings.
- Raw TOML structs (`NotifySection`, `MarkerSection`, `TemplateOverrides`) use `Option` fields and stay private. The resolved `NotifyConfig` and `MarkerConfig` hold the values the rest of the code uses.
- Status names are trimmed and lowercased when the config is resolved, and events are lowercased when parsed, so comparisons are plain string equality.

### Subprocesses

- In the notification path, run commands with `command_stdout_until` in `src/notify/util.rs` and the shared deadline. It returns `None` on failure, timeout or invalid UTF-8.
- Pass arguments as a list to `Command`, never as a shell string. The only shell code is the generated script, and every value in it goes through `shell_quote`.
- Pass alerter text as `--option=value` (`alerter_text_arg` in `src/notify/script.rs`), because a title can start with `-`.
- Find `herdr` with `resolve_herdr_bin` and alerter with `resolve_notifier_bin`. Other tools are found on `PATH`.
- Run git with `--no-optional-locks` and `LC_ALL=C` when you parse its output (`is_dirty` in `src/notify/git.rs`).
- Parse Herdr CLI replies through `herdr_result` in `src/notify/focus.rs`, which unwraps the `{"result": ...}` envelope.

### Files and names

- Every runtime name comes from `PREFIX` (see [Add a state file](#add-a-state-file)).
- Functions that touch the state directory come in pairs: a public wrapper that resolves `plugin_state_dir()`, and a variant whose name ends in "_in", "_into" or "_at" that takes the directory or path, for tests. Examples: `save` and `save_in` in `src/notify/ledger.rs`, `remember_terminal` and `remember_terminal_into`, `record_status` and `record_status_at` in `src/notify/state.rs`, `write_embedded_icon` and `write_embedded_icon_into` in `src/notify/icons.rs`.

### Rust style

- Items are `pub(crate)` when another module uses them, and private otherwise.
- The crate depends only on `serde`, `serde_json` and `toml` (`Cargo.toml`). Timeouts, process groups and the process environment are done with `std` and two libc calls (`kill` in `src/notify/util.rs`, `sysctl` in `src/notify/terminal/client.rs`). Each `unsafe` block has a `SAFETY:` comment.
- Doc comments explain why, and cite ADRs by number, for example "(ADR 12)".
- Comments and docs are plain English prose.
- Functions stay at a cognitive complexity of 10 or less. `clippy::cognitive_complexity` enforces it (`Cargo.toml` `[lints.clippy]`, threshold in `clippy.toml`), and the pre-commit and pre-push hooks fail on it. Split the function instead of raising the threshold.
- Functions stay at a CRAP score of 30 or less (`.cargo-crap.toml`). `scripts/pre-push.sh` runs the tests under `cargo llvm-cov`, saves `target/lcov.info`, and fails on `cargo crap --fail-above`. CRAP needs the coverage run, so it checks only on push; `scripts/pre-commit.sh` runs just `cargo fmt --check` and clippy. The git hooks in `.cargo-husky/hooks/` and any CI job call these two scripts. Install once: `cargo install cargo-llvm-cov cargo-crap --locked`. Cargo-husky copies `.cargo-husky/hooks/*` into `.git/hooks` only when it is rebuilt, so after editing a hook file run `cargo clean -p cargo-husky && cargo test --no-run`. See ADR 16.
