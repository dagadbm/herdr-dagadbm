//! Clickable macOS notifications for agent panes that need attention, with
//! configurable text. Ported from yankewei/herdr-focus-notify (MIT).

mod cli;
mod config;
mod event;
mod executable;
mod focus;
mod git;
mod icons;
mod notification;
mod notifier;
mod script;
mod state;
mod tab_marker;
mod template;
mod terminal;
mod util;

use std::env;
use std::time::{Duration, Instant};

use cli::{parse_cli_args, print_usage, CliAction};
use event::{
    focused_pane_id_from_event_json, render_notification, status_event_from_json, token_values,
    StatusEvent,
};
use executable::resolve_herdr_bin;
use focus::{
    focused_pane_id, frontmost_bundle_id, learn_terminal_from_frontmost, notification_decision,
    pane_details, pane_labels, should_clear_notification_on_focus, NotificationDecision,
};
use notifier::{remove_notification, resolve_notifier_bin, send_notification};
use script::{rewrite_generated_scripts_without_activation, write_focus_script, AlerterOptions};
use state::{
    cleanup_stale_state_files, clear_terminal_bindings, mark_notification_cleared,
    prune_stale_workspace_bindings, reset_notification_clearance,
};

/// Time the Herdr and git calls behind one notification get, together: the
/// `agent get` that decides whether to show it, and the lookups behind its
/// text. Every one of them is best-effort, so a slow Herdr or a pathological
/// repository costs at most this much delay, and the notification goes out
/// with whatever was answered in time.
const LOOKUP_BUDGET: Duration = Duration::from_secs(2);

pub(crate) fn run<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let action = parse_cli_args(args)?;

    match action {
        CliAction::Help => {
            print_usage();
            return Ok(());
        }
        CliAction::InitConfig => {
            let (path, written) = config::init_config()?;
            if written {
                println!("wrote {}", path.display());
            } else {
                println!("{} already exists; left it unchanged", path.display());
            }
            return Ok(());
        }
        CliAction::Cleanup => {
            cleanup_stale_state_files()
                .map_err(|err| format!("failed to clean stale state files: {err}"))?;
            rewrite_generated_scripts_without_activation()
                .map_err(|err| format!("failed to update generated focus scripts: {err}"))?;
            // Terminal bindings for workspaces that no longer exist are stale
            // too; best-effort, since Herdr may not be reachable.
            if let Ok(herdr_bin) = resolve_herdr_bin() {
                let _ = prune_stale_workspace_bindings(&herdr_bin);
            }
            return Ok(());
        }
        CliAction::ClearTerminalBindings => {
            clear_terminal_bindings()
                .map_err(|err| format!("failed to clear terminal bindings: {err}"))?;
            rewrite_generated_scripts_without_activation()
                .map_err(|err| format!("failed to update generated focus scripts: {err}"))?;
            return Ok(());
        }
        CliAction::FocusPane(pane_id) => {
            return focus::focus_pane(&pane_id);
        }
        CliAction::CheckPaneVisibility(pane_id) => {
            let herdr_bin = resolve_herdr_bin()?;
            let deadline = Instant::now() + LOOKUP_BUDGET;
            let details = pane_details(&pane_id, &herdr_bin, deadline).unwrap_or_default();
            if notification_decision(&pane_id, details.focused) == NotificationDecision::Skip {
                return Ok(());
            }
            return Err("pane is not visible in its workspace's bound terminal".to_string());
        }
        CliAction::Event | CliAction::Test => {}
    }

    let test_mode = action == CliAction::Test;
    let herdr_bin = resolve_herdr_bin()?;
    let config = config::load();

    let event = if test_mode {
        // The test renders the configured templates against the focused pane,
        // as if its agent were blocked, so it shows what a real one would.
        StatusEvent {
            pane_id: focused_pane_id(&herdr_bin).unwrap_or_else(|| "test-pane".to_string()),
            status: "blocked".to_string(),
            agent: Some("dagadbm test".to_string()),
            agent_id: None,
            terminal_title: Some("Click to return to this Herdr pane.".to_string()),
            state_label: None,
        }
    } else {
        let Ok(event_json) = env::var("HERDR_PLUGIN_EVENT_JSON") else {
            return Ok(());
        };

        if env::var("HERDR_PLUGIN_EVENT").as_deref() == Ok("pane.focused") {
            return handle_pane_focused(&event_json, &config, &herdr_bin);
        }

        let Some(event) = status_event_from_json(&event_json)? else {
            return Ok(());
        };
        event
    };

    // One `herdr agent get` answers both questions below: whether the user is
    // already looking at this pane, and what the pane is working on. The skip
    // decision runs before the remaining Herdr and git calls, because a
    // suppressed notification must not pay for them. An unanswered lookup
    // counts as unfocused, so the notification is sent.
    let deadline = Instant::now() + LOOKUP_BUDGET;
    let details = pane_details(&event.pane_id, &herdr_bin, deadline).unwrap_or_default();

    if !test_mode {
        tab_marker::sync_for_status(&config, &event, &details, &herdr_bin, deadline);
        if !config.status_is_enabled(&event.status) {
            return Ok(());
        }
    }

    let mut decision = notification_decision(&event.pane_id, details.focused);
    if decision == NotificationDecision::Skip {
        if test_mode {
            // --test validates the pipeline end to end, so it never
            // suppresses the notification; it just goes without the
            // visibility monitor.
            decision = NotificationDecision::Send;
        } else {
            return Ok(());
        }
    }

    // Resolved before the lookups behind the text, so an install without a
    // notifier fails without paying for them.
    let notifier_bin = resolve_notifier_bin()?;

    let labels = pane_labels(&event.pane_id, details, &herdr_bin, deadline);
    let mut values = token_values(&event, labels, util::hostname());
    if let Some(icon) = config.state_icon(&event.status) {
        values.builtins.insert("state_icon", icon.to_string());
    }
    let notification = render_notification(&event, &config.templates_for(&event.status), &values);

    reset_notification_clearance(&notification.pane_id)
        .map_err(|err| format!("failed to reset notification clearance: {err}"))?;

    let options = AlerterOptions {
        timeout_secs: config.timeout_secs,
        sound: config.sound.clone(),
    };
    let script_path = write_focus_script(
        &notification,
        &herdr_bin,
        &notifier_bin,
        &options,
        decision == NotificationDecision::SendWithVisibilityMonitor,
        test_mode,
    )
    .map_err(|err| format!("failed to write focus script: {err}"))?;

    send_notification(&script_path, test_mode)
        .map_err(|err| format!("failed to send notification: {err}"))?;

    Ok(())
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
        remove_notification(&pane_id, &notifier_bin)
            .map_err(|err| format!("failed to remove notification: {err}"))?;
    }

    Ok(())
}
