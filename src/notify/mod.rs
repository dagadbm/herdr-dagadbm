//! Clickable macOS notifications for agent panes that need attention, with
//! configurable text. Ported from yankewei/herdr-focus-notify (MIT).

mod cli;
mod config;
mod event;
mod executable;
mod focus;
mod git;
mod icons;
mod ledger;
mod notification;
mod notifier;
mod outcome;
mod script;
mod state;
mod tab_marker;
mod template;
mod terminal;
mod timings;
mod util;

use std::env;
use std::time::Instant;

use cli::{parse_cli_args, print_usage, CliAction, Outcome};
use event::{
    focused_pane_id_from_event_json, render_notification, status_event_from_json, token_values,
    StatusEvent,
};
use executable::resolve_herdr_bin;
use focus::{
    focused_pane_id, frontmost_bundle_id, learn_terminal_from_frontmost, notification_decision,
    pane_details, pane_labels, should_clear_notification_on_focus, NotificationDecision,
    PaneDetails,
};
use notification::{FocusNotification, PaneLabels};
use notifier::{remove_notification, resolve_notifier_bin, send_notification};
use script::{
    rewrite_generated_scripts_without_activation, test_timeout_secs, write_focus_script,
    AlerterOptions,
};
use state::{
    cleanup_stale_state_files, clear_terminal_bindings, mark_notification_cleared,
    prune_stale_workspace_bindings, record_status, reset_notification_clearance,
};
use tab_marker::OriginalLabels;
use timings::Timings;

pub(crate) fn run<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    parse_cli_args(args).and_then(|(action, timings)| dispatch(action, timings))
}

fn dispatch(action: CliAction, timings: Timings) -> Result<(), String> {
    match action.clone() {
        CliAction::Help => {
            print_usage();
            Ok(())
        }
        CliAction::InitConfig => init_config(),
        CliAction::Cleanup => cleanup(),
        CliAction::ClearTerminalBindings => clear_bindings(),
        CliAction::FocusPane(pane_id) => focus::focus_pane(&pane_id, &timings),
        CliAction::NotificationEnded(how, pane_id, post_id) => {
            notification_ended(how, &pane_id, &post_id, &timings)
        }
        CliAction::CheckPaneVisibility(pane_id) => check_pane_visibility(&pane_id, &timings),
        CliAction::Event | CliAction::Test => notify(action == CliAction::Test, timings),
    }
}

fn notification_ended(
    how: Outcome,
    pane_id: &str,
    post_id: &str,
    timings: &Timings,
) -> Result<(), String> {
    match how {
        Outcome::Clicked => outcome::clicked(pane_id, post_id, timings),
        Outcome::Closed => outcome::closed(pane_id, post_id, timings),
        Outcome::Ended => outcome::ended(pane_id, post_id),
    }
}

fn init_config() -> Result<(), String> {
    let (path, written) = config::init_config()?;
    if written {
        println!("wrote {}", path.display());
    } else {
        println!("{} already exists; left it unchanged", path.display());
    }
    Ok(())
}

fn rewrite_focus_scripts() -> Result<(), String> {
    rewrite_generated_scripts_without_activation()
        .map_err(|err| format!("failed to update generated focus scripts: {err}"))
}

fn cleanup() -> Result<(), String> {
    cleanup_stale_state_files()
        .map_err(|err| format!("failed to clean stale state files: {err}"))
        .and_then(|()| rewrite_focus_scripts())?;
    // Terminal bindings for workspaces that no longer exist are stale
    // too; best-effort, since Herdr may not be reachable.
    let _ = resolve_herdr_bin().map(|herdr_bin| prune_stale_workspace_bindings(&herdr_bin));
    Ok(())
}

fn clear_bindings() -> Result<(), String> {
    clear_terminal_bindings().map_err(|err| format!("failed to clear terminal bindings: {err}"))?;
    rewrite_focus_scripts()
}

fn check_pane_visibility(pane_id: &str, timings: &Timings) -> Result<(), String> {
    let herdr_bin = resolve_herdr_bin()?;
    let deadline = Instant::now() + timings.lookup_budget;
    let details = pane_details(pane_id, &herdr_bin, deadline).unwrap_or_default();
    if notification_decision(pane_id, details.focused) == NotificationDecision::Skip {
        return Ok(());
    }
    Err("pane is not visible in its workspace's bound terminal".to_string())
}

fn notify(test_mode: bool, timings: Timings) -> Result<(), String> {
    let herdr_bin = resolve_herdr_bin()?;
    let config = config::load();

    let Some(event) = next_event(test_mode, &config, &herdr_bin)? else {
        return Ok(());
    };

    // One `herdr agent get` answers both questions below: whether the user is
    // already looking at this pane, and what the pane is working on. The skip
    // decision runs before the remaining Herdr and git calls, because a
    // suppressed notification must not pay for them. An unanswered lookup
    // counts as unfocused, so the notification is sent.
    let deadline = Instant::now() + timings.lookup_budget;
    let mut details = pane_details(&event.pane_id, &herdr_bin, deadline).unwrap_or_default();

    let Some((decision, original)) = screen(
        test_mode,
        &config,
        &event,
        &mut details,
        &herdr_bin,
        deadline,
    ) else {
        return Ok(());
    };

    // Resolved before the lookups behind the text, so an install without a
    // notifier fails without paying for them.
    let notifier_bin = resolve_notifier_bin()?;

    let labels = pane_labels(
        &event.pane_id,
        details,
        original.tab,
        original.workspace,
        &herdr_bin,
        deadline,
    );
    let notification = compose(&config, &event, labels);
    deliver(
        notification,
        &config,
        timings,
        &herdr_bin,
        &notifier_bin,
        decision,
        test_mode,
    )
}

