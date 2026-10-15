use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusNotification {
    pub(crate) pane_id: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Secondary line shown under the title (alerter `--subtitle`). An empty
    /// rendered subtitle is left out rather than shown as a blank line.
    pub(crate) subtitle: Option<String>,
    /// The button that focuses the pane, and the text its click reports back.
    pub(crate) action_label: String,
    pub(crate) close_label: String,
    pub(crate) group: String,
    pub(crate) app_icon: Option<String>,
}

/// What Herdr and git add to a notification about its pane, beyond the event's
/// own copy. Every field is best-effort: one that was not answered in time
/// stays None and renders as an empty token.
#[derive(Debug, Default)]
pub(crate) struct PaneLabels {
    pub(crate) workspace: Option<String>,
    pub(crate) tab: Option<String>,
    pub(crate) terminal_title: Option<String>,
    pub(crate) terminal_title_stripped: Option<String>,
    pub(crate) branch: Option<String>,
    /// Whether the pane's directory has tracked changes versus `HEAD`.
    pub(crate) dirty: bool,
    /// Custom values the pane reported through `herdr pane report-metadata`,
    /// rendered by `$name` tokens.
    pub(crate) metadata: BTreeMap<String, String>,
}
