# 11. Skip a repeated agent status

Date: 2026-10-15

## Status

Accepted

## Context

Herdr sends `pane.agent_status_changed` again when the pane's metadata changes, even if the status is the same. The agent marker (ADR 10) changes the display agent with `pane report-metadata`, so every marked status came back as a second event, and the plugin sent a second notification.

Herdr starts a new plugin process for each event, so the plugin cannot keep the last status in memory.

## Decision

The `notify` feature saves the last agent status of each pane in `$HERDR_PLUGIN_STATE_DIR/herdr-<pane>.last-status`. An event with the same status as the saved one is skipped before any Herdr call: no markers and no notification.

- The status is saved before the markers write to Herdr, so the echo of that write always finds it.
- Every real change still counts, so `blocked`, `working`, `blocked` notifies twice.
- Focusing a pane does not reset the saved status.
- When the file cannot be read or written, the event counts as a change.
- Startup cleanup removes `.last-status` files older than 24 hours.

## Consequences

The same status is never notified twice in a row for one pane, whatever makes Herdr repeat the event. A long-running daemon could keep this in memory, but one small file per pane is simpler.
