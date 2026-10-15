# dagadbm

A Herdr plugin that collects small features as they are needed. Each feature lives in its own folder under `src/`, and the binary runs it as `dagadbm <feature>`.

## Features

### notify

Clickable macOS notifications for agent panes that are blocked or done. Clicking a notification focuses the pane. The title, subtitle, message, buttons, sound, timeout and statuses are all configurable.

This feature is a port of [yankewei/herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify) (MIT), with configurable templates added.

## Requirements

- macOS
- Herdr 0.7.5 or later
- Rust, to build on install
- [alerter](https://github.com/vjeantet/alerter): `brew install vjeantet/tap/alerter`

## Install

```sh
herdr plugin install dagadbm/herdr-dagadbm
```

For local development, build the binary first, then link the plugin. Linking does not run the build.

```sh
cargo build --release
herdr plugin link .
```

## Configure

Herdr's own `config.toml` rejects keys it does not know, so settings live in the plugin's config directory:

```sh
herdr plugin action invoke dagadbm.notify-init-config   # writes a commented file with every default
$EDITOR "$(herdr plugin config-dir dagadbm)/config.toml"
```

No config file is needed; without one, every default applies. [`config.example.toml`](config.example.toml) lists every key with its default. Every key is optional. The file is re-read on every event, so edits apply to the next notification without a reload.

```toml
[notify]
notify_on = ["blocked", "done"]
title = "{state_icon} {agent}"
subtitle = "📂 {workspace} · 📑 {tab}"   # empty: no subtitle line
message = "{terminal_title_stripped}"
action_label = "Focus"
close_label = "Dismiss"
sound = ""      # an alerter sound name, e.g. "default"; empty plays no sound
timeout = 0     # seconds before an unclicked notification is dismissed; 0 keeps it

[notify.state_icons]      # what {state_icon} renders, per status
blocked = "🚨"
done = "✅"
working = "⏳"
idle = "💤"
unknown = "❓"

[notify.status.blocked]   # overrides for one status
title = "{state_icon} {agent} needs you · {workspace}"
```

### Tokens

Templates use Herdr's own syntax and token names:

| Token | Value |
| --- | --- |
| `{workspace}` | Workspace label |
| `{tab}` | Tab label |
| `{pane}` | Pane part of the pane id, e.g. `p3` |
| `{agent}` | Agent display name |
| `{terminal_title}` | Pane's terminal title |
| `{terminal_title_stripped}` | Terminal title without the status glyph |
| `{state_icon}` | `🚨` blocked, `✅` done, `⏳` working, `💤` idle, `❓` other; set in `[notify.state_icons]` |
| `{state_text}` | Herdr's status label, else e.g. `Blocked` |
| `{branch}` | Worktree branch |
| `{git_status}` | `*` when tracked files changed |
| `{hostname}` | Short host name |
| `$name` | Custom pane metadata value |
| `{{` `}}` | Literal braces |

A token without a value renders as an empty string. An unknown `{token}` is shown as written.

## Actions

- `dagadbm.notify-test`: send a test notification for the focused pane, using your templates.
- `dagadbm.notify-init-config`: write the default config if none exists.
- `dagadbm.notify-clear-terminal-bindings`: forget which terminal app shows each workspace.

## Decisions

Design decisions are recorded in [`docs/adr`](docs/adr).

## License

MIT. The notify feature is derived from herdr-focus-notify by yankewei (MIT). The agent icons have their own notice in `src/notify/assets/icons/NOTICE.md`.
