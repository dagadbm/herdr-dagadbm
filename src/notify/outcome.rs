//! How a notification ended, as its generated script reports it: a click, a
//! close, or anything else. A click also puts back the notifications alerter
//! 26.5 closed along with it (ADR 12).

use std::thread;
use std::time::Duration;

use crate::notify::executable::resolve_herdr_bin;
use crate::notify::focus;
use crate::notify::ledger::{self, Status, CLICK_WINDOW};
use crate::notify::notifier::{resolve_notifier_bin, send_notification, shown_groups};
use crate::notify::script::{write_focus_script, AlerterOptions};
use crate::notify::state::cleared_notification_marker_path;

/// How long a close waits before it reads the click marker. A click and the
/// close it causes report at about the same moment, and the click's stamp has
/// to land first.
const CLOSE_SETTLE: Duration = Duration::from_millis(200);

/// The user clicked the notification: it is done, and its pane is focused.
/// Then, after the click window, every other pending notification that is no
/// longer shown is posted again.
pub(crate) fn clicked(pane_id: &str, post_id: &str) -> Result<(), String> {
    // Resolved first, so a second click that re-posts does not take this one
    // for a notification it closed.
    let _ = ledger::resolve(pane_id, Some(post_id), Status::Done);
    let focused = focus::focus_pane(pane_id);
    repost_closed_by_click(pane_id);
    focused
}

/// The notification was closed: by the user, by focusing its pane, or by
/// alerter as a side effect of a click on another notification. Only the last
/// one leaves it pending, for the click to post again; a close that cannot be
/// told apart counts as the user's, because showing a notification the user
/// closed is worse than missing one.
pub(crate) fn closed(pane_id: &str, post_id: &str) -> Result<(), String> {
    thread::sleep(CLOSE_SETTLE);
    let status = if cleared_notification_marker_path(pane_id).exists() {
        Status::Done
    } else if ledger::click_is_recent() {
        return Ok(());
    } else {
        Status::Dismissed
    };
    ledger::resolve(pane_id, Some(post_id), status)
        .map_err(|err| format!("failed to record the closed notification: {err}"))
}

/// The notification ended any other way: it timed out, was removed, or
/// alerter failed.
pub(crate) fn ended(pane_id: &str, post_id: &str) -> Result<(), String> {
    ledger::resolve(pane_id, Some(post_id), Status::Done)
        .map_err(|err| format!("failed to record the ended notification: {err}"))
}

/// Posts again, silently and with their stored text, the pending notifications
/// of other panes that are no longer shown. Best-effort: without alerter's
/// list nothing is known, and nothing is posted.
fn repost_closed_by_click(clicked_pane: &str) {
    let others_pending = || {
        ledger::pending_records()
            .into_iter()
            .any(|record| record.notification.pane_id != clicked_pane)
    };
    if !others_pending() {
        return;
    }
    thread::sleep(CLICK_WINDOW);

    let (Ok(notifier_bin), Ok(herdr_bin)) = (resolve_notifier_bin(), resolve_herdr_bin()) else {
        return;
    };
    let listed_at = ledger::now_ms();
    let Some(shown) = shown_groups(&notifier_bin) else {
        return;
    };

    for record in ledger::pending_records() {
        let notification = &record.notification;
        if notification.pane_id == clicked_pane || shown.contains(&notification.group) {
            continue;
        }
        // Posted too recently for the list to show it yet.
        if record.posted_at_ms + CLICK_WINDOW.as_millis() as u64 > listed_at {
            continue;
        }
        // Its pane was focused meanwhile.
        if cleared_notification_marker_path(&notification.pane_id).exists() {
            let _ = ledger::resolve(&notification.pane_id, Some(&record.post_id), Status::Done);
            continue;
        }
        let Some(record) = ledger::claim_repost(&record) else {
            continue;
        };
        let options = AlerterOptions {
            timeout_secs: record.timeout_secs,
            sound: None,
        };
        if let Ok(script_path) = write_focus_script(
            &record.notification,
            &record.post_id,
            &herdr_bin,
            &notifier_bin,
            &options,
            record.monitor_visibility,
        ) {
            let _ = send_notification(&script_path, false);
        }
    }
}
