# 1. One plugin with self-contained features

Date: 2026-10-15

## Status

Accepted

## Context

herdr-dagadbm is meant to collect several small Herdr features over time, added as they are needed. The choice was between one plugin that grows new features, and a repository holding several separate Herdr plugins.

## Decision

Ship one Herdr plugin, `herdr-dagadbm`, built as one binary, `dagadbm`. The binary dispatches on its first argument, the feature name: `dagadbm notify ...`.

Each feature lives in its own folder under `src/` (`src/notify/`). The folder holds all of the feature's code, including assets such as icons. The one exception is the example config: the repository root has a single `config.example.toml`, with one section per feature, as other Herdr plugins ship it. A feature does not import code from another feature. When a second feature needs the same code, that code moves to `src/shared/`. Until then, `src/shared/` does not exist.

Manifest entries are grouped per feature, and action ids carry the feature name as a prefix: `notify-test`, `notify-init-config`, `notify-clear-terminal-bindings`.

## Consequences

- One `herdr plugin install`, one build and one config directory, whatever the number of features.
- Removing a feature means deleting its folder and its manifest entries.
- Each event hook in the manifest names its feature, so a new feature that hooks the same event adds its own `[[events]]` entry instead of sharing one.