/// The status event to notify about, or `None` when there is nothing to do.
fn next_event(
    test_mode: bool,
    config: &config::NotifyConfig,
    herdr_bin: &str,
) -> Result<Option<StatusEvent>, String> {
    if test_mode {
        // The test renders the configured templates against the focused pane,
        // as if its agent were blocked, so it shows what a real one would.
        return Ok(Some(StatusEvent {
            pane_id: focused_pane_id(herdr_bin).unwrap_or_else(|| "test-pane".to_string()),
            status: "blocked".to_string(),
            agent: Some("dagadbm test".to_string()),
            agent_id: None,
            terminal_title: Some("Click to return to this Herdr pane.".to_string()),
            state_label: None,
        }));
    }

    let Ok(event_json) = env::var("HERDR_PLUGIN_EVENT_JSON") else {
        return Ok(None);
    };
    if env::var("HERDR_PLUGIN_EVENT").as_deref() == Ok("pane.focused") {
        handle_pane_focused(&event_json, config, herdr_bin)?;
        return Ok(None);
    }

    let Some(event) = status_event_from_json(&event_json)? else {
        return Ok(None);
    };
    // A repeat of the pane's last status already had its notification and
    // its markers. Best-effort: when the state cannot be read or written,
    // the event counts as a change.
    if !record_status(&event.pane_id, &event.status).unwrap_or(true) {
        return Ok(None);
    }
    Ok(Some(event))
}

/// How to send the notification and the labels read for it, or `None` when
/// it is suppressed.
fn screen(
    test_mode: bool,
    config: &config::NotifyConfig,
    event: &StatusEvent,
    details: &mut PaneDetails,
    herdr_bin: &str,
    deadline: Instant,
) -> Option<(NotificationDecision, OriginalLabels)> {
    // Herdr calls a pane focused even when another app (a video overlay) is
    // frontmost. Only a skipped notification means the user is really looking
    // at it, so that, not Herdr's flag, decides whether the markers apply.
    let decision = notification_decision(&event.pane_id, details.focused);
    details.focused = decision == NotificationDecision::Skip;

    // The labels are read before the markers change them, so the
    // notification names the tab and workspace rather than the markers.
    let mut original = OriginalLabels::default();
    if !test_mode {
        original = tab_marker::sync_for_status(config, event, details, herdr_bin, deadline);
        if !config.status_is_enabled(&event.status) {
            return None;
        }
    }

    match decision {
        // --test validates the pipeline end to end, so it never suppresses the
        // notification; it just goes without the visibility monitor.
        NotificationDecision::Skip if test_mode => Some((NotificationDecision::Send, original)),
        NotificationDecision::Skip => None,
        decision => Some((decision, original)),
    }
}

fn compose(
    config: &config::NotifyConfig,
    event: &StatusEvent,
    labels: PaneLabels,
) -> FocusNotification {
    let mut values = token_values(event, labels, util::hostname());
    if let Some(icon) = config.state_icon(&event.status) {
        values.builtins.insert("state_icon", icon.to_string());
    }
    render_notification(event, &config.templates_for(&event.status), &values)
}

fn deliver(
    notification: FocusNotification,
    config: &config::NotifyConfig,
    timings: Timings,
    herdr_bin: &str,
    notifier_bin: &str,
    decision: NotificationDecision,
    test_mode: bool,
) -> Result<(), String> {
    reset_notification_clearance(&notification.pane_id)
        .map_err(|err| format!("failed to reset notification clearance: {err}"))?;

    let options = AlerterOptions {
        timeout_secs: if test_mode {
            test_timeout_secs(config.timeout_secs)
        } else {
            config.timeout_secs
        },
        sound: config.sound.clone(),
        timings,
    };
    let monitor_visibility = decision == NotificationDecision::SendWithVisibilityMonitor;

    // The ledger keeps the notification as sent, so a click that closes it by
    // mistake can post it again (ADR 12). Best-effort: without the record the
    // notification still goes out, it just cannot come back.
    let record = ledger::Record::new(notification, options.timeout_secs, monitor_visibility);
    let _ = ledger::save(&record);

    let script_path = write_focus_script(
        &record.notification,
        &record.post_id,
        herdr_bin,
        notifier_bin,
        &options,
        monitor_visibility,
    )
    .map_err(|err| format!("failed to write focus script: {err}"))?;

    send_notification(&script_path, test_mode)
        .map_err(|err| format!("failed to send notification: {err}"))
}

fn handle_pane_focused(
    event_json: &str,
    config: &config::NotifyConfig,
    herdr_bin: &str,
) -> Result<(), String> {
    let Some(pane_id) = focused_pane_id_from_event_json(event_json)? else {
        return Ok(());
    };

    tab_marker::clear_for_focus(config, &pane_id, herdr_bin);

    // Zero-configuration terminal detection: bind the frontmost terminal to
    // this pane's workspace. Both decisions below share one frontmost lookup,
    // taken as early as possible because the app it reports must still be the
    // one the user focused from. learn_terminal_from_frontmost ignores
    // notification-originated focus events and obvious non-terminal apps,
    // while keeping this event path best-effort.
    let frontmost = frontmost_bundle_id();
    let workspace = util::workspace_of(&pane_id);
    learn_terminal_from_frontmost(workspace, frontmost.as_deref());

    if should_clear_notification_on_focus(workspace, frontmost.as_deref()) {
        let notifier_bin = resolve_notifier_bin()?;
        mark_notification_cleared(&pane_id)
            .map_err(|err| format!("failed to mark notification as cleared: {err}"))?;
        let _ = ledger::resolve(&pane_id, None, ledger::Status::Done);
        remove_notification(&pane_id, &notifier_bin)
            .map_err(|err| format!("failed to remove notification: {err}"))?;
    }

    Ok(())
}
