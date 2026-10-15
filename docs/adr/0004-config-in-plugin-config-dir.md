# 4. Config lives in the plugin config directory

Date: 2026-10-15

## Status

Accepted

## Context

The first preference was to keep the plugin's settings inside Herdr's own `config.toml`, so everything is in one file. We tested this with `herdr config check` on Herdr 0.9.3, and Herdr rejects both options:

- An unknown section fails with `unknown config section [plugins]; ignoring section` and exit code 1.
- An unknown top-level key fails with `unknown config key dagadbm-notify-title; ignoring key` and exit code 1.

Other published plugins, such as herdr-file-viewer, herdr-tiny-fingers, herdr-file-picker and herdr-context, keep their settings in `$HERDR_PLUGIN_CONFIG_DIR`. They only put `[[keys.command]]` bindings in Herdr's `config.toml`.

## Decision

Read settings from `$HERDR_PLUGIN_CONFIG_DIR/config.toml`. `herdr plugin config-dir dagadbm` prints that directory. Each feature owns one section, for example `[notify]`.

Every key is optional and falls back to a default hardcoded in the feature. The file is re-read on every invocation, so edits apply to the next event without a reload. A file that cannot be parsed is reported on stderr, which Herdr keeps in the plugin log, and the defaults apply.

The repository ships a commented `config.example.toml` at its root that lists every default, as other plugins do. The binary embeds it, and the `notify-init-config` action writes it to the config directory. It only writes when no config file exists.

## Consequences

- `herdr config check` stays clean, and future Herdr versions cannot break the plugin's settings.
- The settings live in a second file, outside Herdr's `config.toml`.
- No config file is needed: without one, every default applies.
