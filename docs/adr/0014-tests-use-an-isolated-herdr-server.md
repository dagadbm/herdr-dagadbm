# 14. Tests use an isolated Herdr server

Date: 2026-10-16

## Status

Accepted

## Context

Three `pane.focused` integration tests ran with no `herdr` stub. Inside a Herdr pane they reached the user's live server, where `clear_for_focus` can rename a real tab and clear the agent marker on a real pane. Stubs avoid that, but a stub only answers what the code expects today, so it cannot show that the real Herdr CLI still accepts the plugin's calls.

Herdr can run a named session (`HERDR_SESSION`) with its own socket and state. A named session still shares `~/.config/herdr`, so it loads the user's installed plugins. Running it under a throwaway `HOME` isolates it fully.

## Decision

Tests that need Herdr run the real `herdr` against one isolated server per test binary: `HerdrServer` in `tests/support/herdr_server.rs`.

- The first test that needs it starts the session `herdr-dagadbm-test` under a short throwaway `HOME` in `/tmp`. The path is short because macOS limits a socket path to about 104 bytes.
- A watchdog process stops the server and deletes that `HOME` once the test binary exits.
- Each test creates its own workspace on the shared server.
- The tests run in every `cargo test`. A missing `herdr` fails them.

Every integration test that reaches Herdr uses this server. A call log comes from a wrapper that logs and then runs the real `herdr`; a hang comes from a wrapper that hangs on one command. Only tests of a missing `herdr` binary or a failing click socket keep a fake. Herdr has one focused pane per server, so tests that need or move the focus hold a lock, and the rest stay unfocused. `alerter`, `git` and `open` are always stubbed. `lsappinfo` is read-only, so one test that calls the real one is accepted.

## Consequences

- No test touches the user's Herdr session.
- A change in the Herdr CLI can fail a test instead of breaking the plugin silently. The first run found one: Herdr 0.9.5 returns custom pane values as `tokens`, not `metadata`, so `$name` tokens rendered empty.
- `cargo test` needs Herdr installed, and the first Herdr test pays about one second of server start.
- The Rust test harness has no hook that runs after all tests, so the teardown relies on the watchdog.
