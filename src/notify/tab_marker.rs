//! Marks the tab label, the workspace label and the agent name of an agent that needs the user: the configured icon is put
//! in front of the tab's label, the workspace's label and the pane's display agent name, and taken off again when the agent moves on
//! or the user focuses the pane. A tab or workspace stays marked while any of
//! its agents needs the user: each such pane holds a token in Herdr, so nothing
//! is stored by the plugin. A label that starts with the icon is marked, and a
//! tab the user renamed in the meantime is left alone.

use std::time::{Duration, Instant};

use crate::notify::config::{MarkerConfig, NotifyConfig};
use crate::notify::event::StatusEvent;
use crate::notify::focus::{
    listed_panes, tab_label, workspace_label_by_id, ListedPane, PaneDetails,
};
use crate::notify::util::{command_stdout_until, workspace_id_from_pane_id};

/// Time the Herdr calls behind one marker change get, together.
const MARKER_BUDGET: Duration = Duration::from_secs(2);

/// The `--source` the pane's display agent is reported under.
const SOURCE: &str = "dagadbm";

/// The pane tokens that say its agent needs the user, one per label that
/// shows it. A tab or workspace stays marked while any of its panes holds one.
const TAB_TOKEN: &str = "marked_tab";
const WORKSPACE_TOKEN: &str = "marked_workspace";

/// The labels of a pane's tab and workspace as the user named them, read
/// before a marker changes them and without a marker they already had, so a
/// notification names them rather than the marker. None when that marker is
/// off or the label was not read.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct OriginalLabels {
    pub(crate) tab: Option<String>,
    pub(crate) workspace: Option<String>,
}

