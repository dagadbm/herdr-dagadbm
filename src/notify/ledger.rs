//! The ledger of sent notifications: one record per pane in
//! `$HERDR_PLUGIN_STATE_DIR`, holding the notification exactly as it was
//! shown and whether it still waits for the user.
//!
//! alerter 26.5 closes another notification when one is clicked, and the
//! closed one reports `@CLOSED`, as if the user had dismissed it (ADR 12). The
//! record lets the click put it back with the same text.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::notify::notification::FocusNotification;
use crate::notify::state::plugin_state_dir;
use crate::notify::util::{sanitize_group_id, PREFIX};

/// Every ledger file starts with `herdr-dagadbm-notify.repost.`, followed by
/// its kind: `record.<pane>.json`, `claim.<pane>.<post_id>` or `click`.
fn namespace() -> String {
    format!("{PREFIX}.repost")
}

fn record_name(pane_id: &str) -> String {
    format!("{}.record.{}.json", namespace(), sanitize_group_id(pane_id))
}

/// Touched by the generated script on every click, before it focuses the pane.
fn click_marker_name() -> String {
    format!("{}.click", namespace())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    /// Shown, or closed by another notification's click and about to be shown
    /// again.
    Pending,
    /// Dealt with: the pane was focused, the notification clicked, or it timed
    /// out or ended any other way.
    Done,
    /// The user closed it.
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Record {
    /// Names one posting: the first send or a re-post. The generated script
    /// reports back with it, so a script that outlived its posting changes
    /// nothing.
    pub(crate) post_id: String,
    pub(crate) status: Status,
    /// Milliseconds since the epoch of the first send.
    pub(crate) sent_at_ms: u64,
    /// Milliseconds since the epoch of the latest posting.
    pub(crate) posted_at_ms: u64,
    /// The text as rendered at send time; a re-post shows it unchanged.
    pub(crate) notification: FocusNotification,
    pub(crate) timeout_secs: u64,
    pub(crate) monitor_visibility: bool,
}

impl Record {
    /// A pending record for a notification about to be sent. It keeps no
    /// sound: a re-post is silent.
    pub(crate) fn new(
        notification: FocusNotification,
        timeout_secs: u64,
        monitor_visibility: bool,
    ) -> Self {
        let now = now_ms();
        Self {
            post_id: post_id(now),
            status: Status::Pending,
            sent_at_ms: now,
            posted_at_ms: now,
            notification,
            timeout_secs,
            monitor_visibility,
        }
    }
}

/// Writes the record, replacing the pane's previous one: a new notification
/// replaces the old one on screen as well, through its alerter group.
pub(crate) fn save(record: &Record) -> io::Result<()> {
    let dir = plugin_state_dir();
    fs::create_dir_all(&dir)?;
    save_in(&dir, record)
}

/// Sets the status of the pane's record while it is pending and, with a
/// `post_id`, still that posting.
pub(crate) fn resolve(pane_id: &str, post_id: Option<&str>, status: Status) -> io::Result<()> {
    resolve_in(&plugin_state_dir(), pane_id, post_id, status)
}

pub(crate) fn pending_records() -> Vec<Record> {
    pending_in(&plugin_state_dir())
}

/// Takes the right to re-post `record` and gives it a new posting, or None
/// when another click took it first or the record changed meanwhile.
pub(crate) fn claim_repost(record: &Record) -> Option<Record> {
    claim_repost_in(&plugin_state_dir(), record)
}

pub(crate) fn click_marker_path() -> PathBuf {
    plugin_state_dir().join(click_marker_name())
}

/// Whether a notification was clicked within the click window.
pub(crate) fn click_is_recent(click_window: Duration) -> bool {
    fs::metadata(click_marker_path())
        .and_then(|metadata| metadata.modified())
        .ok()
        .is_some_and(|clicked| {
            // A click stamped in the future is as recent as it gets.
            SystemTime::now()
                .duration_since(clicked)
                .unwrap_or(Duration::ZERO)
                < click_window
        })
}

/// Whether a state file belongs to the ledger, for the startup cleanup.
pub(crate) fn is_ledger_file(name: &str) -> bool {
    name.starts_with(&format!("{}.", namespace()))
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// Unique across processes by the pid, and within one by the counter.
fn post_id(now_ms: u64) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{now_ms}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn record_path(dir: &Path, pane_id: &str) -> PathBuf {
    dir.join(record_name(pane_id))
}

/// Written beside the record and renamed into place, so a reader never sees
/// half a record.
fn save_in(dir: &Path, record: &Record) -> io::Result<()> {
    let path = record_path(dir, &record.notification.pane_id);
    let temp_path = dir.join(format!(
        "{}.{}.tmp",
        record_name(&record.notification.pane_id),
        std::process::id()
    ));
    fs::write(
        &temp_path,
        serde_json::to_vec(record).map_err(io::Error::other)?,
    )?;
    fs::rename(temp_path, path)
}

fn load_in(dir: &Path, pane_id: &str) -> Option<Record> {
    let content = fs::read(record_path(dir, pane_id)).ok()?;
    serde_json::from_slice(&content).ok()
}

fn resolve_in(dir: &Path, pane_id: &str, post_id: Option<&str>, status: Status) -> io::Result<()> {
    let Some(mut record) = load_in(dir, pane_id) else {
        return Ok(());
    };
    if record.status != Status::Pending || post_id.is_some_and(|id| id != record.post_id) {
        return Ok(());
    }
    record.status = status;
    save_in(dir, &record)
}

fn pending_in(dir: &Path) -> Vec<Record> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&format!("{}.record.", namespace())) && name.ends_with(".json")
        })
        .filter_map(|entry| serde_json::from_slice::<Record>(&fs::read(entry.path()).ok()?).ok())
        .filter(|record| record.status == Status::Pending)
        .collect()
}

