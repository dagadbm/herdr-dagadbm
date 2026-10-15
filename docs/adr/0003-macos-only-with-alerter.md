# 3. macOS only, notifying through alerter

Date: 2026-10-15

## Status

Accepted

## Context

Three notification tools are available on macOS:

- `osascript` supports a title and subtitle, but has no click action and no custom icon.
- `terminal-notifier` supports click actions.
- `alerter` supports a subtitle, action and close buttons, sound, timeout, groups and app icons, and reports which button was clicked.

The subtitle requirement is a macOS concept in any case.

## Decision

Support macOS only (`platforms = ["macos"]`) and send every notification through `alerter` (`brew install vjeantet/tap/alerter`). The plugin looks for `alerter` on `PATH` first, then in `/opt/homebrew/bin`, `/usr/local/bin` and `~/.local/bin`.

## Consequences

- A click can focus the exact pane, and the plugin can remove a notification it sent earlier.
- `alerter` refuses an empty `--message`, so a message template that renders to nothing is sent as a single space.
- Linux support would need a second notifier behind the same script-writing seam.
