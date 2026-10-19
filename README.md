# herdr-dagadbm

My personal [Herdr](https://herdr.dev) plugin. It is a mix of small plugins, one per feature, that I add as I need them. It is built for my own setup and workflow (macOS, my terminals, my agents). You are welcome to use it or borrow from it, but it is not meant as a general-purpose product.

One binary runs every feature by name: `dagadbm <feature>`. Each feature lives in its own folder under `src/` and works on its own.

> **Disclaimer:** this project is written by [Claude](https://claude.ai/code) (Anthropic's AI coding agent) and steered by me: I decide what gets built and how it should behave, review the result, and use it daily. Expect the code to reflect that.

## Origin

The `notify` feature started as a port of [yankewei/herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify) (MIT) by yankewei. Its focus detection, click-to-focus, terminal bindings and agent icons come from there. This project rewrote it in this plugin's structure and added configurable templates and the features below.

## Features

### notify

macOS notifications for agent panes that need you.

- **Clickable notifications** when an agent is blocked or done. Clicking the notification, or its `Focus` button, focuses the pane in Herdr and brings the terminal that runs Herdr to the front.
- **The right terminal tab**: in iTerm2, kitty, WezTerm and Ghostty, the click selects the tab or window that runs Herdr, not just the app.
- **No noise when you are already there**: no notification when the pane is focused and its terminal is in front, and a pending notification goes away when you return to the pane.
- **Markers**: an icon (default `❕` on the tab label, `🟡` on the workspace label, `●` on the agent name) in front of the tab label, the workspace label and the agent name while an agent needs you. They clear when the agent moves on or you focus the pane.
- **Configurable text**: title, subtitle, message, sound, timeout and which statuses notify, with per-status overrides and Herdr's own template tokens.
- **Robust delivery**: a repeated agent status does not notify twice, and a notification that alerter closes when you click another one is posted again.

## Requirements

- macOS
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
title = "⌂ {workspace} ▸ {tab}"
subtitle = ""   # empty: no subtitle line
message = "{state_icon} {terminal_title_stripped}"
sound = ""      # an alerter sound name, e.g. "default"; empty plays no sound
timeout = 0     # seconds before an unclicked notification is dismissed; 0 keeps it

[notify.state_icons]      # what {state_icon} renders, per status
blocked = "×"
done = "✓"
working = "◐"
idle = "○"
unknown = "·"

[notify.tab_marker]       # icon in front of the tab label of agents that need you
enabled = true
icon = "❕"
statuses = ["blocked", "done"]

[notify.workspace_marker] # the same, in front of the workspace label
enabled = true
icon = "🟡"
statuses = ["blocked", "done"]

[notify.agent_marker]     # the same, in front of the agent name on the pane
enabled = true
icon = "●"
statuses = ["blocked", "done"]

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
| `{state_icon}` | `×` blocked, `✓` done, `◐` working, `○` idle, `·` other; set in `[notify.state_icons]` |
| `{state_text}` | Herdr's status label, else e.g. `Blocked` |
| `{branch}` | Worktree branch |
| `{git_status}` | `*` when tracked files changed |
| `{hostname}` | Short host name |
| `$name` | Custom pane value, set with `herdr pane report-metadata <pane> --source <id> --token name=value` |
| `{{` `}}` | Literal braces |

A token without a value renders as an empty string. An unknown `{token}` is shown as written.

## Actions

- `dagadbm.notify-test`: send a test notification for the focused pane, using your templates.
- `dagadbm.notify-init-config`: write the default config if none exists.
- `dagadbm.notify-clear-terminal-bindings`: forget which terminal app shows each workspace.

## Development

The code is organized so that an AI agent, or a person, can find their way quickly:

- [`AGENTS.md`](AGENTS.md): the entry point, with the layout and the commands to format, lint, test, build and release.
- [`docs/architecture.md`](docs/architecture.md): how the plugin works and where to change what.
- [`docs/glossary.md`](docs/glossary.md): the terms used in the code and their Herdr names.
- [`docs/dev-guidelines.md`](docs/dev-guidelines.md) and [`docs/test-guidelines.md`](docs/test-guidelines.md): how to make changes and test them. The tests run against a real, isolated Herdr server, so `cargo test` needs Herdr installed and never touches your own session.
- [`docs/adr`](docs/adr): one record per design decision.

## License

MIT. The notify feature is derived from herdr-focus-notify by yankewei (MIT). The agent icons have their own notice in `src/notify/assets/icons/NOTICE.md`.
