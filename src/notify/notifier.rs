use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::notify::executable::{find_executable, home_dir};
use crate::notify::util::{is_notification_group_id, notification_group_id, shell_quote};

pub(crate) fn resolve_notifier_bin() -> Result<String, String> {
    find_executable("alerter", alerter_candidate_paths()).ok_or_else(|| {
        "no alerter notifier found; install alerter with `brew install vjeantet/tap/alerter`"
            .to_string()
    })
}

pub(crate) fn send_notification(script_path: &Path, foreground: bool) -> io::Result<()> {
    if foreground {
        run_script_foreground(script_path)
    } else {
        spawn_detached_script(script_path)
    }
}

pub(crate) fn remove_notification(pane_id: &str, notifier_bin: &str) -> io::Result<()> {
    let group = notification_group_id(pane_id);

    match Command::new(notifier_bin)
        .arg("--remove")
        .arg(group)
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(io::Error::other(format!(
            "notification removal exited with {status}"
        ))),
        Err(err) => Err(err),
    }
}

/// The alerter groups of this plugin's notifications that are still shown,
/// from `alerter --list ALL`. alerter prints nothing when none is shown. None
/// when alerter cannot be run or the list fails, since nothing is known then.
pub(crate) fn shown_groups(notifier_bin: &str) -> Option<Vec<String>> {
    let output = Command::new(notifier_bin)
        .args(["--list", "ALL"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(groups_from_list_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// The `herdr-` groups in alerter's list: the `groupID` fields of its JSON,
/// or, should the list not be JSON, every word shaped like one of our groups.
fn groups_from_list_output(output: &str) -> Vec<String> {
    let mut groups = Vec::new();
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(list) => collect_group_ids(&list, &mut groups),
        Err(_) => groups.extend(
            output
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
                .filter(|word| is_notification_group_id(word))
                .map(str::to_string),
        ),
    }
    groups
}

fn collect_group_ids(value: &serde_json::Value, groups: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                match value.as_str() {
                    Some(group)
                        if key.eq_ignore_ascii_case("groupid")
                            && is_notification_group_id(group) =>
                    {
                        groups.push(group.to_string());
                    }
                    _ => collect_group_ids(value, groups),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_group_ids(item, groups);
            }
        }
        _ => {}
    }
}

fn alerter_candidate_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/alerter"),
        PathBuf::from("/usr/local/bin/alerter"),
    ];
    if let Some(home) = home_dir() {
        paths.push(home.join(".local/bin/alerter"));
    }
    paths
}

fn run_script_foreground(script_path: &Path) -> io::Result<()> {
    match Command::new("sh").arg(script_path).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(io::Error::other(format!(
            "notification script exited with {status}"
        ))),
        Err(err) => Err(err),
    }
}

fn spawn_detached_script(script_path: &Path) -> io::Result<()> {
    let script_str = script_path.to_string_lossy();
    let cmd = format!(
        "nohup sh {} >/dev/null 2>&1 &",
        shell_quote(script_str.as_ref())
    );

    Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::util::prefixed;

    #[test]
    fn alerter_candidates_include_common_homebrew_paths() {
        let paths = alerter_candidate_paths();

        assert!(paths.contains(&PathBuf::from("/opt/homebrew/bin/alerter")));
        assert!(paths.contains(&PathBuf::from("/usr/local/bin/alerter")));
    }

    #[test]
    fn reads_our_groups_from_the_alerter_list() {
        let (p2, p1) = (prefixed("w1-p2"), prefixed("w2-p1"));
        let json = format!(
            r#"[
            {{"groupID": "{p2}", "title": "{} in a title", "deliveredAt": "now"}},
            {{"groupID": "someone-else", "title": "t"}},
            {{"groupID": "herdr-w3-p1"}},
            {{"groupID": "{p1}"}}
        ]"#,
            prefixed("w9-p9")
        );
        assert_eq!(groups_from_list_output(&json), [p2.clone(), p1]);
        // Nothing shown: alerter prints nothing at all.
        assert!(groups_from_list_output("").is_empty());
        // A list that is not JSON still names the groups.
        assert_eq!(
            groups_from_list_output(&format!("GroupID\tTitle\n{p2}\tsome title\n")),
            [p2]
        );
    }
}
