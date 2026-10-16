# 12. Re-post notifications that a click closes

Date: 2026-10-15

## Status

Accepted

## Context

alerter 26.5 has a bug: clicking one notification also closes another one, the newest of the others. Every alerter process posts as `com.apple.Terminal`, so `didDeliver` runs in every process for every notification and sets that process's `currentNotification` to the newest one. On a click, `didActivate` removes `currentNotification` as well. The process that owned the removed notification sees it disappear and prints `@CLOSED`.

Measured outputs with `--actions Focus` and no `--close-label`:

| What happened | alerter prints |
| --- | --- |
| Click on `Focus` | `Focus` |
| The user pressed the close button | `@CLOSED` |
| Closed by a click on another notification | `@CLOSED` |
| Timeout | `@TIMEOUT` |
| Click on the body (not measured) | `@CONTENTCLICKED` |

A collateral close and a real close print the same thing. Only the timing tells them apart: a collateral close comes right after a click on another notification. Each pane already has its own alerter group, `herdr-dagadbm-notify-<workspace>-<pane>`.

## Decision

**One button.** The plugin passes `--actions Focus` and no `--close-label`. The `action_label` and `close_label` config keys are gone (ADR 7). The generated script matches `Focus`, `@ACTIONCLICKED` and `@CONTENTCLICKED` as a click.

**Ledger.** Every sent notification gets a record in `$HERDR_PLUGIN_STATE_DIR/herdr-dagadbm-notify.repost.record.<pane>.json` (`src/notify/ledger.rs`). It holds the pane, the alerter group, the rendered title, subtitle and message, the app icon, the timeout, whether the visibility monitor ran, the send time, and a status:

- `pending`: shown, or closed by a click and waiting to be posted again.
- `done`: the pane was focused (the `.cleared` marker flow), the notification was clicked, it timed out, or it ended any other way.
- `dismissed`: the user pressed the close button.

A new notification for a pane replaces that pane's record, as its alerter group replaces the old notification on screen. Each posting has a `post_id`. The generated script reports back with it, so a script that outlived its posting changes nothing. The sound is not stored. Startup cleanup removes records, re-post claims and the click marker older than 24 hours, like `.last-status` files (ADR 10).

**Click.** The script touches `herdr-dagadbm-notify.repost.click` and then runs `notify --notification-clicked <pane> <post_id>`. That command marks the record done and focuses the pane. If another record is pending, it then waits one second, runs `alerter --list ALL` and keeps only the `herdr-dagadbm-notify-` groups. Every other pending record whose group is not listed is posted again, by a new script with its own notifier loop, so it can be clicked or closed again. The re-post uses the stored strings and plays no sound.

**No double post.** The re-poster takes a claim file per posting, `herdr-dagadbm-notify.repost.claim.<pane>.<post_id>`, created with `create_new`, and checks that the record is still pending and on that posting. It also skips a record posted within the last second, which `--list` may not show yet. When two clicks happen at the same time, only one of them re-posts.

**Close.** On `@CLOSED` the script runs `notify --notification-closed`. That command waits 200 ms, so the click's marker can land first. Then:

- If the pane was focused (`.cleared` exists), the record is done.
- If the click marker is younger than one second, the close is collateral. The record stays pending, and the clicking process re-posts it.
- Otherwise the record is dismissed and stays closed.

A close that cannot be told apart counts as the user's. Missing a re-show costs less than bringing back a notification the user closed.

**Anything else.** `@TIMEOUT`, any other output, or a failed alerter runs `notify --notification-ended`, which marks the record done.

## Consequences

- Clicking a notification no longer loses another one: it disappears for about a second and comes back with the same text.
- A re-posted notification is new to Notification Center. It moves to the top and shows the re-post time, not the original send time. Its timeout starts again.
- A real close within one second of a click on another notification counts as collateral if that pane is not focused. The notification comes back.
- A record stays pending if its notifier died without reporting (for example on a reboot). A click within 24 hours can post it again if it is no longer listed.
- When `alerter --list ALL` fails or cannot run, nothing is known about what is shown, so nothing is re-posted and the records stay pending. A list that succeeds but prints garbage reads as empty, so every other pending record is re-posted.
- The fix belongs upstream. alerter should check in `didDeliver` that the delivered notification is its own (compare the uuid) before it sets `currentNotification`. When a fixed alerter ships, the ledger and the close handling can go.
