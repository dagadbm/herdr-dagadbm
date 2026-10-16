# 9. Mark the tab and pane label of an agent that needs you

Date: 2026-10-15

## Status

Accepted

## Context

A notification can be missed. Herdr has no API to style a pane or tab border (CLI and socket protocol 22 expose only text: tab labels and pane `--display-agent`), so a coloured accent border is not possible from a plugin.

## Decision

The `notify` feature also changes text that Herdr already draws. When an agent enters a status in `statuses` (default `blocked` and `done`), it puts `icon` (default `⚠`) and a space in front of the tab label (`herdr tab rename`) and in front of the pane's display agent (`herdr pane report-metadata <pane> --source dagadbm --display-agent`).

The marker is removed when the agent enters any other status, and when the user focuses the pane. A focused pane is never marked.

- `[notify.tab_marker]` and `[notify.agent_marker]` each have `enabled`, `icon` and `statuses`, so the tab and the agent name can differ.
- The marker is the only state. A label that starts with `icon ` is marked; nothing is stored. A tab the user renamed meanwhile is left alone.
- Removing the pane marker uses `--clear-display-agent`, so Herdr's own agent label returns.
- Herdr's CLI needs the pane ID before the options.
- A manual pane name hides the agent label on the border, so the pane marker has no effect there; the tab marker still works.
- An unnamed tab is marked with its number and keeps that number as its label after the marker is removed.
- `enabled = false` or an empty `icon` turns the feature off.
- It shares the single `agent get` the notification makes, and runs for every status change, not only `notify_on` ones.

## Consequences

If Herdr adds a border or highlight API, replace the text marker with it.
