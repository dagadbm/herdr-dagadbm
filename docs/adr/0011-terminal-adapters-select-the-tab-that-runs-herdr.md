# 11. Terminal adapters select the tab that runs Herdr

Date: 2026-10-15

## Status

Accepted

## Context

A notification click activates the bound terminal app with `open -b`. When Herdr runs in another tab or window of that terminal, the app comes forward but shows the wrong tab.

Every Herdr client attached to a session shows the same focused pane. So any client's terminal container (tab, window or session) shows the target pane once Herdr focuses it. The plugin only has to bring one of those containers forward.

## Decision

`src/notify/terminal/` selects a client's container before the app activation. The shared core finds the session's Herdr clients with `lsof`, reads each client's environment and working directory, and tries the clients from the most recently used one. A `TerminalAdapter` (one file per terminal) maps a client to one focus command. The core runs the command with a 2-second timeout and stops at the first success.

Supported terminals:

- **iTerm2** (`com.googlecode.iterm2`): `open iterm2:reveal?sessionid=$ITERM_SESSION_ID`. iTerm2 handles the URL itself, so no scripting setup is needed. The adapter skips clients that inherited `ITERM_SESSION_ID` from another terminal.
- **kitty** (`net.kovidgoyal.kitty`): `kitten @ focus-window --match id:$KITTY_WINDOW_ID`. This needs `allow_remote_control` and `listen_on` in `kitty.conf`.
- **WezTerm** (`com.github.wez.wezterm`): `wezterm cli activate-pane --pane-id $WEZTERM_PANE`. The `wezterm` binary comes from `WEZTERM_EXECUTABLE_DIR`. No configuration is needed. The adapter skips clients whose `TERM_PROGRAM` is set to something other than `WezTerm`, because they inherited `WEZTERM_PANE` from another terminal. A client with no `TERM_PROGRAM` is tried.
- **Ghostty** (`com.mitchellh.ghostty`): Ghostty gives its processes no terminal id. The adapter runs AppleScript (Ghostty 1.3 and later) that focuses the first terminal whose working directory equals the client's. If no terminal matches, the script fails and the core tries the next client. The adapter skips clients whose `TERM_PROGRAM` is set to something other than `ghostty`. A client with no `TERM_PROGRAM` is tried.

Each adapter returns no command when a variable it needs is missing or malformed, so the core tries the next client. A terminal without an adapter, or any failure, keeps plain app activation. To add a terminal, add one adapter file, its `mod` line in `src/notify/terminal/mod.rs`, and an entry in `ADAPTERS`.

## Consequences

- Clicking a notification shows the tab that runs Herdr in iTerm2, kitty, WezTerm and Ghostty.
- Ghostty matches by directory. When two Ghostty terminals share a directory, the first match wins and the wrong tab can come forward.
- The WezTerm and Ghostty adapters were not tested against the real applications.
- kitty needs remote control enabled in its configuration.
