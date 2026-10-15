# 6. Templates render exactly as configured

Date: 2026-10-15

## Status

Accepted

## Context

A token can have no value. For example, a pane outside a git repository has no branch. A template like `"{agent} · {branch}"` then leaves a dangling separator. We considered three options: removing separators that end up empty, using an array of parts joined with a separator, or rendering the plain string.

## Decision

Render the template string as written. A token without a value becomes an empty string, and nothing around it is cleaned up. The config should match the output.

There are two exceptions, both forced by `alerter`:

- A subtitle that renders to whitespace only is left out instead of being shown as a blank line.
- An empty message is sent as a single space (ADR 3).

## Consequences

- The renderer is simple and predictable.
- Users who want no dangling separators write templates without them, or use per-status overrides (ADR 7).
