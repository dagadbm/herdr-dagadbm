//! Marks the tab label and the agent name of an agent that needs the user: the configured icon is put
//! in front of the tab's label and of the pane's display agent name, and taken off again when the agent moves on
//! or the user focuses the pane. The marker is the only state: a label that
//! starts with the icon is marked, so nothing is stored, and a tab the user
//! renamed in the meantime is left alone.

use std::time::{Duration, Instant};

use crate::notify::config::NotifyConfig;
use crate::notify::event::StatusEvent;
use crate::notify::focus::{pane_details, tab_label, PaneDetails};
use crate::notify::util::command_stdout_until;

/// Time the Herdr calls behind one marker change get, together.
const MARKER_BUDGET: Duration = Duration::from_secs(2);

/// The `--source` the pane's display agent is reported under.
const SOURCE: &str = "dagadbm";

/// Marks or unmarks the tab label and the agent name of the pane an agent
/// status event is about; `details` is the `agent get` the caller already
/// made. Best-effort: a Herdr that does not answer in time leaves them as they
/// were.
///
/// Returns the tab's label as the user named it, read before the marker
/// changes it and without a marker it already had, so a notification names
/// the tab rather than the marker. None when the tab marker is off or the tab
/// was not read.
pub(crate) fn sync_for_status(
    config: &NotifyConfig,
    event: &StatusEvent,
    details: &PaneDetails,
    herdr_bin: &str,
    deadline: Instant,
) -> Option<String> {
    // A focused pane is already being looked at, so it needs no marker.
    let looked_at = details.focused;

    let tab = &config.tab_marker;
    let mut original_label = None;
    if let (true, Some(tab_id)) = (tab.enabled, details.tab_id()) {
        if let Some(label) = tab_label(tab_id, herdr_bin, deadline) {
            let unmarked = unmarked_label(&label, &tab.icon).map(str::to_string);
            if tab.marks(&event.status) && !looked_at {
                if let Some(marked) = marked_label(&label, &tab.icon) {
                    rename(tab_id, &marked, herdr_bin, deadline);
                }
            } else if let Some(original) = &unmarked {
                rename(tab_id, original, herdr_bin, deadline);
            }
            original_label = Some(unmarked.unwrap_or(label));
        }
    }

    let agent_marker = &config.agent_marker;
    if !agent_marker.enabled {
        return original_label;
    }
    let agent = event.agent.as_deref().unwrap_or_default();
    if agent_marker.marks(&event.status) && !looked_at {
        if let Some(marked) = marked_label(agent, &agent_marker.icon).filter(|_| !agent.is_empty())
        {
            report_display_agent(
                &event.pane_id,
                &["--display-agent", &marked],
                herdr_bin,
                deadline,
            );
        }
    } else if unmarked_label(agent, &agent_marker.icon).is_some() {
        report_display_agent(
            &event.pane_id,
            &["--clear-display-agent"],
            herdr_bin,
            deadline,
        );
    }
    original_label
}

/// Unmarks the tab label and the agent name of a pane the user just focused.
pub(crate) fn clear_for_focus(config: &NotifyConfig, pane_id: &str, herdr_bin: &str) {
    let deadline = Instant::now() + MARKER_BUDGET;
    if config.tab_marker.enabled {
        if let Some(tab_id) = pane_details(pane_id, herdr_bin, deadline)
            .and_then(|details| details.tab_id().map(str::to_string))
        {
            clear(&tab_id, &config.tab_marker.icon, herdr_bin, deadline);
        }
    }
    if config.agent_marker.enabled {
        report_display_agent(pane_id, &["--clear-display-agent"], herdr_bin, deadline);
    }
}

/// Herdr's CLI wants the pane ID before the options.
fn report_display_agent(pane_id: &str, option: &[&str], herdr_bin: &str, deadline: Instant) {
    let mut args = vec!["pane", "report-metadata", pane_id, "--source", SOURCE];
    args.extend_from_slice(option);
    let _ = command_stdout_until(herdr_bin, &args, &[], deadline);
}

fn clear(tab_id: &str, icon: &str, herdr_bin: &str, deadline: Instant) {
    let Some(label) = tab_label(tab_id, herdr_bin, deadline) else {
        return;
    };
    if let Some(original) = unmarked_label(&label, icon) {
        rename(tab_id, original, herdr_bin, deadline);
    }
}

fn rename(tab_id: &str, label: &str, herdr_bin: &str, deadline: Instant) {
    let _ = command_stdout_until(herdr_bin, &["tab", "rename", tab_id, label], &[], deadline);
}

/// `label` with the icon in front; None when it already has it.
fn marked_label(label: &str, icon: &str) -> Option<String> {
    unmarked_label(label, icon)
        .is_none()
        .then(|| format!("{icon} {label}"))
}

