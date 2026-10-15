# 2. Rust, as a clean port of herdr-focus-notify

Date: 2026-10-15

## Status

Accepted

## Context

The notify feature replaces [yankewei/herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify), which is MIT-licensed, written in Rust, and has no configuration. The options were to fork that plugin, to rewrite it in another language, or to port its logic into this plugin's structure.

## Decision

Write the plugin in Rust. Port herdr-focus-notify's tested logic into `src/notify/`: the focus detection, the terminal bindings, the click-to-focus over the Herdr socket, the kitty and iTerm2 window targeting, the bounded Herdr and git lookups, and the bundled agent icons.

Replace its hardcoded notification layout with configurable templates (ADR 5, ADR 6). Drop what the templates no longer need, such as the diffstat and the branch truncation. The original's MIT license is credited in the README and in `src/notify/mod.rs`.

## Consequences

- The plugin is fast, and it builds with `cargo build --release` on install, as the original does.
- The port keeps the original's behaviour and its test suite, adapted to the new layout.
- Fixes upstream do not flow in automatically. They have to be ported by hand.
