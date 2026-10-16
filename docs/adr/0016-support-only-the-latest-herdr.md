# 16. Support only the latest Herdr

Date: 2026-10-16

## Status

Accepted

## Context

Herdr changes its CLI between versions. Herdr 0.9.5 returns custom pane values as `tokens` in `agent get`, where the plugin expected `metadata`. Supporting several Herdr versions would mean keeping old field names and branches that nothing tests. This is a personal plugin, and its one user updates Herdr.

## Decision

Support only the latest Herdr release. `min_herdr_version` in `herdr-plugin.toml` names it. When Herdr changes, the plugin follows the new behaviour and drops the old one. It keeps no aliases or fallbacks for older versions.

## Consequences

- The code has one path per Herdr call, and the tests run against the installed, latest `herdr` (ADR 15).
- After a Herdr update, restart the Herdr server, or a running older server will answer the new plugin.
- Bump `min_herdr_version` with each Herdr update the plugin depends on.