fn claim_repost_in(dir: &Path, record: &Record) -> Option<Record> {
    // One claim per posting, created atomically: of two clicks that both
    // found this posting closed, only the first re-posts it.
    let claim = dir.join(format!(
        "{}.claim.{}.{}",
        namespace(),
        sanitize_group_id(&record.notification.pane_id),
        record.post_id
    ));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(claim)
        .ok()?;

    let current = load_in(dir, &record.notification.pane_id)?;
    if current.post_id != record.post_id || current.status != Status::Pending {
        return None;
    }
    let now = now_ms();
    let reposted = Record {
        post_id: post_id(now),
        posted_at_ms: now,
        ..current
    };
    save_in(dir, &reposted).ok()?;
    Some(reposted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::util::prefixed;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("dagadbm-ledger-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn record(pane_id: &str) -> Record {
        Record::new(
            FocusNotification {
                pane_id: pane_id.to_string(),
                status: "blocked".to_string(),
                title: "⌂ repo ▸ status".to_string(),
                body: "× Tidy up".to_string(),
                subtitle: Some("Codex".to_string()),
                group: crate::notify::util::notification_group_id(pane_id),
                app_icon: None,
            },
            30,
            false,
        )
    }

    #[test]
    fn a_saved_record_loads_back_unchanged() {
        let dir = TempDir::new("round-trip");
        let sent = record("w1:p2");

        save_in(&dir.0, &sent).unwrap();

        assert_eq!(load_in(&dir.0, "w1:p2"), Some(sent));
        assert!(dir
            .0
            .join("herdr-dagadbm-notify.repost.record.w1-p2.json")
            .exists());
        assert_eq!(load_in(&dir.0, "w1:p3"), None);
    }

    #[test]
    fn a_status_changes_only_for_the_current_pending_posting() {
        let dir = TempDir::new("resolve");
        let sent = record("w1:p2");
        save_in(&dir.0, &sent).unwrap();

        // A script that outlived its posting changes nothing.
        resolve_in(&dir.0, "w1:p2", Some("stale"), Status::Dismissed).unwrap();
        assert_eq!(load_in(&dir.0, "w1:p2").unwrap().status, Status::Pending);

        resolve_in(&dir.0, "w1:p2", Some(&sent.post_id), Status::Done).unwrap();
        assert_eq!(load_in(&dir.0, "w1:p2").unwrap().status, Status::Done);

        // A resolved record stays resolved.
        resolve_in(&dir.0, "w1:p2", None, Status::Dismissed).unwrap();
        assert_eq!(load_in(&dir.0, "w1:p2").unwrap().status, Status::Done);
    }

    #[test]
    fn only_pending_records_are_listed() {
        let dir = TempDir::new("pending");
        save_in(&dir.0, &record("w1:p1")).unwrap();
        save_in(&dir.0, &record("w1:p2")).unwrap();
        resolve_in(&dir.0, "w1:p2", None, Status::Dismissed).unwrap();
        fs::write(
            dir.0.join("herdr-dagadbm-notify.repost.record.w1-p3.json"),
            "not json",
        )
        .unwrap();

        let pending = pending_in(&dir.0);

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].notification.pane_id, "w1:p1");
    }

    #[test]
    fn a_posting_is_reposted_once() {
        let dir = TempDir::new("claim");
        let sent = record("w1:p2");
        save_in(&dir.0, &sent).unwrap();

        let reposted = claim_repost_in(&dir.0, &sent).unwrap();

        assert_ne!(reposted.post_id, sent.post_id);
        assert_eq!(reposted.notification, sent.notification);
        assert_eq!(reposted.sent_at_ms, sent.sent_at_ms);
        assert_eq!(load_in(&dir.0, "w1:p2"), Some(reposted.clone()));
        // A second click that found the same posting closed loses the claim.
        assert_eq!(claim_repost_in(&dir.0, &sent), None);
        // So does a click holding a record that has since been resolved.
        resolve_in(&dir.0, "w1:p2", None, Status::Dismissed).unwrap();
        assert_eq!(claim_repost_in(&dir.0, &reposted), None);
    }

    #[test]
    fn a_claim_taken_by_another_click_wins_even_on_a_pending_posting() {
        let dir = TempDir::new("claim-only");
        let sent = record("w1:p2");
        save_in(&dir.0, &sent).unwrap();

        // Another click holds the claim but has not saved its re-post yet:
        // the record is still pending on the same posting, so only the claim
        // stops this one.
        let claim = dir.0.join(format!(
            "herdr-dagadbm-notify.repost.claim.w1-p2.{}",
            sent.post_id
        ));
        fs::write(&claim, "").unwrap();
        assert_eq!(claim_repost_in(&dir.0, &sent), None);
        assert_eq!(load_in(&dir.0, "w1:p2"), Some(sent.clone()));

        fs::remove_file(&claim).unwrap();
        assert!(claim_repost_in(&dir.0, &sent).is_some());
    }

    #[test]
    fn done_and_dismissed_records_are_not_pending() {
        let dir = TempDir::new("pending-statuses");
        for (pane, status) in [
            ("w1:p1", Status::Done),
            ("w1:p2", Status::Dismissed),
            ("w1:p3", Status::Pending),
        ] {
            save_in(
                &dir.0,
                &Record {
                    status,
                    ..record(pane)
                },
            )
            .unwrap();
        }

        let pending: Vec<_> = pending_in(&dir.0)
            .into_iter()
            .map(|record| record.notification.pane_id)
            .collect();

        assert_eq!(pending, ["w1:p3"]);
    }

    #[test]
    fn ledger_files_are_recognised_for_cleanup() {
        assert!(is_ledger_file(
            "herdr-dagadbm-notify.repost.record.w1-p2.json"
        ));
        assert!(is_ledger_file(
            "herdr-dagadbm-notify.repost.claim.w1-p2.123-45"
        ));
        assert!(is_ledger_file("herdr-dagadbm-notify.repost.click"));
        assert!(!is_ledger_file(&prefixed("terminal-memory.json")));
    }
}
