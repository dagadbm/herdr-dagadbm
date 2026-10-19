use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::notify::util::{notification_group_id, prefixed, PREFIX};

const GENERATED_SCRIPT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const FOCUS_ORIGIN_RETENTION: Duration = Duration::from_secs(15);

pub(crate) fn plugin_state_dir() -> PathBuf {
    env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join(PREFIX))
}

/// The most recently used terminal (learned from `pane.focused` events) is
/// persisted here so a click can activate it and skip checks can match it,
/// with no configuration required.
pub(crate) fn remember_terminal(workspace: &str, bundle_id: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    remember_terminal_into(&state_dir, workspace, bundle_id)
}

/// The terminal bound to a pane's workspace, learned from `pane.focused`
/// events. `None` until the workspace has been focused at least once or when
/// the saved bundle id is an obvious non-terminal app.
pub(crate) fn remembered_terminal(workspace: &str) -> Option<String> {
    read_terminal_bindings_from(&terminal_memory_path())?
        .get(workspace)
        .filter(|bundle_id| !is_obvious_non_terminal_bundle(bundle_id))
        .cloned()
}

/// Clears all saved workspace-to-terminal bindings.
pub(crate) fn clear_terminal_bindings() -> io::Result<()> {
    let binding_result = match fs::remove_file(terminal_memory_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    };
    binding_result?;
    clear_focus_origin_markers()
}

/// Marks a focus operation as originating from a notification click. The
/// marker is intentionally short-lived because Herdr delivers the resulting
/// focus event in a separate plugin process.
pub(crate) fn mark_focus_origin(workspace: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    fs::write(focus_origin_marker_path(workspace), [])
}

/// Returns whether a recent notification-originated focus operation is still
/// suppressing terminal learning for this workspace.
pub(crate) fn focus_origin_is_active(workspace: &str) -> bool {
    let path = focus_origin_marker_path(workspace);
    let active = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < FOCUS_ORIGIN_RETENTION);

    if !active {
        let _ = fs::remove_file(path);
    }
    active
}

