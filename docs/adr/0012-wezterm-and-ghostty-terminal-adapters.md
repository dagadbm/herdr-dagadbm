# 12. WezTerm and Ghostty terminal adapters

Date: 2026-10-15

## Status

Accepted

## Context

A notification click activates the bound terminal app. When Herdr runs in another tab of that terminal, the app comes forward but shows the wrong tab. Adapters for kitty and iTerm2 select the tab that runs Herdr first. WezTerm and Ghostty had no adapter.

## Decision

- **WezTerm:** the adapter runs `wezterm cli activate-pane --pane-id $WEZTERM_PANE` for the Herdr client. The `wezterm` binary comes from `WEZTERM_EXECUTABLE_DIR`.
- **Ghostty:** Ghostty gives its processes no terminal id. The adapter reads the client's working directory and runs AppleScript that focuses the first terminal with that working directory. If no terminal matches, the script fails and the core tries the next client.

## Consequences

- WezTerm needs no configuration.
- Ghostty (1.3 and later) is matched by directory. When two Ghostty terminals share a directory, the first match wins and the wrong tab can come forward. This adapter was not tested against a real Ghostty.
- Any failure keeps plain app activation.
