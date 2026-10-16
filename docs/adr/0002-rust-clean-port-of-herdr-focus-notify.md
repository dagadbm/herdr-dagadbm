# 2. Rust, as a clean port of herdr-focus-notify

Date: 2026-10-15

## Status

Accepted. ADR 8 was merged into this ADR on 2026-10-16.

## Context

The notify feature replaces [yankewei/herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify), which is MIT-licensed, written in Rust, and has no configuration. The options were to fork that plugin, to rewrite it in another language, or to port its logic into this plugin's structure.

herdr-focus-notify does more than send notifications:

- It skips a notification when the pane is focused and the terminal bound to its workspace is the frontmost app.
- It removes a notification automatically when the user returns to the pane.
- A click focuses the pane through the Herdr socket.
- On a click, it raises the right kitty or iTerm2 window.
- It cleans up its state.
- It has an action that clears the learned terminal bindings.

## Decision

Write the plugin in Rust. Port herdr-focus-notify's tested logic into `src/notify/`: the focus detection, the terminal bindings, the click-to-focus over the Herdr socket, the kitty and iTerm2 window targeting, the bounded Herdr and git lookups, and the bundled agent icons.

Keep all of the focus behaviour listed above, and add no config flags for it. The defaults are what is wanted. A flag can be added later, under `[notify]`, if a real need appears. Later ADRs extend this behaviour: ADR 12 adds terminal adapters and ADR 13 adds the re-post of closed notifications.

Replace its hardcoded notification layout with configurable templates (ADR 5, ADR 6). Drop what the templates no longer need, such as the diffstat and the branch truncation. The original's MIT license is credited in the README and in `src/notify/mod.rs`.

## Consequences

- The plugin is fast, and it builds with `cargo build --release` on install, as the original does.
- The port keeps the original's behaviour and its test suite, adapted to the new layout.
- There is less config to document and test.
- Fixes upstream do not flow in automatically. They have to be ported by hand.
