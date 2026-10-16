# Glossary

One entry per term used in the code and the docs. Each entry gives the definition, the code identifiers, the file that defines them, and Herdr's own name for the same thing. Template tokens use only Herdr's names (ADR 5). Runtime names start with the sub-plugin name (ADR 14). How the pieces fit together is in [architecture.md](architecture.md).

Entries are grouped by area. The last section lists synonyms and inconsistent names found in the code.

## Plugin structure

### Plugin id
The id Herdr knows the plugin by: `dagadbm`. It is also the binary name and the metadata source of the agent marker.
- Code: `SOURCE`
- File: `herdr-plugin.toml`, `Cargo.toml`, `src/notify/tab_marker.rs`
- Herdr: plugin `id`, as in `herdr plugin config-dir dagadbm` and `herdr plugin log list --plugin dagadbm`.

### Feature
One self-contained part of the plugin, run as `dagadbm <feature>`. Its code lives in `src/<feature>/`. The only feature is `notify`.
- Code: `run`, `USAGE`
- File: `src/main.rs`
- Herdr: none. Herdr sees one plugin (ADR 1).

### Sub-plugin name
`herdr-dagadbm-<feature>`, the prefix of every runtime name a feature creates. For `notify` it is `herdr-dagadbm-notify`. The binary, the module and the config section keep the short feature name (ADR 14).
- Code: `PREFIX`, `prefixed`
- File: `src/notify/util.rs`
- Herdr: none.

### Shared code
Code that two features need. It goes in `src/shared/`, which does not exist yet, and takes the sub-plugin prefix as an argument (ADR 1, ADR 14).
- Code: none yet.
- File: none yet.
- Herdr: none.

### Hook
An entry in `herdr-plugin.toml` that makes Herdr run the binary: `[[build]]`, `[[startup]]`, `[[actions]]` or `[[events]]`.
- Code: the `command` arrays.
- File: `herdr-plugin.toml`
- Herdr: same names.

### Action
A hook the user runs by hand. Its id carries the feature name as a prefix: `notify-test`, `notify-init-config`, `notify-clear-terminal-bindings` (ADR 1).
- Code: `CliAction::Test`, `CliAction::InitConfig`, `CliAction::ClearTerminalBindings`
- File: `herdr-plugin.toml`, `src/notify/cli.rs`
- Herdr: action, invoked as `herdr plugin action invoke dagadbm.<id>`.

