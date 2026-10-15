# 5. Herdr's template syntax and token names

Date: 2026-10-15

## Status

Accepted

## Context

The notification text has to be configurable with variables such as the agent, tab and workspace. Herdr's own `config.toml` already has a template syntax:

- `window_title` uses `{hostname}`, `{workspace}`, `{tab}`, `{pane}` and `{terminal_title}`, with `{{` and `}}` for literal braces.
- The sidebar rows use `state_icon`, `state_text`, `agent`, `terminal_title_stripped`, `branch` and `git_status`.
- Custom pane metadata uses `$name`.

## Decision

Use Herdr's syntax and only Herdr's token names. Add no tokens that Herdr does not define itself.

| Token | Value |
| --- | --- |
| `{workspace}` | Workspace label |
| `{tab}` | Tab label, or its number for an unnamed tab |
| `{pane}` | Pane part of the pane id (`w1:p3` → `p3`). Herdr gives panes no names |
| `{agent}` | Display name of the agent, falling back to its id |
| `{terminal_title}` | Pane's terminal title, as reported |
| `{terminal_title_stripped}` | Terminal title without the agent's status glyph |
| `{state_icon}` | Per status, the glyphs of Herdr's symbol indicators: `×` blocked, `✓` done, `◐` working, `○` idle, `·` other. Configurable (ADR 7) |
| `{state_text}` | Herdr's label for the status, else the capitalized status (`Blocked`) |
| `{branch}` | Branch of the worktree that owns the pane's directory |
| `{git_status}` | `*` when tracked files changed versus `HEAD`, else empty |
| `{hostname}` | `hostname -s` |
| `$name` | Value the pane reported through `herdr pane report-metadata` |
| `{{` `}}` | Literal braces |

An unknown `{token}` is kept as written, so a typo stays visible in the notification.

## Consequences

- A user who knows Herdr's config already knows the syntax.
- The diffstat that herdr-focus-notify showed (`+120/-45`) is gone, because Herdr has no token for it.
