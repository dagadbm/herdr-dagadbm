# 8. Identity, install and rollout

Date: 2026-10-15

## Status

Accepted

## Context

The plugin needs an id, a repository, a development loop, and a way to replace the installed herdr-focus-notify without sending duplicate notifications.

## Decision

- The plugin id is `dagadbm`, and the repository is `dagadbm/herdr-dagadbm`.
- For development, build the binary, then link the plugin: `cargo build --release && herdr plugin link .`. Linking does not run the build.
- The plugin is installed from GitHub with `herdr plugin install --yes dagadbm/herdr-dagadbm`. Herdr refuses to install over a linked plugin, so `herdr plugin unlink dagadbm` comes first when switching from development.
- herdr-focus-notify is uninstalled. `~/dotfiles/herdr-update.sh` uninstalls it and installs herdr-dagadbm instead.
- The dotfiles do not manage the plugin's config file. The defaults live in the plugin, and a user who wants changes writes the file with `notify-init-config`.
- `ui.toast.delivery` stays `"off"` in Herdr's config, so Herdr's own toasts do not duplicate this plugin's notifications. Its comment names herdr-dagadbm.

## Consequences

- One update script keeps every machine on the same plugin set.
- Only one plugin sends notifications, so none arrive twice.