pub(crate) fn clear_focus_origin(workspace: &str) -> io::Result<()> {
    match fs::remove_file(focus_origin_marker_path(workspace)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(crate) fn focus_origin_marker_path(workspace: &str) -> PathBuf {
    plugin_state_dir().join(prefixed(&format!(
        "focus-origin-{}.marker",
        crate::notify::util::sanitize_group_id(workspace)
    )))
}

fn is_focus_origin_marker(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .is_some_and(|rest| rest.starts_with("-focus-origin-"))
        && name.ends_with(".marker")
}

/// The entries of `dir`, or none when it does not exist yet.
fn read_dir_or_empty(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries.collect(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

fn clear_focus_origin_markers() -> io::Result<()> {
    for entry in read_dir_or_empty(&plugin_state_dir())? {
        let path = entry.path();
        let is_marker = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_focus_origin_marker);
        if is_marker && path.is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Removes bindings for workspaces that no longer exist in Herdr.
///
/// Uses `herdr pane list` to learn the live workspace set; when that cannot
/// be trusted (command failure or no panes at all) nothing is pruned, so the
/// memory file never loses bindings against an empty world. Best-effort: a
/// stale binding is harmless, it just gets re-learned on the next focus.
/// The rewrite reuses the same atomic write path as learning.
pub(crate) fn prune_stale_workspace_bindings(herdr_bin: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    let path = terminal_memory_path();
    let Some(mut bindings) = read_terminal_bindings_from(&path) else {
        return Ok(());
    };
    let Some(live) = crate::notify::focus::live_workspace_ids(herdr_bin) else {
        return Ok(());
    };

    let before = bindings.len();
    bindings.retain(|workspace, _| live.iter().any(|id| id == workspace));
    if bindings.len() == before {
        return Ok(());
    }

    fs::create_dir_all(&state_dir)?;
    remember_bindings_into(&state_dir, &bindings)
}

fn remember_bindings_into(
    dir: &Path,
    bindings: &std::collections::HashMap<String, String>,
) -> io::Result<()> {
    let path = dir.join(terminal_memory_name());
    let temp_path = dir.join(prefixed(&format!(
        "terminal-memory-{}.tmp",
        std::process::id()
    )));
    let json = serde_json::json!({ "workspaces": bindings });
    fs::write(
        &temp_path,
        serde_json::to_vec(&json).map_err(io::Error::other)?,
    )?;
    fs::rename(temp_path, &path)
}

fn remember_terminal_into(dir: &Path, workspace: &str, bundle_id: &str) -> io::Result<()> {
    let path = dir.join(terminal_memory_name());

    // Read existing bindings so other workspaces survive the rewrite.
    let mut bindings = read_terminal_bindings_from(&path).unwrap_or_default();
    bindings.insert(workspace.to_string(), bundle_id.to_string());

    remember_bindings_into(dir, &bindings)
}

/// Reads the per-workspace bindings map (`{ "workspaces": { <id>: <bundle> } }`).
fn read_terminal_bindings_from(path: &Path) -> Option<std::collections::HashMap<String, String>> {
    let content = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let workspaces = value.get("workspaces")?.as_object()?;
    let mut bindings = std::collections::HashMap::new();
    for (workspace, id) in workspaces {
        if let Some(id) = id.as_str() {
            bindings.insert(workspace.clone(), id.to_string());
        }
    }
    if bindings.is_empty() {
        return None;
    }
    Some(bindings)
}

fn terminal_memory_path() -> PathBuf {
    plugin_state_dir().join(terminal_memory_name())
}

fn terminal_memory_name() -> String {
    prefixed("terminal-memory.json")
}

pub(crate) fn is_obvious_non_terminal_bundle(bundle_id: &str) -> bool {
    matches!(
        bundle_id,
        "com.google.Chrome"
            | "com.apple.Safari"
            | "org.mozilla.firefox"
            | "com.microsoft.edgemac"
            | "company.thebrowser.Browser"
            | "com.brave.Browser"
            | "com.apple.finder"
            | "com.apple.notificationcenterui"
    )
}

pub(crate) fn mark_notification_cleared(pane_id: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    fs::write(cleared_notification_marker_path(pane_id), [])
}

pub(crate) fn reset_notification_clearance(pane_id: &str) -> io::Result<()> {
    match fs::remove_file(cleared_notification_marker_path(pane_id)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(crate) fn cleared_notification_marker_path(pane_id: &str) -> PathBuf {
    plugin_state_dir().join(format!("{}.cleared", notification_group_id(pane_id)))
}

/// Records `status` as the last agent status seen for the pane and returns
/// whether it differs from the one recorded before. Herdr repeats
/// `pane.agent_status_changed` with an unchanged status, for example when the
/// agent marker changes the pane's display agent, and a repeat must not send
/// a second notification. Records before anything else writes to Herdr, so the
/// event a write echoes always finds this status in place.
pub(crate) fn record_status(pane_id: &str, status: &str) -> io::Result<bool> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    record_status_at(&last_status_path(pane_id), status)
}

fn record_status_at(path: &Path, status: &str) -> io::Result<bool> {
    if fs::read_to_string(path).is_ok_and(|previous| previous == status) {
        return Ok(false);
    }
    fs::write(path, status)?;
    Ok(true)
}

fn last_status_path(pane_id: &str) -> PathBuf {
    plugin_state_dir().join(format!("{}.last-status", notification_group_id(pane_id)))
}

pub(crate) fn cleanup_stale_state_files() -> io::Result<()> {
    let _ = remove_legacy_state_in(&plugin_state_dir());
    cleanup_stale_state_files_in(
        &plugin_state_dir(),
        SystemTime::now(),
        GENERATED_SCRIPT_RETENTION,
        TEMP_FILE_RETENTION,
        FOCUS_ORIGIN_RETENTION,
    )
}

/// The retention period for a generated state file, or None when the file is
/// not managed by cleanup. Cleared markers only gate the gap between a new
/// notification being queued and its script starting (seconds); every send
/// resets the marker first, so any marker old enough to be stale here is
/// inert. Ledger records, their re-post claims and the click marker are kept
/// a day, like the last statuses.
fn retention_for(
    name: &str,
    script_retention: Duration,
    temp_file_retention: Duration,
    focus_origin_retention: Duration,
) -> Option<Duration> {
    if crate::notify::script::is_generated_script(name) {
        Some(script_retention)
    } else if is_focus_origin_marker(name) {
        Some(focus_origin_retention)
    } else if crate::notify::ledger::is_ledger_file(name)
        || (name.starts_with(PREFIX) && is_transient_state_file(name))
    {
        Some(temp_file_retention)
    } else {
        None
    }
}

fn is_transient_state_file(name: &str) -> bool {
    name.contains(".result.")
        || name.contains(".status.")
        || name.ends_with(".cleared")
        || name.ends_with(".last-status")
}

/// Whether `name` is a state file from before every name started with the
/// sub-plugin name (ADR 13): `herdr-<pane>.*`, `focus-<hash>.sh`,
/// `.focus-<hash>.<pid>.tmp`, `focus-origin-<ws>.marker` and the terminal
/// memory with its temp file.
fn is_legacy_state_file(name: &str) -> bool {
    is_legacy_pane_file(name)
        || is_legacy_ledger_temp(name)
        || is_legacy_focus_script(name)
        || is_legacy_focus_temp(name)
        || (name.starts_with("focus-origin-") && name.ends_with(".marker"))
        || is_legacy_memory_temp(name)
}

fn is_hex_hash(value: &str) -> bool {
    value.len() == 16 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

fn is_legacy_pane_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("herdr-") else {
        return false;
    };
    !name.starts_with(PREFIX)
        && (rest.ends_with(".cleared")
            || rest.ends_with(".last-status")
            || rest.ends_with(".notification.json")
            || rest.ends_with(".repost")
            || rest.contains(".result.")
            || rest.contains(".status."))
}

/// The ledger's temp record of an old group.
fn is_legacy_ledger_temp(name: &str) -> bool {
    name.strip_prefix(".herdr-").is_some_and(|rest| {
        !rest.starts_with(&PREFIX["herdr-".len()..])
            && rest.contains(".notification.json.")
            && rest.ends_with(".tmp")
    })
}

fn is_legacy_focus_script(name: &str) -> bool {
    name.strip_prefix("focus-")
        .and_then(|rest| rest.strip_suffix(".sh"))
        .is_some_and(is_hex_hash)
}

fn is_legacy_focus_temp(name: &str) -> bool {
    name.strip_prefix(".focus-")
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .and_then(|rest| rest.split_once('.'))
        .is_some_and(|(hash, pid)| is_hex_hash(hash) && is_digits(pid))
}

fn is_legacy_memory_temp(name: &str) -> bool {
    name.strip_prefix(".terminal-memory-")
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .is_some_and(is_digits)
}

/// Moves state from the names used before ADR 13 out of the way: the terminal
/// memory is renamed so bindings survive, the icons folder is removed (icons
/// are rewritten on demand) and every other old file is deleted.
fn remove_legacy_state_in(state_dir: &Path) -> io::Result<()> {
    migrate_legacy_memory(state_dir)?;
    let old_icons = state_dir.join("icons");
    if old_icons.is_dir() {
        fs::remove_dir_all(old_icons)?;
    }

    for entry in read_dir_or_empty(state_dir)? {
        let path = entry.path();
        let is_legacy = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_legacy_state_file);
        if is_legacy && path.is_file() {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

fn migrate_legacy_memory(state_dir: &Path) -> io::Result<()> {
    let old_memory = state_dir.join("terminal-memory.json");
    if !old_memory.is_file() {
        return Ok(());
    }
    let new_memory = state_dir.join(terminal_memory_name());
    if new_memory.exists() {
        fs::remove_file(&old_memory)
    } else {
        fs::rename(&old_memory, new_memory)
    }
}

fn cleanup_stale_state_files_in(
    state_dir: &Path,
    now: SystemTime,
    script_retention: Duration,
    temp_file_retention: Duration,
    focus_origin_retention: Duration,
) -> io::Result<()> {
    for entry in read_dir_or_empty(state_dir)? {
        let path = entry.path();
        let Some(retention) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| {
                retention_for(
                    name,
                    script_retention,
                    temp_file_retention,
                    focus_origin_retention,
                )
            })
        else {
            continue;
        };
        if path.is_file() && is_older_than(&entry, now, retention) {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
    }

    Ok(())
}

fn is_older_than(entry: &fs::DirEntry, now: SystemTime, retention: Duration) -> bool {
    entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= retention)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_status_is_not_a_change() {
        let dir = std::env::temp_dir().join(format!("dagadbm-status-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(prefixed("w1-p1.last-status"));

        assert!(record_status_at(&path, "blocked").unwrap());
        assert!(!record_status_at(&path, "blocked").unwrap());
        assert!(record_status_at(&path, "working").unwrap());
        assert!(record_status_at(&path, "blocked").unwrap());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_removes_only_generated_state_files() {
        let dir = std::env::temp_dir().join(format!("dagadbm-state-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let removed = [
            prefixed("focus-0123456789abcdef.sh"),
            prefixed("w1-p1.cleared.result.123456"),
            prefixed("w1-p1.cleared.status.123456"),
            prefixed("w1-p1.cleared"),
            prefixed("w1-p1.last-status"),
            prefixed("focus-origin-w1.marker"),
            "herdr-dagadbm-notify.repost.record.w1-p1.json".to_string(),
            "herdr-dagadbm-notify.repost.claim.w1-p1.1-2-3".to_string(),
            "herdr-dagadbm-notify.repost.click".to_string(),
        ];
        for name in &removed {
            fs::write(dir.join(name), "old").unwrap();
        }
        fs::write(dir.join("focus-click.log"), "keep").unwrap();
        fs::write(dir.join("other.cleared"), "keep").unwrap();

        cleanup_stale_state_files_in(
            &dir,
            SystemTime::now(),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        )
        .unwrap();

        for name in &removed {
            assert!(!dir.join(name).exists(), "{name}");
        }
        assert!(dir.join("focus-click.log").exists());
        assert!(dir.join("other.cleared").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_names_are_removed_and_the_terminal_memory_is_kept() {
        let dir = std::env::temp_dir().join(format!("dagadbm-legacy-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("icons")).unwrap();
        fs::write(dir.join("icons/claude.png"), "old").unwrap();
        let legacy = [
            "focus-0123456789abcdef.sh",
            ".focus-0123456789abcdef.42.tmp",
            "focus-origin-w1.marker",
            "herdr-w1-p1.cleared",
            "herdr-w1-p1.cleared.result.Ab12Cd",
            "herdr-w1-p1.cleared.status.Ab12Cd",
            "herdr-w1-p1.last-status",
            "herdr-w1-p1.notification.json",
            ".herdr-w1-p1.notification.json.42.tmp",
            "herdr-w1-p1.1-2-3.repost",
            ".terminal-memory-42.tmp",
        ];
        for name in legacy {
            fs::write(dir.join(name), "old").unwrap();
        }
        fs::write(
            dir.join("terminal-memory.json"),
            r#"{"workspaces":{"w1":"app.warp"}}"#,
        )
        .unwrap();
        let kept = [
            prefixed("w1-p1.cleared"),
            prefixed("w1-p1.last-status"),
            prefixed("focus-0123456789abcdef.sh"),
            "herdr-dagadbm-notify.repost.record.w1-p1.json.42.tmp".to_string(),
            "focus-click.log".to_string(),
            "focus-notes.sh".to_string(),
            "herdr-notes.txt".to_string(),
        ];
        for name in &kept {
            fs::write(dir.join(name), "new").unwrap();
        }

        remove_legacy_state_in(&dir).unwrap();

        for name in legacy {
            assert!(!dir.join(name).exists(), "{name}");
        }
        assert!(!dir.join("icons").exists());
        assert!(!dir.join("terminal-memory.json").exists());
        let bindings = read_terminal_bindings_from(&dir.join(terminal_memory_name())).unwrap();
        assert_eq!(bindings.get("w1").map(String::as_str), Some("app.warp"));
        for name in &kept {
            assert!(dir.join(name).exists(), "{name}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_per_workspace_bindings() {
        let dir =
            std::env::temp_dir().join(format!("dagadbm-terminal-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join(terminal_memory_name());

        // Two workspaces bind independently and one rewrite keeps the other.
        remember_terminal_into(&dir, "w1", "com.googlecode.iterm2").unwrap();
        remember_terminal_into(&dir, "w2", "net.kovidgoyal.kitty").unwrap();
        let bindings = read_terminal_bindings_from(&path).expect("bindings should exist");
        assert_eq!(
            bindings.get("w1").map(String::as_str),
            Some("com.googlecode.iterm2")
        );
        assert_eq!(
            bindings.get("w2").map(String::as_str),
            Some("net.kovidgoyal.kitty")
        );

        // Overwriting one workspace does not disturb the other.
        remember_terminal_into(&dir, "w1", "app.warp").unwrap();
        let bindings = read_terminal_bindings_from(&path).expect("bindings should exist");
        assert_eq!(bindings.get("w1").map(String::as_str), Some("app.warp"));
        assert_eq!(
            bindings.get("w2").map(String::as_str),
            Some("net.kovidgoyal.kitty")
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
