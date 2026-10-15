use std::collections::BTreeMap;

use serde::Deserialize;

use crate::notify::config::Templates;
use crate::notify::icons::agent_icon_path;
use crate::notify::notification::{FocusNotification, PaneLabels};
use crate::notify::template::{render, TokenValues};
use crate::notify::util::notification_group_id;

#[derive(Debug, Deserialize)]
struct PluginEvent {
    data: Option<EventData>,
}

#[derive(Debug, Deserialize)]
struct EventData {
    pane_id: Option<String>,
    agent_status: Option<String>,
    agent: Option<String>,
    display_agent: Option<String>,
    /// The pane's terminal title: what that agent or shell is working on.
    title: Option<String>,
    /// Labels Herdr reports per status, e.g. `{"blocked": "Needs an answer"}`.
    state_labels: Option<BTreeMap<String, String>>,
}

/// What an agent status event says about its pane, before Herdr and git are
/// asked for the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusEvent {
    pub(crate) pane_id: String,
    /// Lowercase, e.g. `blocked`.
    pub(crate) status: String,
    pub(crate) agent: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) terminal_title: Option<String>,
    pub(crate) state_label: Option<String>,
}

pub(crate) fn status_event_from_json(json: &str) -> Result<Option<StatusEvent>, String> {
    let event: PluginEvent =
        serde_json::from_str(json).map_err(|err| format!("invalid event json: {err}"))?;
    let Some(data) = event.data else {
        return Ok(None);
    };
    let Some(pane_id) = pane_id_from_event_data(&data) else {
        return Ok(None);
    };

    let status = data
        .agent_status
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let state_label = data
        .state_labels
        .as_ref()
        .and_then(|labels| labels.get(status.as_str()))
        .and_then(|label| trimmed(Some(label)))
        .map(str::to_string);

    Ok(Some(StatusEvent {
        pane_id,
        status,
        agent: first_non_empty([data.display_agent.as_deref(), data.agent.as_deref()])
            .map(str::to_string),
        agent_id: trimmed(data.agent.as_deref()).map(str::to_string),
        terminal_title: trimmed(data.title.as_deref()).map(str::to_string),
        state_label,
    }))
}

pub(crate) fn focused_pane_id_from_event_json(json: &str) -> Result<Option<String>, String> {
    let event: PluginEvent =
        serde_json::from_str(json).map_err(|err| format!("invalid event json: {err}"))?;

    Ok(event.data.as_ref().and_then(pane_id_from_event_data))
}

fn pane_id_from_event_data(data: &EventData) -> Option<String> {
    trimmed(data.pane_id.as_deref()).map(str::to_string)
}

/// The values of every template token for one notification. Herdr's answers
/// about the pane win over the event's own copy, which can be older.
pub(crate) fn token_values(
    event: &StatusEvent,
    labels: PaneLabels,
    hostname: Option<String>,
) -> TokenValues {
    let mut values = TokenValues::default();
    let mut set = |token: &'static str, value: Option<String>| {
        if let Some(value) = value {
            values.builtins.insert(token, value);
        }
    };

    set("workspace", labels.workspace);
    set("tab", labels.tab);
    set("pane", pane_name(&event.pane_id));
    set("agent", event.agent.clone());
    set(
        "terminal_title",
        labels
            .terminal_title
            .or_else(|| event.terminal_title.clone()),
    );
    set(
        "terminal_title_stripped",
        labels
            .terminal_title_stripped
            .or_else(|| event.terminal_title.clone()),
    );
    set("state_icon", state_icon(&event.status).map(str::to_string));
    set("state_text", state_text(event));
    set("branch", labels.branch);
    set("git_status", labels.dirty.then(|| "*".to_string()));
    set("hostname", hostname);

    values.metadata = labels.metadata;
    values
}

pub(crate) fn render_notification(
    event: &StatusEvent,
    templates: &Templates,
    values: &TokenValues,
) -> FocusNotification {
    let subtitle = render(&templates.subtitle, values);

    FocusNotification {
        pane_id: event.pane_id.clone(),
        status: event.status.clone(),
        title: render(&templates.title, values),
        body: render(&templates.message, values),
        subtitle: (!subtitle.trim().is_empty()).then_some(subtitle),
        action_label: render(&templates.action_label, values),
        close_label: render(&templates.close_label, values),
        group: notification_group_id(&event.pane_id),
        app_icon: agent_icon_path(&[event.agent.as_deref(), event.agent_id.as_deref()]),
    }
}

/// The pane part of a pane id, e.g. `w1:p3` -> `p3`: Herdr gives panes no
/// name of their own.
fn pane_name(pane_id: &str) -> Option<String> {
    let name = pane_id.rsplit(':').next().unwrap_or(pane_id);
    (!name.is_empty()).then(|| name.to_string())
}

/// The default `{state_icon}` per status; `[notify.state_icons]` replaces
/// any of them.
fn state_icon(status: &str) -> Option<&'static str> {
    match status {
        "blocked" => Some("🚨"),
        "done" => Some("✅"),
        "working" => Some("⏳"),
        "idle" => Some("💤"),
        "" => None,
        _ => Some("❓"),
    }
}

