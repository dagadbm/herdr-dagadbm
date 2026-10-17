# Test guidelines

How the tests are organised, how the stubs work, and how to add a test. The architecture they test is in [architecture.md](architecture.md). Change recipes are in [dev-guidelines.md](dev-guidelines.md).

## Running tests

```sh
cargo test                                          # everything
scripts/pre-push.sh                                  # fmt, clippy, everything under coverage (target/lcov.info), then the CRAP gate
cargo test --bin dagadbm <name>                     # unit tests whose name contains <name>
cargo test --test notify_cli_test <name>            # integration tests whose name contains <name>
```

The integration tests need the real `herdr` installed. They start their own isolated Herdr server (see [The test Herdr server](#the-test-herdr-server)), so a plain `cargo test` is safe inside a Herdr pane: no test reaches your live session.

## Unit tests and integration tests

| | Unit tests | Integration tests |
| --- | --- | --- |
| Where | A `#[cfg(test)] mod tests` at the end of each file in `src/` | `tests/<feature>_cli_test.rs`; today only `tests/notify_cli_test.rs` |
| What runs | One function, in process | The built binary, `dagadbm notify ...`, as Herdr would run it (`binary()` in `tests/notify_cli_test.rs`, through `CARGO_BIN_EXE_dagadbm`) |
| External tools | None, or the test Herdr server through a wrapper passed by path | The test Herdr server for `herdr`; stubs on `PATH` for the rest |
| Use for | Parsers, renderers, decisions, file formats | Hook flows, call order, what alerter and Herdr receive, timing |

Put logic in a pure function so a unit test can reach it. The code already splits most work this way:

- Parsers take the raw text: `pane_details_from_get_json`, `tab_label_from_get_json`, `branch_from_worktree_list_json`, `bundle_id_from_lsappinfo` in `src/notify/focus.rs`; `groups_from_list_output` in `src/notify/notifier.rs`; `client_pids_from_lsof` in `src/notify/terminal/client.rs`.
- Decisions take their inputs: `notification_decision_from_bundles` in `src/notify/focus.rs`.
- The script builder returns a string: `alerter_focus_script` in `src/notify/script.rs`. Its tests assert on the script text.
- State functions have a variant that takes a directory: `save_in`, `resolve_in`, `claim_repost_in` in `src/notify/ledger.rs`; `record_status_at`, `remember_terminal_into`, `cleanup_stale_state_files_in` in `src/notify/state.rs`. Unit tests call these with a temp directory, never `plugin_state_dir()`. The `TempDir` helper in the tests of `src/notify/ledger.rs` removes its directory when dropped.

A unit test that needs Herdr uses the same test Herdr server. `src/main.rs` compiles `tests/support/herdr_server.rs` into the unit tests as `test_herdr`, because code in `src/` cannot import from `tests/`. `TestHerdr` in the tests of `src/notify/tab_marker.rs` is the model: it passes a `recording_herdr` wrapper's path straight to the function and maps the real IDs back to `w1:t1` and `w1:p2` in the recorded calls.

## How stubs on PATH work

The binary finds most tools on `PATH` first: alerter through `find_executable` in `src/notify/executable.rs`; `git`, `lsappinfo`, `open`, `lsof`, `ps` and `hostname` by plain name. `herdr` comes from `HERDR_BIN_PATH` when it is set (`resolve_herdr_bin`). An integration test therefore:

1. Creates a fresh directory with `temp_test_dir()`.
2. Writes each stub into it with `write_executable(path, script)`. A stub is a `#!/bin/sh` script with mode `0700`.
3. Runs the binary with `PATH` set to `path_with_temp_dir(&temp_dir)`, which puts the directory in front of the inherited `PATH`.
4. Connects every run that can reach Herdr to the test Herdr server with `HerdrServer::connect` or `HerdrServer::connect_logged`, including `pane.focused` runs. Otherwise the inherited `HERDR_BIN_PATH` and `HERDR_SOCKET_PATH` win, and they point at your live session.
5. Sets `HERDR_PLUGIN_STATE_DIR` (and `HERDR_PLUGIN_CONFIG_DIR` when it uses a config) to paths inside the temp directory.
6. Removes the temp directory at the end, after detached scripts have finished (see [Waiting for detached work](#waiting-for-detached-work)).

Stubs find their log and state files through environment variables that the test sets on the command: `NOTIFIER_LOG`, `HERDR_LOG`, `GIT_LOG`, `OPEN_LOG`, `FRONTMOST_STATE`, `STUB_DIR`. The binary passes its environment on to the scripts it starts, so a stub called from a detached script still sees them.

## Writing a new stub

A stub must:

- **Answer exactly the calls the code makes.** Match on the first arguments with `case "$1 $2" in`, as the `alerter` stub in `Session::new` does. The calls per tool are listed in [architecture.md](architecture.md#external-process-boundaries).
- **Print what the parser expects.** Herdr CLI replies are JSON wrapped in `{"result": ...}`, because `herdr_result` in `src/notify/focus.rs` unwraps that envelope. `lsappinfo` must answer both `front` and `info`; `write_fixed_frontmost` and `write_state_frontmost` in `tests/notify_cli_test.rs` show both forms.
- **Exit 0 for success.** A non-zero exit, or no output, is how a stub says "no answer". The binary then treats the value as missing.
- **Never call the real tool.**
- **Log its arguments** to a file named by an environment variable, one argument per line or `|`-separated, so the test can assert on them. If the test polls the log while the stub may still be writing, write to a temp file and move it into place with mv, as `write_logging_notifier` does.
- **End.** A stub that stands in for a posting alerter must exit, or wait for a file the test writes, so the detached script can finish. The `alerter` stub in `Session::new` waits up to 15 s for `answer-<group>`.

To make a stub answer differently during one test, read the answer from a file the test rewrites, like `FRONTMOST_STATE` in `visible_focused_pane_removes_its_pending_notification`.

For the Herdr socket, use `focus_socket(expected_pane_id, response)`. It binds a Unix socket at a short `/tmp` path from `unique_socket_path` (the path must fit `sun_path`), asserts the `pane.focus` request, and writes the response. Set `HERDR_SOCKET_PATH` to its path. In every other test that can click, remove the variable with `env_remove("HERDR_SOCKET_PATH")`, as `Session::command` does.

## Feeding and capturing events

- **Feed an event** by setting `HERDR_PLUGIN_EVENT` (for example `pane.agent_status_changed`) and `HERDR_PLUGIN_EVENT_JSON` on the command. The JSON needs a `data` object with `pane_id`; a status event also uses `agent_status`, `agent`, `display_agent`, `title` and `state_labels` (`EventData` in `src/notify/event.rs`). Without `HERDR_PLUGIN_EVENT_JSON` the binary exits quietly (`no_event_is_quiet_without_any_output`).
- **Capture what the binary sends** through the stub logs. Assert on alerter's arguments for the notification text, on the `herdr` log for markers and lookups, and on state files for the ledger.
- **Capture a real event from Herdr** when you need a new fixture: in a local debug build, print `HERDR_PLUGIN_EVENT_JSON` to stderr from `run` in `src/notify/mod.rs`, trigger the event, and read it with `herdr plugin log list --plugin dagadbm`. Remove the print before you commit. Copy only the fields the code reads into the test.

## Short waits

`binary()` starts the plugin with the hidden flag `--integration-tests`. It swaps every wait for the short one in `Timings::integration_tests` (`src/notify/timings.rs`): the click window, the close settle, the lookup budget, the focus socket timeout and the visibility monitor poll. The generated scripts pass the flag on to the commands they run. A test that asserts a time compares it with these values, not the production ones.

## Waiting for detached work

The binary starts the notification script with `nohup` and exits at once, so the work continues after `output()` returns. Wait for it with a bounded poll, never with a single sleep:

- `wait_for_notifier_output(log)`: the alerter stub has written its log.
- `wait_for_detached_notifier(state_dir)`: every script has exited. The script's `.result.` and `.status.` temp files are gone. Call this before you delete the temp directory, or the delete races with the script.
- `Session::wait_for_posts`, `Session::wait_for_status` and `Session::wait_for_script_end` for ledger flows.

Tests that check timing (`CLICK_WINDOW`, `CLOSE_SETTLE`, `LOOKUP_BUDGET`) assert a bound, such as "under 10 s" or "at least 1 s", not an exact time.

## The `Session` helper

`Session` in `tests/notify_cli_test.rs` sets up click, close and re-post flows (ADR 12). It is a workspace `repo` on the test Herdr server, with one tab labelled `status`. Tests name panes `w1:p2`, `w1:p3` and so on. `Session::pane` turns a name into a real pane, created on first use, and every method takes the name. Every pane is an unfocused agent. It provides:

- The real `herdr` through a logging wrapper, and stubs for `alerter`, `lsappinfo` and `open` (on `PATH`). `STUB_DIR` points at the session directory. `HERDR_SOCKET_PATH` is removed (`Session::command`), because a real click would move the shared server's focus; the wrapper still reaches the test server.
- `notify(pane)`: sends a `blocked` event and waits for the posting.
- `answer(pane, reply)`: makes that pane's alerter print `Focus`, `@CLOSED`, `@TIMEOUT`, `@CONTENTCLICKED`, or fail with `@EXIT<n>`.
- The file `shown` is what `alerter --list ALL` prints. The file `list-fails` makes the list fail.
- `posts`, `posts_for`, `record`, `status`, `post_id`, `click_marker`, `cleared_marker`: read what happened.
- `report(outcome, pane, post_id)`: calls `--notification-<outcome>` directly.
- `forget_last_status(pane)`: lets the same status notify again (ADR 10).
- `finish()`: waits for every script, then removes the directory. Answer every notification first.

## The config example parity test

`the_example_config_parses_to_the_defaults` in `src/notify/config.rs` parses `config.example.toml` (embedded as `EXAMPLE_CONFIG`) and compares it with `NotifyConfig::default()`. It compares the enabled statuses, the templates for `blocked`, `done` and `working`, `sound`, `timeout_secs`, and both markers.

It does not compare `[notify.state_icons]`, because those defaults live in `state_icon` in `src/notify/event.rs`. It cannot notice a key that the example sets but the code does not read, because unknown keys are ignored. When you add a config key, add a comparison for it here ([dev-guidelines.md](dev-guidelines.md#add-a-config-key)).

## The rule: tests never call real tools

Tests run against stubs, or against the isolated test Herdr server. Stub `git`, `alerter` and `lsappinfo`, and `open` when a click can happen. A test must never call your real tools or your live Herdr session. The real ones change your Herdr session, post real notifications, or read your real frontmost app. For each new integration test, check:

- Every run that can reach Herdr either sets `HERDR_BIN_PATH` to a stub or uses `HerdrServer::connect` (see [The test Herdr server](#the-test-herdr-server)). Never the live Herdr.
- A run that sends `pane.focused`, or a status event for a focused pane, has an `lsappinfo` stub.
- A run that can post has an `alerter` stub. A run that can click has an `open` stub and either `focus_socket` or no `HERDR_SOCKET_PATH`.
- A run whose pane has a `cwd` in the `agent get` reply has a `git` stub.
- `HERDR_PLUGIN_STATE_DIR` points into the temp directory.

### Known gaps

These existing tests break the rule. They are listed in [architecture.md](architecture.md#known-discrepancies) too.

- `blocked_agent_marks_tab_and_agent_name_and_working_or_focus_restores_them` sends `pane.focused` without an `lsappinfo` stub. This is accepted: `lsappinfo` only reads the frontmost app.
- Unit tests in `src/notify/icons.rs`, `renders_the_default_layout_from_herdr_labels` in `src/notify/event.rs`, and the script tests in `src/notify/script.rs` use the real `plugin_state_dir()`.

## The test Herdr server

A test that needs Herdr itself, not a stub, uses the real `herdr` against an isolated server (ADR 14). The server is `HerdrServer` in `tests/support/herdr_server.rs`. A test file loads it with `mod support;`.

- **Setup, once per test binary.** `HerdrServer::shared` starts the server on first use: the named session `herdr-dagadbm-test` under a throwaway `HOME` in `/tmp`. It has its own socket and state and loads none of your plugins. It never touches your own Herdr session.
- **Teardown, once.** A watchdog process waits for the test binary to exit, then stops the server and deletes the throwaway `HOME`. This also runs after a panic or Ctrl-C.
- **One workspace per test.** Tests run in parallel against the one server. Call `HerdrServer::workspace` and use only the IDs it returns. Never hard-code `w1:p2`.
- **Point the plugin at it** with `HerdrServer::connect` on the `Command`, which sets `HERDR_BIN_PATH` and `HERDR_SOCKET_PATH` the way Herdr does for a hook.
- **Arrange and check state** with `HerdrServer::herdr`, which runs the real CLI and returns its JSON, and `HerdrServer::herdr_quiet` for commands that print nothing. For example, `pane report-agent` fakes an agent and `tab get` reads a label back.
- **Focus is shared.** Herdr has one focused pane per server. A workspace made at startup, the lobby, keeps it, so test panes count as unfocused. A test that needs a focused pane, or that moves the focus, holds `HerdrServer::focus`. The guard gives the focus back to the lobby when it is dropped, and only one test holds it at a time.
- **Set up state with the real CLI.** `report_agent` makes a pane an agent, so `agent get` answers (states: `idle`, `working`, `blocked`, `unknown`). `rename_tab`, `split`, `set_terminal_title` and `workspace_in` (a label and a directory) cover the rest. Read state back with `tab_label` and `focused_pane`.
- **Log or break the calls.** `logging_herdr` writes a wrapper that logs each call to `$HERDR_LOG` and then runs the real `herdr`. `hanging_herdr` hangs on one command, such as `worktree list`, and passes the rest through. Both set the test server's socket themselves. Use them with `connect_logged`.
- **Git repositories for setup** come from `real_git` in `tests/notify_cli_test.rs`, which ignores your git config. The plugin's own `git` calls still go to a stub.
- **A missing `herdr` fails the test.** It is never skipped.

These tests keep a fake instead, because the real server cannot produce what they test: a missing or broken `herdr` binary (`test_mode_reports_bad_configured_herdr_binary`, `focus_click_without_terminal_binding_does_not_activate_or_focus`), and a click socket that fails or must not move the shared focus (`focus_click_reports_*`, `a_stale_click_focuses_and_reposts_others_but_not_its_own_pane`, the click in `notification_focus_does_not_overwrite_existing_terminal_binding`, through `focus_socket`).

`focus_event_removes_notification_for_foreground_terminal` is the model: it fakes a blocked agent, marks the tab, sends `pane.focused`, and checks that the real tab label lost its marker.

## Model test

For a new event test, copy the shape of `normal_notification_names_the_pane_and_never_requests_an_explanation` in `tests/notify_cli_test.rs`. It builds a real git repository with a linked worktree, sets up its workspace on the test Herdr server, logs the `herdr` calls with `logging_herdr`, stubs `git` and `alerter`, writes a config, sends one `pane.agent_status_changed` event, asserts on alerter's arguments and on the `herdr` and `git` logs, and waits for the detached script before it cleans up.

For a click, close or re-post test, copy `a_click_reposts_the_notification_it_closed_silently_and_unchanged`, which uses `Session`.