/// Marks or unmarks the tab label, the workspace label and the agent name of
/// the pane an agent status event is about; `details` is the `agent get` the
/// caller already made. Best-effort: a Herdr that does not answer in time
/// leaves them as they were.
pub(crate) fn sync_for_status(
    config: &NotifyConfig,
    event: &StatusEvent,
    details: &PaneDetails,
    herdr_bin: &str,
    deadline: Instant,
) -> OriginalLabels {
    // A focused pane is already being looked at, so it needs no marker.
    let looked_at = details.focused;

    let tab = &config.tab_marker;
    let workspace = &config.workspace_marker;
    let panes = if tab.enabled || workspace.enabled {
        listed_panes(herdr_bin, deadline).unwrap_or_default()
    } else {
        Vec::new()
    };
    let wants_tab =
        tab.enabled && details.tab_id().is_some() && tab.marks(&event.status) && !looked_at;
    let wants_workspace = workspace.enabled && workspace.marks(&event.status) && !looked_at;
    set_token(
        &panes,
        &event.pane_id,
        TAB_TOKEN,
        wants_tab,
        herdr_bin,
        deadline,
    );
    set_token(
        &panes,
        &event.pane_id,
        WORKSPACE_TOKEN,
        wants_workspace,
        herdr_bin,
        deadline,
    );

    let mut original = OriginalLabels::default();
    if let (true, Some(tab_id)) = (tab.enabled, details.tab_id()) {
        let mark = wants_tab
            || marked_elsewhere(&panes, &event.pane_id, TAB_TOKEN, |pane| {
                pane.tab_id.as_deref() == Some(tab_id)
            });
        original.tab = tab_label(tab_id, herdr_bin, deadline).map(|label| {
            sync_label(&label, tab, mark, |new| {
                rename(tab_id, new, herdr_bin, deadline)
            })
        });
    }
    if let (true, Some(workspace_id)) =
        (workspace.enabled, workspace_id_from_pane_id(&event.pane_id))
    {
        let mark = wants_workspace
            || marked_elsewhere(&panes, &event.pane_id, WORKSPACE_TOKEN, |pane| {
                workspace_id_from_pane_id(&pane.pane_id) == Some(workspace_id)
            });
        original.workspace =
            workspace_label_by_id(workspace_id, herdr_bin, deadline).map(|label| {
                sync_label(&label, workspace, mark, |new| {
                    rename_workspace(workspace_id, new, herdr_bin, deadline)
                })
            });
    }

    let agent_marker = &config.agent_marker;
    if !agent_marker.enabled {
        return original;
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
    original
}

/// Puts the marker on `label`, or takes it off, through `rename`, and returns
/// the label without a marker.
fn sync_label(label: &str, marker: &MarkerConfig, mark: bool, rename: impl FnOnce(&str)) -> String {
    let unmarked = unmarked_label(label, &marker.icon);
    if mark {
        if let Some(marked) = marked_label(label, &marker.icon) {
            rename(&marked);
        }
    } else if let Some(original) = unmarked {
        rename(original);
    }
    unmarked.unwrap_or(label).to_string()
}

/// Unmarks the agent name of a pane the user just focused, and the tab and
/// workspace labels unless another agent in them still needs the user.
pub(crate) fn clear_for_focus(config: &NotifyConfig, pane_id: &str, herdr_bin: &str) {
    let deadline = Instant::now() + MARKER_BUDGET;
    let tab = &config.tab_marker;
    let workspace = &config.workspace_marker;
    if tab.enabled || workspace.enabled {
        let panes = listed_panes(herdr_bin, deadline).unwrap_or_default();
        set_token(&panes, pane_id, TAB_TOKEN, false, herdr_bin, deadline);
        set_token(&panes, pane_id, WORKSPACE_TOKEN, false, herdr_bin, deadline);
        let tab_id = panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .and_then(|pane| pane.tab_id.as_deref());
        if let (true, Some(tab_id)) = (tab.enabled, tab_id) {
            let mark = marked_elsewhere(&panes, pane_id, TAB_TOKEN, |pane| {
                pane.tab_id.as_deref() == Some(tab_id)
            });
            if let Some(label) = tab_label(tab_id, herdr_bin, deadline) {
                sync_label(&label, tab, mark, |new| {
                    rename(tab_id, new, herdr_bin, deadline)
                });
            }
        }
        if let (true, Some(workspace_id)) = (workspace.enabled, workspace_id_from_pane_id(pane_id))
        {
            let mark = marked_elsewhere(&panes, pane_id, WORKSPACE_TOKEN, |pane| {
                workspace_id_from_pane_id(&pane.pane_id) == Some(workspace_id)
            });
            if let Some(label) = workspace_label_by_id(workspace_id, herdr_bin, deadline) {
                sync_label(&label, workspace, mark, |new| {
                    rename_workspace(workspace_id, new, herdr_bin, deadline)
                });
            }
        }
    }
    if config.agent_marker.enabled {
        report_display_agent(pane_id, &["--clear-display-agent"], herdr_bin, deadline);
    }
}

/// Whether a pane other than `pane_id` in the same group holds `token`: some
/// other agent there still needs the user.
fn marked_elsewhere(
    panes: &[ListedPane],
    pane_id: &str,
    token: &str,
    in_group: impl Fn(&ListedPane) -> bool,
) -> bool {
    panes
        .iter()
        .any(|pane| pane.pane_id != pane_id && in_group(pane) && pane.has_token(token))
}

/// Sets or clears `token` on the pane, when the listing says it differs. The
/// tokens are what tells a tab or workspace which of its agents still need the
/// user, so no state is stored outside Herdr.
fn set_token(
    panes: &[ListedPane],
    pane_id: &str,
    token: &str,
    on: bool,
    herdr_bin: &str,
    deadline: Instant,
) {
    let has = panes
        .iter()
        .any(|pane| pane.pane_id == pane_id && pane.has_token(token));
    if on == has {
        return;
    }
    if on {
        let value = format!("{token}=1");
        report_display_agent(pane_id, &["--token", &value], herdr_bin, deadline);
    } else {
        report_display_agent(pane_id, &["--clear-token", token], herdr_bin, deadline);
    }
}

/// Herdr's CLI wants the pane ID before the options.
fn report_display_agent(pane_id: &str, option: &[&str], herdr_bin: &str, deadline: Instant) {
    let mut args = vec!["pane", "report-metadata", pane_id, "--source", SOURCE];
    args.extend_from_slice(option);
    let _ = command_stdout_until(herdr_bin, &args, &[], deadline);
}

fn rename(tab_id: &str, label: &str, herdr_bin: &str, deadline: Instant) {
    let _ = command_stdout_until(herdr_bin, &["tab", "rename", tab_id, label], &[], deadline);
}

fn rename_workspace(workspace_id: &str, label: &str, herdr_bin: &str, deadline: Instant) {
    let _ = command_stdout_until(
        herdr_bin,
        &["workspace", "rename", workspace_id, label],
        &[],
        deadline,
    );
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
            marked_label("mobile zsh", "🟡").as_deref(),
            Some("🟡 mobile zsh")
        );
        assert_eq!(marked_label("🟡 mobile zsh", "🟡"), None);
    }

    #[test]
    fn takes_the_icon_off_only_when_it_is_there() {
        assert_eq!(unmarked_label("🟡 mobile zsh", "🟡"), Some("mobile zsh"));
        assert_eq!(unmarked_label("mobile zsh", "🟡"), None);
        assert_eq!(unmarked_label("🟡mobile", "🟡"), None);
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
            "build 🟡 later",
            "7",
            "ünïcödé 🚀",
        ] {
            assert_eq!(round_trip(label, "🟡"), label);
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
        assert_eq!(unmarked_label("renamed", "🟡"), None);
        assert_eq!(unmarked_label("renamed 🟡 tab", "🟡"), None);
        assert_eq!(unmarked_label(" 🟡 tab", "🟡"), None);
    }

    /// A workspace on the test Herdr server whose tab has `label`, with an
    /// agent in its pane, reached through a `herdr` that records its calls.
    /// The calls name the tab `w1:t1` and the pane `w1:p2`, whatever their
    /// real IDs, so the expectations read the same in every run.
    struct TestHerdr {
        dir: std::path::PathBuf,
        workspace: crate::test_herdr::Workspace,
    }

    impl TestHerdr {
        fn new(name: &str, label: &str) -> Self {
            Self::with_workspace(name, label, "dotfiles")
        }

        fn with_workspace(name: &str, label: &str, workspace_label: &str) -> Self {
            let server = crate::test_herdr::HerdrServer::shared();
            let dir = std::env::temp_dir()
                .join(format!("dagadbm-tab-marker-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let workspace = server.workspace();
            server.rename_tab(&workspace.tab_id, label);
            server.rename_workspace(&workspace.workspace_id, workspace_label);
            server.report_agent(&workspace.pane_id, "claude", "blocked");
            server.recording_herdr(&dir, &dir.join("log"));
            Self { dir, workspace }
        }

        fn bin(&self) -> String {
            self.dir.join("herdr").to_str().unwrap().to_string()
        }

        fn pane(&self) -> &str {
            &self.workspace.pane_id
        }

        fn label(&self) -> String {
            crate::test_herdr::HerdrServer::shared().tab_label(&self.workspace.tab_id)
        }

        /// A second pane in the same tab, with its own blocked agent.
        fn second_pane(&self) -> String {
            let server = crate::test_herdr::HerdrServer::shared();
            let pane = server.split(self.pane());
            server.report_agent(&pane, "codex", "blocked");
            pane
        }

        fn pane_event(&self, pane: &str, status: &str) -> StatusEvent {
            StatusEvent {
                pane_id: pane.to_string(),
                ..self.event(status, "claude")
            }
        }

        fn status(&self, pane: &str, status: &str) {
            self.status_in(pane, &self.workspace.tab_id, status);
        }

        /// An unfocused agent in `tab_id` enters `status`.
        fn status_in(&self, pane: &str, tab_id: &str, status: &str) {
            self.status_with(&NotifyConfig::default(), pane, tab_id, status);
        }

        fn status_with(&self, config: &NotifyConfig, pane: &str, tab_id: &str, status: &str) {
            sync_for_status(
                config,
                &self.pane_event(pane, status),
                &PaneDetails::in_tab(Some(tab_id), false),
                &self.bin(),
                deadline(),
            );
        }

        /// A second tab in the workspace, with a blocked agent in its pane.
        fn other_tab(&self) -> (String, String) {
            let server = crate::test_herdr::HerdrServer::shared();
            let (tab, pane) = server.tab_in(&self.workspace.workspace_id, "other");
            server.report_agent(&pane, "codex", "blocked");
            (tab, pane)
        }

        fn tab_label_of(&self, tab_id: &str) -> String {
            crate::test_herdr::HerdrServer::shared().tab_label(tab_id)
        }

        fn focus(&self, pane: &str) {
            clear_for_focus(&NotifyConfig::default(), pane, &self.bin());
        }

        fn labels(&self) -> (String, String) {
            (self.label(), self.workspace_label())
        }

        fn workspace_label(&self) -> String {
            crate::test_herdr::HerdrServer::shared().workspace_label(&self.workspace.workspace_id)
        }

        fn event(&self, status: &str, agent: &str) -> StatusEvent {
            StatusEvent {
                pane_id: self.pane().to_string(),
                status: status.to_string(),
                agent: (!agent.is_empty()).then(|| agent.to_string()),
                agent_id: None,
                terminal_title: None,
                state_label: None,
            }
        }

        fn in_tab(&self, focused: bool) -> PaneDetails {
            PaneDetails::in_tab(Some(&self.workspace.tab_id), focused)
        }

        fn unfocused_in_tab(&self) -> PaneDetails {
            self.in_tab(false)
        }

        /// The recorded calls, and forgets them.
        fn take_calls(&self) -> Vec<String> {
            let log = self.dir.join("log");
            let calls = std::fs::read_to_string(&log).unwrap_or_default();
            let _ = std::fs::remove_file(&log);
            calls
                .lines()
                .map(|line| {
                    line.trim_end_matches('|')
                        .split('|')
                        .map(|field| match field {
                            f if f == self.workspace.tab_id => "w1:t1",
                            f if f == self.workspace.pane_id => "w1:p2",
                            f if f == self.workspace.workspace_id => "w1",
                            f => f,
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect()
        }
    }

    impl Drop for TestHerdr {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn a_blocked_agent_marks_its_tab_and_agent_name() {
        let herdr = TestHerdr::new("blocked", "mobile zsh");
        let config = NotifyConfig::default();

        let original = sync_for_status(
            &config,
            &herdr.event("blocked", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(original.tab.as_deref(), Some("mobile zsh"));
        assert_eq!(original.workspace.as_deref(), Some("dotfiles"));

        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_tab=1",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_workspace=1",
                "tab|get|w1:t1",
                "tab|rename|w1:t1|🟡 mobile zsh",
                "workspace|get|w1",
                "workspace|rename|w1|🟡 dotfiles",
                "pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|● claude",
            ]
        );
        assert_eq!(herdr.label(), "🟡 mobile zsh");
        assert_eq!(herdr.workspace_label(), "🟡 dotfiles");
    }

    #[test]
    fn an_agent_back_at_work_gets_its_original_tab_label_and_agent_name_back() {
        let herdr = TestHerdr::with_workspace("working", "🟡 mobile zsh", "🟡 dotfiles");

        sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("working", "● claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "tab|get|w1:t1",
                "tab|rename|w1:t1|mobile zsh",
                "workspace|get|w1",
                "workspace|rename|w1|dotfiles",
                "pane|report-metadata|w1:p2|--source|dagadbm|--clear-display-agent",
            ]
        );
        assert_eq!(herdr.label(), "mobile zsh");
        assert_eq!(herdr.workspace_label(), "dotfiles");
    }

    #[test]
    fn marking_then_moving_on_restores_the_exact_label() {
        let herdr = TestHerdr::new("round-trip", "  my tab 🟡 x");
        let config = NotifyConfig::default();
        let details = herdr.unfocused_in_tab();

        sync_for_status(
            &config,
            &herdr.event("done", "codex"),
            &details,
            &herdr.bin(),
            deadline(),
        );
        assert_eq!(herdr.label(), "🟡   my tab 🟡 x");
        sync_for_status(
            &config,
            &herdr.event("working", "● codex"),
            &details,
            &herdr.bin(),
            deadline(),
        );
        assert_eq!(herdr.label(), "  my tab 🟡 x");
        assert_eq!(herdr.workspace_label(), "dotfiles");
    }

    #[test]
    fn focusing_the_pane_clears_both_markers() {
        let herdr = TestHerdr::with_workspace("focus", "🟡 3", "🟡 dotfiles");

        clear_for_focus(&NotifyConfig::default(), herdr.pane(), &herdr.bin());

        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "tab|get|w1:t1",
                "tab|rename|w1:t1|3",
                "workspace|get|w1",
                "workspace|rename|w1|dotfiles",
                "pane|report-metadata|w1:p2|--source|dagadbm|--clear-display-agent",
            ]
        );
        assert_eq!(herdr.label(), "3");
        assert_eq!(herdr.workspace_label(), "dotfiles");
    }

    #[test]
    fn focusing_with_both_markers_off_calls_nothing() {
        let herdr = TestHerdr::new("focus-off", "🟡 3");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;
        config.agent_marker.enabled = false;
        config.workspace_marker.enabled = false;

        clear_for_focus(&config, herdr.pane(), &herdr.bin());

        assert!(herdr.take_calls().is_empty());
    }

    #[test]
    fn a_disabled_tab_marker_leaves_the_tab_alone() {
        let herdr = TestHerdr::new("tab-off", "mobile");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;
        config.workspace_marker.enabled = false;

        sync_for_status(
            &config,
            &herdr.event("blocked", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            ["pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|● claude"]
        );
    }

    #[test]
    fn a_disabled_agent_marker_leaves_the_agent_name_alone() {
        let herdr = TestHerdr::new("agent-off", "mobile");
        let mut config = NotifyConfig::default();
        config.agent_marker.enabled = false;

        sync_for_status(
            &config,
            &herdr.event("blocked", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_tab=1",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_workspace=1",
                "tab|get|w1:t1",
                "tab|rename|w1:t1|🟡 mobile",
                "workspace|get|w1",
                "workspace|rename|w1|🟡 dotfiles"
            ]
        );
    }

    const MARKED: (&str, &str) = ("🟡 mobile", "🟡 dotfiles");
    const UNMARKED: (&str, &str) = ("mobile", "dotfiles");

    fn assert_labels(herdr: &TestHerdr, expected: (&str, &str)) {
        let (tab, workspace) = herdr.labels();
        assert_eq!((tab.as_str(), workspace.as_str()), expected);
    }

    #[test]
    fn focusing_one_of_two_blocked_agents_keeps_the_tab_and_workspace_marked() {
        let herdr = TestHerdr::new("two-focus", "mobile");
        let other = herdr.second_pane();
        herdr.status(herdr.pane(), "blocked");
        herdr.status(&other, "blocked");
        assert_labels(&herdr, MARKED);

        herdr.focus(herdr.pane());
        assert_labels(&herdr, MARKED);

        herdr.focus(&other);
        assert_labels(&herdr, UNMARKED);
    }

    #[test]
    fn one_of_two_agents_moving_on_keeps_the_tab_and_workspace_marked() {
        let herdr = TestHerdr::new("two-working", "mobile");
        let other = herdr.second_pane();
        herdr.status(herdr.pane(), "blocked");
        herdr.status(&other, "blocked");

        herdr.status(herdr.pane(), "working");
        assert_labels(&herdr, MARKED);

        herdr.status(&other, "working");
        assert_labels(&herdr, UNMARKED);
    }

    #[test]
    fn focusing_a_pane_that_was_never_marked_leaves_another_agents_marker() {
        let herdr = TestHerdr::new("never-marked", "mobile");
        let calm = herdr.second_pane();
        herdr.status(herdr.pane(), "blocked");
        assert_labels(&herdr, MARKED);

        herdr.focus(&calm);

        assert_labels(&herdr, MARKED);
    }

    #[test]
    fn each_tab_of_a_workspace_is_marked_by_its_own_agents() {
        let herdr = TestHerdr::new("two-tabs", "mobile");
        let (other_tab, other) = herdr.other_tab();
        herdr.status(herdr.pane(), "blocked");
        herdr.status_in(&other, &other_tab, "blocked");
        assert_eq!(herdr.label(), "🟡 mobile");
        assert_eq!(herdr.tab_label_of(&other_tab), "🟡 other");

        herdr.focus(herdr.pane());

        assert_eq!(herdr.label(), "mobile");
        assert_eq!(herdr.tab_label_of(&other_tab), "🟡 other");
        assert_eq!(herdr.workspace_label(), "🟡 dotfiles");

        herdr.focus(&other);

        assert_eq!(herdr.tab_label_of(&other_tab), "other");
        assert_eq!(herdr.workspace_label(), "dotfiles");
    }

    #[test]
    fn an_agent_in_one_tab_moving_on_leaves_the_other_tab_and_the_workspace_marked() {
        let herdr = TestHerdr::new("tab-moves-on", "mobile");
        let (other_tab, other) = herdr.other_tab();
        herdr.status(herdr.pane(), "blocked");
        herdr.status_in(&other, &other_tab, "blocked");

        herdr.status(herdr.pane(), "working");

        assert_eq!(herdr.label(), "mobile");
        assert_eq!(herdr.tab_label_of(&other_tab), "🟡 other");
        assert_eq!(herdr.workspace_label(), "🟡 dotfiles");
    }

    #[test]
    fn another_workspace_keeps_its_marker_when_this_ones_pane_is_focused() {
        let herdr = TestHerdr::new("two-workspaces", "mobile");
        let away = TestHerdr::new("two-workspaces-away", "elsewhere");
        away.status(away.pane(), "blocked");
        herdr.status(herdr.pane(), "blocked");

        herdr.focus(herdr.pane());

        assert_labels(&herdr, UNMARKED);
        assert_eq!(away.label(), "🟡 elsewhere");
        assert_eq!(away.workspace_label(), "🟡 dotfiles");
    }

    #[test]
    fn closing_a_marked_pane_unmarks_on_the_next_event_in_its_tab() {
        let herdr = TestHerdr::new("closed", "mobile");
        let calm = herdr.second_pane();
        herdr.status(herdr.pane(), "blocked");
        assert_labels(&herdr, MARKED);

        crate::test_herdr::HerdrServer::shared().close_pane(herdr.pane());
        herdr.status(&calm, "working");

        assert_labels(&herdr, UNMARKED);
    }

    #[test]
    fn an_agent_that_needs_you_again_after_you_looked_is_marked_again() {
        let herdr = TestHerdr::new("again", "mobile");
        herdr.status(herdr.pane(), "blocked");
        herdr.focus(herdr.pane());
        assert_labels(&herdr, UNMARKED);

        herdr.status(herdr.pane(), "done");

        assert_labels(&herdr, MARKED);
    }

    #[test]
    fn the_tab_and_the_workspace_follow_their_own_statuses() {
        let herdr = TestHerdr::new("own-statuses", "mobile");
        let mut config = NotifyConfig::default();
        config.tab_marker = config.tab_marker.with_statuses(&["blocked"]);
        config.workspace_marker = config.workspace_marker.with_statuses(&["done"]);
        let tab = &herdr.workspace.tab_id;

        herdr.status_with(&config, herdr.pane(), tab, "done");
        assert_labels(&herdr, ("mobile", "🟡 dotfiles"));

        herdr.status_with(&config, herdr.pane(), tab, "blocked");
        assert_labels(&herdr, ("🟡 mobile", "dotfiles"));

        herdr.status_with(&config, herdr.pane(), tab, "working");
        assert_labels(&herdr, UNMARKED);
    }

    #[test]
    fn a_disabled_workspace_marker_leaves_the_workspace_alone() {
        let herdr = TestHerdr::new("workspace-off", "mobile");
        let mut config = NotifyConfig::default();
        config.workspace_marker.enabled = false;

        sync_for_status(
            &config,
            &herdr.event("blocked", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert!(!herdr
            .take_calls()
            .iter()
            .any(|call| call.contains("workspace")));
        assert_eq!(herdr.workspace_label(), "dotfiles");
    }

    #[test]
    fn a_workspace_the_user_renamed_is_not_unmarked() {
        let herdr = TestHerdr::with_workspace("renamed", "mobile", "my 🟡 space");

        let original = sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("working", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(original.workspace.as_deref(), Some("my 🟡 space"));
        assert_eq!(herdr.workspace_label(), "my 🟡 space");
    }

    #[test]
    fn a_focused_pane_is_not_marked() {
        let herdr = TestHerdr::new("focused", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("blocked", "claude"),
            &herdr.in_tab(true),
            &herdr.bin(),
            deadline(),
        );

        // The labels are read to see whether they need clearing, nothing more.
        assert_eq!(
            herdr.take_calls(),
            ["pane|list", "tab|get|w1:t1", "workspace|get|w1"]
        );
        assert_eq!(herdr.label(), "mobile");
    }

    #[test]
    fn a_status_that_is_not_marked_changes_nothing_unmarked() {
        let herdr = TestHerdr::new("unlisted", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("working", "claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            ["pane|list", "tab|get|w1:t1", "workspace|get|w1"]
        );
    }

    #[test]
    fn a_pane_without_a_tab_marks_the_workspace_and_agent() {
        let herdr = TestHerdr::new("no-tab", "mobile");

        sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("blocked", "claude"),
            &PaneDetails::in_tab(None, false),
            &herdr.bin(),
            deadline(),
        );

        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_workspace=1",
                "workspace|get|w1",
                "workspace|rename|w1|🟡 dotfiles",
                "pane|report-metadata|w1:p2|--source|dagadbm|--display-agent|● claude"
            ]
        );
    }

    #[test]
    fn an_already_marked_tab_and_agent_are_not_marked_again() {
        let herdr = TestHerdr::with_workspace("already", "🟡 mobile", "🟡 dotfiles");

        let original = sync_for_status(
            &NotifyConfig::default(),
            &herdr.event("blocked", "● claude"),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        // The notification still names the tab without the marker.
        assert_eq!(original.tab.as_deref(), Some("mobile"));
        assert_eq!(original.workspace.as_deref(), Some("dotfiles"));
        // The tokens are written even when the labels already carry the
        // marker, since they are what keeps it there.
        assert_eq!(
            herdr.take_calls(),
            [
                "pane|list",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_tab=1",
                "pane|report-metadata|w1:p2|--source|dagadbm|--token|marked_workspace=1",
                "tab|get|w1:t1",
                "workspace|get|w1"
            ]
        );
        assert_eq!(herdr.label(), "🟡 mobile");
        assert_eq!(herdr.workspace_label(), "🟡 dotfiles");
    }

    #[test]
    fn an_event_without_an_agent_reports_no_agent_name() {
        let herdr = TestHerdr::new("no-agent", "mobile");
        let mut config = NotifyConfig::default();
        config.tab_marker.enabled = false;
        config.workspace_marker.enabled = false;

        sync_for_status(
            &config,
            &herdr.event("blocked", ""),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );
        sync_for_status(
            &config,
            &herdr.event("working", ""),
            &herdr.unfocused_in_tab(),
            &herdr.bin(),
            deadline(),
        );

        assert!(herdr.take_calls().is_empty());
    }
}
