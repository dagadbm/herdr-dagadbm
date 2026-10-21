# 9. Mark the tab and pane label of an agent that needs you

Date: 2026-10-15

## Status

Accepted

## Context

A notification can be missed. Herdr has no API to style a pane or tab border (CLI and socket protocol 22 expose only text: tab labels and pane `--display-agent`), so a coloured accent border is not possible from a plugin.

## Decision

The `notify` feature also changes text that Herdr already draws. When an agent enters a status in `statuses` (default `blocked` and `done`), it puts `icon` (default `●` for the tab and the agent name, `🟡` for the workspace) and a space in front of the tab label (`herdr tab rename`), in front of the workspace label (`herdr workspace rename`) and in front of the pane's display agent (`herdr pane report-metadata <pane> --source dagadbm --display-agent`).

The marker is removed when the agent enters any other status, and when the user focuses the pane. A focused pane is never marked.

Herdr sends no `pane.focused` event when the user returns to a pane it already calls focused (for example from another terminal tab). The notification's visibility monitor catches that return, so when it finds the pane visible it removes the markers as a focus would (`visibility_verdict` in `src/notify/mod.rs`). It runs only while the notification is alive.

- `[notify.tab_marker]`, `[notify.workspace_marker]` and `[notify.agent_marker]` each have `enabled`, `icon` and `statuses`, so they can differ.
- The workspace is the one in the pane ID, read with `workspace get`. A notification names it without the marker, like the tab.
- A tab or workspace can hold several agents, so its marker stays while any agent in it needs you. Each pane holds a token (`marked_tab`, `marked_workspace`, reported with `--source dagadbm --token`) while its own agent does; Herdr keeps it and `pane list` returns it. The tab or workspace label is marked while any of its panes holds the token. The token comes off when that agent moves to another status or that pane is focused. Focusing another pane in the same tab or workspace changes nothing.
- The default icon for the workspace (`🟡`) is an emoji, not the text glyph `⚠`: an emoji keeps its own colour, while a text glyph takes the label's colour and got lost among Herdr's text. Plugins cannot colour label text. The tab and the agent name are the exception: a colour emoji looked out of place beside Herdr's plain text, and `❕` is drawn two cells wide, leaving a gap, so their default is the single-width plain glyph `●`.
- The plugin stores no files for this. A label that starts with `icon ` is marked, and the pane tokens say which agents still need you. A tab the user renamed meanwhile is left alone.
- If a marked pane is closed, its tab or workspace keeps the marker until the next status or focus event in it.
- Removing the pane marker uses `--clear-display-agent`, so Herdr's own agent label returns.
- Herdr's CLI needs the pane ID before the options.
- A manual pane name hides the agent label on the border, so the pane marker has no effect there; the tab marker still works.
- An unnamed tab is marked with its number and keeps that number as its label after the marker is removed.
- `enabled = false` or an empty `icon` turns the feature off.
- It shares the single `agent get` the notification makes, and runs for every status change, not only `notify_on` ones.

## Consequences

If Herdr adds a border or highlight API, replace the text marker with it.
