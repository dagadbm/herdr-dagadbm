# 7. Configurable fields, statuses and per-status overrides

Date: 2026-10-15

## Status

Accepted

## Context

herdr-focus-notify hardcodes which statuses notify (`blocked` and `done`) and the text of every field. The goal is to configure the title and subtitle at minimum, with sane defaults.

## Decision

The `[notify]` section has these keys:

- `notify_on`: the agent statuses that send a notification. Default `["blocked", "done"]`.
- `title`: default `"⌂ {workspace} ▸ {tab}"`.
- `subtitle`: default `""`. An empty subtitle is left out.
- `message`: default `"{state_icon} {terminal_title_stripped}"`.
- `action_label`: default `"Focus"`.
- `close_label`: default `"Dismiss"`.
- `sound`: an `alerter` sound name. Default `""`, which plays no sound.
- `timeout`: seconds before an unclicked notification is dismissed. Default `0`, which keeps it until dismissed.

`[notify.status.<status>]` can override any of the five text fields for one status. Each field is looked up in that order: the status table, then `[notify]`, then the default.

`[notify.state_icons]` maps a status to the text `{state_icon}` renders, e.g. `done = "✓"`. It accepts every status, including ones not in `notify_on`. Statuses it does not list keep their defaults: `×` blocked, `✓` done, `◐` working, `○` idle, `·` any other. The defaults are the glyphs of Herdr's `status_indicators = "symbols"` (Herdr source, `src/client/shell.rs`); Herdr does not expose them to plugins, so they are copied. Any template can use the token, including the subtitle. There is no matching table for `{state_text}`.

The notification icon is not configurable. It comes from the bundled agent icons.

## Consequences

- By default the title is the workspace and tab, the subtitle is empty, and the message is the status icon and the terminal title. macOS shows the title in bold, so it stays short.
- `action_label` is also the text `alerter` reports on a click. The generated script matches on the configured label, so a custom label still focuses the pane.
