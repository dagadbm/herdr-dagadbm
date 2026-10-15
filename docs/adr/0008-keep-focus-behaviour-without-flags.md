# 8. Keep the focus behaviour, without config flags

Date: 2026-10-15

## Status

Accepted

## Context

herdr-focus-notify does more than send notifications:

- It skips a notification when the pane is focused and the terminal bound to its workspace is the frontmost app.
- It removes a notification automatically when the user returns to the pane.
- A click focuses the pane through the Herdr socket.
- On a click, it raises the right kitty or iTerm2 window.
- It cleans up its state on startup.
- It has an action that clears the learned terminal bindings.

## Decision

Keep all of it, unchanged, and add no config flags for it. The defaults are what is wanted.

## Consequences

- There is less config to document and test.
- A flag can be added later, under `[notify]`, if a real need appears.