/// `label` without the icon in front; None when it does not have it.
fn unmarked_label<'a>(label: &'a str, icon: &str) -> Option<&'a str> {
    label
        .strip_prefix(icon)
        .and_then(|rest| rest.strip_prefix(' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_the_icon_in_front_once() {
        assert_eq!(
            marked_label("mobile zsh", "⚠").as_deref(),
            Some("⚠ mobile zsh")
        );
        assert_eq!(marked_label("⚠ mobile zsh", "⚠"), None);
    }

    #[test]
    fn takes_the_icon_off_only_when_it_is_there() {
        assert_eq!(unmarked_label("⚠ mobile zsh", "⚠"), Some("mobile zsh"));
        assert_eq!(unmarked_label("mobile zsh", "⚠"), None);
        assert_eq!(unmarked_label("⚠mobile", "⚠"), None);
    }

    fn round_trip(label: &str, icon: &str) -> String {
        let marked = marked_label(label, icon).unwrap();
        unmarked_label(&marked, icon).unwrap().to_string()
    }

    #[test]
    fn unmarking_a_marked_label_gives_back_the_exact_label() {
        for label in [
            "mobile zsh",
            "  two  spaces ",
            "build ⚠ later",
            "7",
            "ünïcödé 🚀",
        ] {
            assert_eq!(round_trip(label, "⚠"), label);
            assert_eq!(round_trip(label, "🔥"), label);
            assert_eq!(round_trip(label, "●●"), label);
        }
    }

    #[test]
    fn marking_a_marked_label_twice_does_not_double_the_icon() {
        let once = marked_label("mobile", "🔥").unwrap();
        assert_eq!(once, "🔥 mobile");
        assert_eq!(marked_label(&once, "🔥"), None);
    }

    #[test]
    fn a_label_the_user_renamed_is_not_unmarked() {
        assert_eq!(unmarked_label("renamed", "⚠"), None);
        assert_eq!(unmarked_label("renamed ⚠ tab", "⚠"), None);
        assert_eq!(unmarked_label(" ⚠ tab", "⚠"), None);
    }

    /// A fake `herdr`: logs each call's arguments, `|`-separated, one call per
    /// line; answers `tab get` with the label in `label` and `agent get` with a
    /// focused pane in tab `w1:t1`; `tab rename` updates `label`.
    struct StubHerdr {
        dir: std::path::PathBuf,
    }

    impl StubHerdr {
        fn new(name: &str, label: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("dagadbm-tab-marker-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("label"), label).unwrap();
            let script = format!(
                r#"#!/bin/sh
dir='{dir}'
printf '%s|' "$@" >> "$dir/log"
echo >> "$dir/log"
case "$1 $2" in
  "tab get") printf '{{"result":{{"tab":{{"label":"%s","number":1}}}}}}' "$(cat "$dir/label")" ;;
  "tab rename") printf '%s' "$4" > "$dir/label" ;;
  "agent get") printf '{{"result":{{"agent":{{"focused":true,"pane_id":"%s","tab_id":"w1:t1"}}}}}}' "$3" ;;
esac
"#,
                dir = dir.display()
            );
            let bin = dir.join("herdr");
            std::fs::write(&bin, script).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { dir }
        }

        fn bin(&self) -> String {
            self.dir.join("herdr").to_str().unwrap().to_string()
        }

        fn label(&self) -> String {
            std::fs::read_to_string(self.dir.join("label")).unwrap()
        }

        /// The logged calls, and forgets them.
        fn take_calls(&self) -> Vec<String> {
            let log = self.dir.join("log");
            let calls = std::fs::read_to_string(&log).unwrap_or_default();
            let _ = std::fs::remove_file(&log);
            calls
                .lines()
                .map(|line| line.trim_end_matches('|').to_string())
                .collect()
        }
    }

    impl Drop for StubHerdr {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    fn event(status: &str, agent: &str) -> StatusEvent {
        StatusEvent {
            pane_id: "w1:p2".to_string(),
            status: status.to_string(),
            agent: (!agent.is_empty()).then(|| agent.to_string()),
            agent_id: None,
            terminal_title: None,
            state_label: None,
        }
    }

    fn unfocused_in_tab() -> PaneDetails {
        PaneDetails::in_tab(Some("w1:t1"), false)
    }

    #[test]
    fn a_blocked_agent_marks_its_tab_and_agent_name() {
        let herdr = StubHerdr::new("blocked", "mobile zsh");
        let config = NotifyConfig::default();

        let original = sync_for_status(
            &config,
            &event("blocked", "claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(original.as_deref(), Some("mobile zsh"));

        assert_eq!(
            herdr.take_calls(),
            [
                "tab|get|w1:t1",
                "tab|rename|w1:t1|⚠ mobile zsh",
                "pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|⚠ claude",
            ]
        );
        assert_eq!(herdr.label(), "⚠ mobile zsh");
    }

    #[test]
    fn an_agent_back_at_work_gets_its_original_tab_label_and_agent_name_back() {
        let herdr = StubHerdr::new("working", "⚠ mobile zsh");

        sync_for_status(
            &NotifyConfig::default(),
            &event("working", "⚠ claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            [
                "tab|get|w1:t1",
                "tab|rename|w1:t1|mobile zsh",
                "pane|report-metadata|w1:p2|--source|dagadbm|--clear-display-agent",
            ]
        );
        assert_eq!(herdr.label(), "mobile zsh");
    }

    #[test]
    fn marking_then_moving_on_restores_the_exact_label() {
        let herdr = StubHerdr::new("round-trip", "  my tab ⚠ x");
        let config = NotifyConfig::default();
        let details = unfocused_in_tab();

        sync_for_status(
            &config,
            &event("done", "codex"),
            &details,
            &herdr.bin(),
            deadline(),
        );
        assert_eq!(herdr.label(), "⚠   my tab ⚠ x");
        sync_for_status(
            &config,
            &event("working", "⚠ codex"),
            &details,
            &herdr.bin(),
            deadline(),
        );
        assert_eq!(herdr.label(), "  my tab ⚠ x");
    }

    #[test]
    fn focusing_the_pane_clears_both_markers() {
        let herdr = StubHerdr::new("focus", "⚠ 3");

        clear_for_focus(&NotifyConfig::default(), "w1:p2", &herdr.bin());

        assert_eq!(
            herdr.take_calls(),
            [
                "agent|get|w1:p2",
                "tab|get|w1:t1",
                "tab|rename|w1:t1|3",
                "pane|report-metadata|w1:p2|--source|dagadbm|--clear-display-agent",
            ]
        );
        assert_eq!(herdr.label(), "3");
    }

    #[test]
    fn focusing_with_both_markers_off_calls_nothing() {
        let herdr = StubHerdr::new("focus-off", "⚠ 3");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;
        config.agent_marker.enabled = false;

        clear_for_focus(&config, "w1:p2", &herdr.bin());

        assert!(herdr.take_calls().is_empty());
    }

    #[test]
    fn a_disabled_tab_marker_leaves_the_tab_alone() {
        let herdr = StubHerdr::new("tab-off", "mobile");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;

        sync_for_status(
            &config,
            &event("blocked", "claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            ["pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|⚠ claude"]
        );
    }

    #[test]
    fn a_disabled_agent_marker_leaves_the_agent_name_alone() {
        let herdr = StubHerdr::new("agent-off", "mobile");
        let mut config = NotifyConfig::default();
        config.agent_marker.enabled = false;

        sync_for_status(
            &config,
            &event("blocked", "claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            ["tab|get|w1:t1", "tab|rename|w1:t1|⚠ mobile"]
        );
    }

    #[test]
    fn a_focused_pane_is_not_marked() {
        let herdr = StubHerdr::new("focused", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &event("blocked", "claude"),
            &PaneDetails::in_tab(Some("w1:t1"), true),
            &herdr.bin(),
            deadline(),
        );

        // The tab label is read to see whether it needs clearing, nothing more.
        assert_eq!(herdr.take_calls(), ["tab|get|w1:t1"]);
        assert_eq!(herdr.label(), "mobile");
    }

    #[test]
    fn a_status_that_is_not_marked_changes_nothing_unmarked() {
        let herdr = StubHerdr::new("unlisted", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &event("working", "claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(herdr.take_calls(), ["tab|get|w1:t1"]);
    }

    #[test]
    fn a_pane_without_a_tab_only_marks_the_agent() {
        let herdr = StubHerdr::new("no-tab", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &event("blocked", "claude"),
            &PaneDetails::in_tab(None, false),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            ["pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|⚠ claude"]
        );
    }

    #[test]
    fn an_already_marked_tab_and_agent_are_not_marked_again() {
        let herdr = StubHerdr::new("already", "⚠ mobile");

        let original = sync_for_status(
            &NotifyConfig::default(),
            &event("blocked", "⚠ claude"),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        // The notification still names the tab without the marker.
        assert_eq!(original.as_deref(), Some("mobile"));
        assert_eq!(herdr.take_calls(), ["tab|get|w1:t1"]);
        assert_eq!(herdr.label(), "⚠ mobile");
    }

    #[test]
    fn an_event_without_an_agent_reports_no_agent_name() {
        let herdr = StubHerdr::new("no-agent", "mobile");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;

        sync_for_status(
            &config,
            &event("blocked", ""),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );
        sync_for_status(
            &config,
            &event("working", ""),
            &unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert!(herdr.take_calls().is_empty());
    }
}
