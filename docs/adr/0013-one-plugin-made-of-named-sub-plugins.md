# 13. One plugin made of named sub-plugins

Date: 2026-10-16

## Status

Accepted

## Context

The repo is one Herdr plugin with one binary (ADR 1). Each feature keeps its code in `src/<feature>/`. At runtime the features share one state directory, `$HERDR_PLUGIN_STATE_DIR`, and one notification system. A file or an alerter group named only after a pane (`herdr-w1-p3.cleared`) does not show which feature wrote it, and two features could pick the same name.

## Decision

The repo is one big plugin, `herdr-dagadbm`. Each feature is a small sub-plugin named `herdr-dagadbm-<feature>`, for example `herdr-dagadbm-notify`.

- Every file, folder, alerter group and temporary directory that a feature creates at runtime starts with its sub-plugin name.
- When Herdr gives no `HERDR_PLUGIN_STATE_DIR`, a feature uses `<temp dir>/herdr-dagadbm-<feature>`. For `notify` that is `herdr-dagadbm-notify`.
- The binary subcommand, the module and the config section keep the short name: `dagadbm notify`, `src/notify/`, `[notify]`.
- Code that two features need goes in `src/shared/`. Shared code takes the prefix as an argument and does not choose a name of its own.

## Consequences

- A file in the state directory shows its owner at a glance, and features cannot collide.
- `notify` builds every name from one constant, `PREFIX` in `src/notify/util.rs`: the alerter group and per-pane state files (`herdr-dagadbm-notify-<pane>.*`), the generated scripts (`herdr-dagadbm-notify-focus-<hash>.sh`), the focus-origin markers, the terminal memory (`herdr-dagadbm-notify-terminal-memory.json`) and the icons folder (`herdr-dagadbm-notify-icons`).
- Startup cleanup removes the old names (`herdr-<pane>.*`, `focus-<hash>.sh`, `focus-origin-<ws>.marker`, the `icons` folder) and renames the old `terminal-memory.json`, so saved terminal bindings survive. Old scripts that are still running are left alone.
- A feature may group the files of one mechanism in a namespace after the prefix. The ledger of ADR 12 uses `herdr-dagadbm-notify.repost.<kind>...`: `record.<pane>.json`, `claim.<pane>.<post_id>` and `click`. Dots separate the prefix, the namespace and the kind; dashes stay inside identifiers such as `w1-p3`.
- A new feature adds one prefix constant and builds its names from it.