### CLI action
What one call of `dagadbm notify` does, parsed from its arguments. The table of all of them is in [architecture.md](architecture.md#entry-points-of-dagadbm-notify).
- Code: `CliAction`, `parse_cli_args`, `print_usage`
- File: `src/notify/cli.rs`
- Herdr: none.

## Herdr concepts

### Plugin event
The event Herdr runs an `[[events]]` hook for. Its name is in `HERDR_PLUGIN_EVENT` and its data in `HERDR_PLUGIN_EVENT_JSON`. `notify` handles `pane.agent_status_changed` and `pane.focused`.
- Code: `PluginEvent`, `EventData`, `status_event_from_json`, `focused_pane_id_from_event_json`
- File: `src/notify/event.rs`
- Herdr: event, with fields `pane_id`, `agent_status`, `agent`, `display_agent`, `title` and `state_labels` under `data`.

### Workspace
A Herdr workspace. Its id is the part of a pane id before the colon (`w1`). Terminal bindings are kept per workspace.
- Code: `workspace_of`, `workspace_id_from_pane_id`, `workspace_label`, `WorkspaceInfo`
- File: `src/notify/util.rs`, `src/notify/focus.rs`
- Herdr: workspace, `workspace_id`; its `label` from `herdr workspace list`. Token `{workspace}`.

### Tab
A Herdr tab. Its id looks like `w1:t7`. An unnamed tab is known by its number.
- Code: `tab_id`, `tab_label`, `TabInfo`, `tab_label_from_get_json`
- File: `src/notify/focus.rs`
- Herdr: tab, `tab_id`, `label` and `number` from `herdr tab get`. Token `{tab}`.

### Pane id
Herdr's id of a pane, `workspace:pane`, for example `w1:p3`. The pane part alone renders as `{pane}`.
- Code: `pane_id`, `pane_name`
- File: `src/notify/event.rs`
- Herdr: `pane_id`. Herdr gives panes no name of their own (ADR 5).

### Agent
The program running in a pane, such as Codex or Claude. In the code, `agent` is the display name and `agent_id` is Herdr's raw `agent` field (see [Synonyms](#synonyms-and-inconsistent-names)).
- Code: `StatusEvent`, `agent`, `agent_id`
- File: `src/notify/event.rs`
- Herdr: `display_agent`, falling back to `agent`. Token `{agent}`.

### Display agent
The agent name Herdr shows on the pane border. The agent marker changes it.
- Code: `report_display_agent`
- File: `src/notify/tab_marker.rs`
- Herdr: `display_agent`, set with `herdr pane report-metadata <pane> --source <id> --display-agent <name>` and removed with `--clear-display-agent`.

### Agent status
What an agent is doing: `blocked`, `done`, `working`, `idle`, or another value. The code lowercases and trims it.
- Code: `status` in `StatusEvent`
- File: `src/notify/event.rs`
- Herdr: `agent_status` in the event.

### State label
Herdr's own text for a status, if the event carries one, for example `Needs an answer`. Without it, `{state_text}` is the capitalized status.
- Code: `state_label`, `state_text`
- File: `src/notify/event.rs`
- Herdr: `state_labels` in the event. Token `{state_text}`.

### State icon
The glyph `{state_icon}` renders for a status. Defaults: `×` blocked, `✓` done, `◐` working, `○` idle, `·` any other. `[notify.state_icons]` overrides any status (ADR 7).
- Code: `state_icon`, `state_icons`
- File: `src/notify/event.rs`, `src/notify/config.rs`
- Herdr: the glyphs of `status_indicators = "symbols"`. Token `{state_icon}`.

### Terminal title
The pane's terminal title. The stripped form has no status glyph in front.
- Code: `terminal_title`, `terminal_title_stripped`
- File: `src/notify/focus.rs`, `src/notify/event.rs`
- Herdr: `title` in the event; `terminal_title` and `terminal_title_stripped` in `herdr agent get`. Tokens `{terminal_title}`, `{terminal_title_stripped}`.

### Pane metadata
Custom values a pane reported to Herdr. A template renders them as `$name`. Strings, numbers and booleans render; other values do not.
- Code: `metadata`, `metadata_text`
- File: `src/notify/focus.rs`, `src/notify/template.rs`
- Herdr: values set with `herdr pane report-metadata --token <name>=<value>`; `tokens` in `herdr agent get`, read into `AgentInfo` in `src/notify/focus.rs`.

### Worktree branch
The branch of the worktree that owns the pane's directory: the longest worktree path the directory starts with. A detached `HEAD` has no branch.
- Code: `worktree_branch`, `branch_from_worktree_list_json`, `WorktreeInfo`
- File: `src/notify/focus.rs`
- Herdr: `herdr worktree list --cwd`. Token `{branch}`.

### Git status
`*` when the pane's directory has tracked changes versus `HEAD`, else empty. Only checked when there is a branch.
- Code: `is_dirty`, `dirty`
- File: `src/notify/git.rs`, `src/notify/notification.rs`
- Herdr: token `{git_status}`.

### Herdr socket
The Unix socket of the Herdr API. A click sends one `pane.focus` request on it.
- Code: `focus_pane_via_socket`, `FOCUS_SOCKET_TIMEOUT`
- File: `src/notify/focus.rs`
- Herdr: `HERDR_SOCKET_PATH`, method `pane.focus`.

### Herdr client
A process of a terminal tab attached to the Herdr session. Every client shows the same focused pane. Clients are found through the session's client socket.
- Code: `HerdrClient`, `herdr_clients`, `client_socket_path`, `last_input`
- File: `src/notify/terminal/client.rs`
- Herdr: client; `HERDR_CLIENT_SOCKET_PATH`, else `<socket stem>-client.sock`.

### State directory
Where `notify` keeps all its files. The full list is in [architecture.md](architecture.md#runtime-state).
- Code: `plugin_state_dir`
- File: `src/notify/state.rs`
- Herdr: `HERDR_PLUGIN_STATE_DIR`. Without it: `<temp dir>/herdr-dagadbm-notify`.

### Config directory
Where the plugin's `config.toml` lives.
- Code: `config_path`
- File: `src/notify/config.rs`
- Herdr: `HERDR_PLUGIN_CONFIG_DIR`, printed by `herdr plugin config-dir dagadbm`.

## Config

### Notify config
The parsed `[notify]` section with defaults applied.
- Code: `NotifyConfig`, `load`, `parse`
- File: `src/notify/config.rs`
- Herdr: none (ADR 4).

### Enabled statuses
The agent statuses that send a notification. Config key `notify_on`, default `["blocked", "done"]`.
- Code: `statuses`, `status_is_enabled`, `DEFAULT_STATUSES`
- File: `src/notify/config.rs`
- Herdr: none.

### Templates
The three texts of a notification: title, subtitle and message. Each is looked up in the status's own table, then `[notify]`, then the default (ADR 7).
- Code: `Templates`, `TemplateOverrides`, `templates_for`, `DEFAULT_TITLE`, `DEFAULT_SUBTITLE`, `DEFAULT_MESSAGE`
- File: `src/notify/config.rs`
- Herdr: none; the syntax is Herdr's (ADR 5).

### Per-status override
A `[notify.status.<status>]` table that replaces title, subtitle or message for one status.
- Code: `per_status`
- File: `src/notify/config.rs`
- Herdr: none.

### Sound and timeout
An alerter sound name (empty plays nothing) and the seconds before an unclicked notification is dismissed (0 keeps it).
- Code: `sound`, `timeout_secs`, `AlerterOptions`
- File: `src/notify/config.rs`, `src/notify/script.rs`
- Herdr: none; alerter's `--sound` and `--timeout`.

### Marker
An icon and a space put in front of a label to show that an agent needs the user. The tab marker changes the tab label; the agent marker changes the display agent. Each has `enabled`, `icon` and `statuses` (ADR 10). An empty icon turns it off.
- Code: `MarkerConfig`, `MarkerSection`, `marks`, `tab_marker`, `agent_marker`, `DEFAULT_MARKER_ICON`, `sync_for_status`, `clear_for_focus`, `marked_label`, `unmarked_label`
- File: `src/notify/config.rs`, `src/notify/tab_marker.rs`
- Herdr: tab `label` (`herdr tab rename`) and `display_agent`.

### Example config
The commented file that lists every key with its default. It is embedded in the binary and written by `--init-config` when no config exists.
- Code: `EXAMPLE_CONFIG`, `init_config`
- File: `config.example.toml`, `src/notify/config.rs`
- Herdr: none.

## Templates

### Template
A string with tokens, rendered exactly as written (ADR 6).
- Code: `render`
- File: `src/notify/template.rs`
- Herdr: the same syntax as Herdr's window title and sidebar rows in its own config (ADR 5).

### Token
A `{name}` placeholder from the fixed list, or a `$name` metadata placeholder. `{{` and `}}` are literal braces. An unknown `{name}` stays as written.
- Code: `TOKENS`, `TokenValues`, `builtins`, `token_values`
- File: `src/notify/template.rs`, `src/notify/event.rs`
- Herdr: the same names (ADR 5).

## Notifications

### Notification
One rendered notification, ready for alerter, and the text the ledger stores for a re-post.
- Code: `FocusNotification`, `render_notification`
- File: `src/notify/notification.rs`, `src/notify/event.rs`
- Herdr: none. Herdr's own toasts are a separate feature, kept off (ADR 9).

### Pane details
What one `herdr agent get` says about a pane: focused or not, its directory, tab, titles and metadata.
- Code: `PaneDetails`, `pane_details`, `pane_details_from_get_json`, `AgentInfo`
- File: `src/notify/focus.rs`
- Herdr: `herdr agent get <pane>`.

### Pane labels
What the other Herdr and git lookups add to a notification: workspace and tab labels, branch, dirty flag.
- Code: `PaneLabels`, `pane_labels`
- File: `src/notify/notification.rs`, `src/notify/focus.rs`
- Herdr: `herdr workspace list`, `herdr tab get`, `herdr worktree list`.

### Status event
What a `pane.agent_status_changed` event says, before any lookup.
- Code: `StatusEvent`, `status_event_from_json`
- File: `src/notify/event.rs`
- Herdr: the event's `data`.

### Notifier
The program that shows notifications: always alerter (ADR 3).
- Code: `resolve_notifier_bin`, `notifier_bin`, `send_notification`, `remove_notification`, `shown_groups`
- File: `src/notify/notifier.rs`
- Herdr: none.

### Alerter group
The alerter `--group` of a pane's notifications, `herdr-dagadbm-notify-<pane>`. A new notification in the group replaces the old one.
- Code: `notification_group_id`, `is_notification_group_id`, `sanitize_group_id`, `group`
- File: `src/notify/util.rs`, `src/notify/notification.rs`
- Herdr: none.

### Generated script
The shell script that shows one posting of a notification and reports how it ended. One per pane, rewritten on each send.
- Code: `write_focus_script`, `alerter_focus_script`, `is_generated_script`, `rewrite_generated_scripts_without_activation`
- File: `src/notify/script.rs`
- Herdr: none.

### Notification decision
Whether to skip, send, or send with a visibility monitor. An unfocused pane always sends. A focused pane is skipped when the frontmost app is the bound terminal.
- Code: `NotificationDecision`, `notification_decision`, `notification_decision_from_bundles`
- File: `src/notify/focus.rs`
- Herdr: `focused` in `herdr agent get`.

### Visibility monitor
A loop in the generated script that runs `--check-pane-visibility` every 2 s and removes the notification once the user can see the pane.
- Code: `monitor_visibility`, `SendWithVisibilityMonitor`, `CheckPaneVisibility`
- File: `src/notify/script.rs`, `src/notify/focus.rs`, `src/notify/cli.rs`
- Herdr: none.

### Test notification
A notification for the focused pane with status `blocked`, sent by the `notify-test` action. It is never skipped and times out within 10 s.
- Code: `Test`, `test_timeout_secs`
- File: `src/notify/cli.rs`, `src/notify/script.rs`
- Herdr: none.

### Agent icon
A bundled PNG chosen by agent name and written to the icons folder for alerter's `--app-icon`.
- Code: `agent_icon_path`, `AGENT_ICONS`, `AgentIcon`, `icon_for_agent`, `normalize_agent_name`
- File: `src/notify/icons.rs`
- Herdr: none.

## Focus and terminals

### Terminal binding
Which terminal app (bundle id) shows a workspace. It is learned from `pane.focused` events and stored in the terminal memory file. A click activates the bound terminal; without a binding a click does not focus.
- Code: `remember_terminal`, `remembered_terminal`, `learn_terminal_from_frontmost`, `clear_terminal_bindings`, `prune_stale_workspace_bindings`, `terminal_memory_path`
- File: `src/notify/state.rs`, `src/notify/focus.rs`
- Herdr: none.

### Bundle id
A macOS app identifier, such as `com.googlecode.iterm2`.
- Code: `bundle_id`, `frontmost_bundle_id`, `bundle_id_from_lsappinfo`, `is_obvious_non_terminal_bundle`
- File: `src/notify/focus.rs`, `src/notify/state.rs`, `src/notify/terminal/mod.rs`
- Herdr: none.

### Focus origin marker
A short-lived file saying that a click is focusing a pane in a workspace, so the `pane.focused` event it causes does not learn a binding.
- Code: `mark_focus_origin`, `focus_origin_is_active`, `clear_focus_origin`, `FOCUS_ORIGIN_RETENTION`
- File: `src/notify/state.rs`
- Herdr: none.

### Cleared marker
A file saying the user saw the pane, so its notification is over. The generated script exits at start if it exists.
- Code: `mark_notification_cleared`, `reset_notification_clearance`, `cleared_notification_marker_path`, `should_clear_notification_on_focus`
- File: `src/notify/state.rs`, `src/notify/focus.rs`
- Herdr: none.

### Terminal adapter
Code for one terminal app that maps a Herdr client to a command focusing its container (ADR 12).
- Code: `TerminalAdapter`, `ADAPTERS`, `adapter_for`, `FocusCommand`, `raise_client_container`, `run_focus_command`
- File: `src/notify/terminal/mod.rs`
- Herdr: none.

### Container
The terminal tab, window or session that runs one Herdr client (ADR 12).
- Code: `focus_command`
- File: `src/notify/terminal/mod.rs`
- Herdr: none.

### Activate
Bring the bound terminal app to the front with `open -b`.
- Code: `activate_terminal`
- File: `src/notify/focus.rs`
- Herdr: none.

## Ledger and outcomes

### Ledger
One record per pane of the latest notification sent, so a notification alerter closed by mistake can be posted again (ADR 13).
- Code: `Record`, `save`, `resolve`, `pending_records`, `is_ledger_file`, `namespace`
- File: `src/notify/ledger.rs`
- Herdr: none.

### Ledger status
Where a notification is in its life: `pending` (shown, or waiting to be re-posted), `done` (dealt with) or `dismissed` (the user closed it).
- Code: `Status`, `Pending`, `Done`, `Dismissed`
- File: `src/notify/ledger.rs`
- Herdr: none. Not the same as the agent status.

### Posting
One showing of a notification: the first send or a re-post. Its id, `post_id`, has the form `<ms since epoch>-<pid>-<counter>`. It is passed to the generated script, which reports back with it. A re-post gets a new one.
- Code: `post_id`, `posted_at_ms`, `sent_at_ms`
- File: `src/notify/ledger.rs`
- Herdr: none.

### Outcome
How a posting ended, as the generated script reports it: clicked, closed, or ended any other way.
- Code: `Outcome`, `Clicked`, `Closed`, `Ended`, `NotificationEnded`, `clicked`, `closed`, `ended`
- File: `src/notify/cli.rs`, `src/notify/outcome.rs`
- Herdr: none.

### Re-post
Posting again, silently and with the stored text, a pending notification that a click on another notification closed.
- Code: `repost_closed_by_click`, `claim_repost`
- File: `src/notify/outcome.rs`, `src/notify/ledger.rs`
- Herdr: none.

### Claim
A file created with `create_new` that gives one click the right to re-post one posting.
- Code: `claim_repost_in`
- File: `src/notify/ledger.rs`
- Herdr: none.

### Click marker and click window
A file the generated script touches on every click. A close within `CLICK_WINDOW` (1 s) of it is collateral. A close waits `CLOSE_SETTLE` (200 ms) before reading it.
- Code: `click_marker_path`, `click_is_recent`, `CLICK_WINDOW`, `CLOSE_SETTLE`
- File: `src/notify/ledger.rs`, `src/notify/outcome.rs`
- Herdr: none.

### Collateral close
A close that alerter 26.5 causes when the user clicks another notification. It prints `@CLOSED`, like a real close (ADR 13).
- Code: `closed`
- File: `src/notify/outcome.rs`
- Herdr: none.

## Runtime helpers

### Last status
The last agent status seen for a pane, used to skip repeated events (ADR 11).
- Code: `record_status`, `record_status_at`, `last_status_path`
- File: `src/notify/state.rs`
- Herdr: none.

### Retention and cleanup
How long each state file is kept, and the pass that removes stale ones.
- Code: `cleanup_stale_state_files`, `retention_for`, `GENERATED_SCRIPT_RETENTION`, `TEMP_FILE_RETENTION`
- File: `src/notify/state.rs`
- Herdr: none.

### Legacy state
State files with names from before ADR 14. Cleanup deletes them and renames the old terminal memory.
- Code: `is_legacy_state_file`, `remove_legacy_state_in`
- File: `src/notify/state.rs`
- Herdr: none.

### Lookup budget and deadline
One deadline shared by every lookup behind one notification. A call that would start after it does not run.
- Code: `LOOKUP_BUDGET`, `MARKER_BUDGET`, `deadline`
- File: `src/notify/mod.rs`, `src/notify/tab_marker.rs`
- Herdr: none.

### Bounded command
A command run with a deadline, its output read on another thread, and its whole process group killed afterwards. The unbounded form just runs and returns stdout.
- Code: `command_stdout_until`, `kill_process_group`, `command_stdout`
- File: `src/notify/util.rs`
- Herdr: none.

### Executable lookup
How `herdr` and `alerter` are found: `PATH` first, then fixed candidate paths. `HERDR_BIN_PATH` wins for `herdr`.
- Code: `resolve_herdr_bin`, `find_executable`, `executable_in_path`, `is_executable_file`, `resolve_notifier_bin`
- File: `src/notify/executable.rs`, `src/notify/notifier.rs`
- Herdr: `HERDR_BIN_PATH`.

## Synonyms and inconsistent names

These names differ between the config, the code and Herdr. Use the code name when you search, and the config or Herdr name when you talk to users.

- **`message` and `body`.** The config key and `Templates` field are `message` (`src/notify/config.rs`). The same text is `body` in `FocusNotification` (`src/notify/notification.rs`).
- **`notify_on` and `statuses`.** The config key is `notify_on`. The `NotifyConfig` field is `statuses`, read through `status_is_enabled` (`src/notify/config.rs`). The markers' config key is also `statuses`, which means "statuses that get a marker", a different list.
- **`timeout` and `timeout_secs`.** Config key versus field in `NotifyConfig`, `AlerterOptions` and `Record`.
- **`agent` and `agent_id`.** Herdr's event has `agent` (raw id) and `display_agent` (name). In `StatusEvent` (`src/notify/event.rs`), `agent` holds the display name, falling back to the raw id, and `agent_id` holds Herdr's `agent`. The names are swapped relative to Herdr.
- **`title` and `terminal_title`.** Herdr's event field `title` becomes `terminal_title` in `StatusEvent`. The notification also has a `title`, which is the rendered title template.
- **Status means three things.** The agent status (a string in `StatusEvent`), the ledger `Status` enum (`src/notify/ledger.rs`), and Herdr's "state" in `state_icon`, `state_text` and `state_labels`, which name the same agent status.
- **"Ended" means two things.** `CliAction::NotificationEnded` covers every outcome, while `Outcome::Ended` is only the "anything else" case (`src/notify/cli.rs`).
- **Notifier and alerter.** `notifier.rs`, `notifier_bin` and `resolve_notifier_bin` mean alerter; `AlerterOptions` and `alerter_focus_script` say alerter.
- **Binding, memory, remembered, bound.** One concept, the terminal binding: `remember_terminal`, `remembered_terminal`, `terminal_memory_path`, `clear_terminal_bindings`, `bound_terminal` (`src/notify/state.rs`, `src/notify/focus.rs`).
- **Cleared, clearance, clear.** The cleared marker is written by `mark_notification_cleared`, deleted by `reset_notification_clearance`, and decided by `should_clear_notification_on_focus`. `clear_for_focus` in `src/notify/tab_marker.rs` is unrelated: it removes markers.
- **Group id used for non-groups.** `sanitize_group_id` (`src/notify/util.rs`) also builds focus-origin marker names and ledger file names.
- **Focus names that do more.** `focus.rs` holds most Herdr queries. `FocusNotification` is any rendered notification. `write_focus_script` writes the whole notification script, not only the focus step.
- **Plugin id versus sub-plugin name.** Files and alerter groups use `herdr-dagadbm-notify`. The Herdr metadata source (`SOURCE`), the socket request id `dagadbm:focus` and error messages use `dagadbm`.