/// Herdr's label for the status when the event carries one, else the status
/// itself, capitalized: `Blocked`.
fn state_text(event: &StatusEvent) -> Option<String> {
    event.state_label.clone().or_else(|| {
        let mut chars = event.status.chars();
        let first = chars.next()?;
        Some(first.to_uppercase().chain(chars).collect())
    })
}

fn first_non_empty<const N: usize>(values: [Option<&str>; N]) -> Option<&str> {
    values.into_iter().find_map(trimmed)
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::config::NotifyConfig;

    fn blocked_event() -> StatusEvent {
        status_event_from_json(
            r#"{
                "event": "pane.agent_status_changed",
                "data": {
                    "pane_id": "w1:p3",
                    "workspace_id": "w1",
                    "agent_status": "Blocked",
                    "agent": "codex",
                    "display_agent": "Codex",
                    "title": "Implement plugin"
                }
            }"#,
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn reads_the_status_event() {
        let event = blocked_event();

        assert_eq!(event.pane_id, "w1:p3");
        assert_eq!(event.status, "blocked");
        assert_eq!(event.agent.as_deref(), Some("Codex"));
        assert_eq!(event.agent_id.as_deref(), Some("codex"));
        assert_eq!(event.terminal_title.as_deref(), Some("Implement plugin"));
        assert_eq!(event.state_label, None);
    }

    #[test]
    fn ignores_events_without_a_pane() {
        let json = r#"{"data": {"agent_status": "blocked", "agent": "Codex"}}"#;

        assert!(status_event_from_json(json).unwrap().is_none());
        assert!(status_event_from_json(r#"{}"#).unwrap().is_none());
    }

    #[test]
    fn renders_the_default_layout_from_herdr_labels() {
        let event = blocked_event();
        let values = token_values(
            &event,
            PaneLabels {
                workspace: Some("sample-repo".to_string()),
                tab: Some("status".to_string()),
                terminal_title: Some("◐ Tidy up the parser tests".to_string()),
                terminal_title_stripped: Some("Tidy up the parser tests".to_string()),
                branch: Some("main".to_string()),
                dirty: true,
                ..PaneLabels::default()
            },
            Some("mac".to_string()),
        );

        let notification = render_notification(
            &event,
            &NotifyConfig::default().templates_for("blocked"),
            &values,
        );

        assert_eq!(notification.title, "⌂ sample-repo ▸ status");
        assert_eq!(notification.subtitle, None);
        assert_eq!(notification.body, "◐ Tidy up the parser tests");
        assert_eq!(notification.action_label, "Focus");
        assert_eq!(notification.close_label, "Dismiss");
        assert_eq!(notification.group, "herdr-w1-p3");
        assert!(notification
            .app_icon
            .as_deref()
            .unwrap()
            .ends_with("/icons/codex-color.png"));
    }

    #[test]
    fn missing_labels_render_empty_and_the_event_title_fills_in() {
        let event = blocked_event();
        let values = token_values(&event, PaneLabels::default(), None);

        let notification = render_notification(
            &event,
            &NotifyConfig::default().templates_for("blocked"),
            &values,
        );

        assert_eq!(notification.title, "⌂  ▸ ");
        assert_eq!(notification.subtitle, None);
        assert_eq!(notification.body, "Implement plugin");
    }

    #[test]
    fn fills_every_token() {
        let mut event = blocked_event();
        event.state_label = Some("Needs an answer".to_string());
        let mut metadata = BTreeMap::new();
        metadata.insert("jj".to_string(), "clean".to_string());
        let values = token_values(
            &event,
            PaneLabels {
                metadata,
                ..PaneLabels::default()
            },
            Some("mac".to_string()),
        );
        let templates = Templates {
            title: "{state_icon} {state_text}".to_string(),
            subtitle: "{pane}@{hostname}".to_string(),
            message: "{terminal_title} $jj".to_string(),
            action_label: "Go".to_string(),
            close_label: "Later".to_string(),
        };

        let notification = render_notification(&event, &templates, &values);

        assert_eq!(notification.title, "🚨 Needs an answer");
        assert_eq!(notification.subtitle.as_deref(), Some("p3@mac"));
        assert_eq!(notification.body, "Implement plugin clean");
        assert_eq!(notification.action_label, "Go");
        assert_eq!(notification.close_label, "Later");
    }

    #[test]
    fn an_empty_subtitle_is_left_out() {
        let event = blocked_event();
        let values = token_values(&event, PaneLabels::default(), None);
        let mut templates = NotifyConfig::default().templates_for("blocked");
        templates.subtitle = "{branch}".to_string();

        assert_eq!(
            render_notification(&event, &templates, &values).subtitle,
            None
        );
    }

    #[test]
    fn extracts_pane_id_from_focus_event() {
        let json = r#"{"event": "pane.focused", "data": {"pane_id": " w1:p2 "}}"#;

        assert_eq!(
            focused_pane_id_from_event_json(json).unwrap(),
            Some("w1:p2".to_string())
        );
    }
}
