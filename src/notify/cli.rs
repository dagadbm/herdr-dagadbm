#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
    Event,
    Test,
    Help,
    InitConfig,
    Cleanup,
    ClearTerminalBindings,
    CheckPaneVisibility(String),
    FocusPane(String),
    /// How a notification ended, reported by its generated script with the
    /// pane and the posting.
    NotificationEnded(Outcome, String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Clicked,
    Closed,
    Ended,
}

/// Parses the arguments after `dagadbm notify`.
pub(crate) fn parse_cli_args<I, S>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut action = CliAction::Event;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        match arg {
            "--test" => set_action(&mut action, CliAction::Test, arg)?,
            "-h" | "--help" => set_action(&mut action, CliAction::Help, arg)?,
            "--init-config" => set_action(&mut action, CliAction::InitConfig, arg)?,
            "--cleanup" => set_action(&mut action, CliAction::Cleanup, arg)?,
            "--clear-terminal-bindings" => {
                set_action(&mut action, CliAction::ClearTerminalBindings, arg)?
            }
            "--check-pane-visibility" | "--focus-pane" => {
                if action != CliAction::Event {
                    return Err(format!("cannot combine {arg} with another command"));
                }
                let pane_id = args
                    .next()
                    .ok_or_else(|| format!("missing pane ID after {arg}"))?;
                action = if arg == "--focus-pane" {
                    CliAction::FocusPane(pane_id.as_ref().to_string())
                } else {
                    CliAction::CheckPaneVisibility(pane_id.as_ref().to_string())
                };
            }
            "--notification-clicked" | "--notification-closed" | "--notification-ended" => {
                if action != CliAction::Event {
                    return Err(format!("cannot combine {arg} with another command"));
                }
                let outcome = match arg {
                    "--notification-clicked" => Outcome::Clicked,
                    "--notification-closed" => Outcome::Closed,
                    _ => Outcome::Ended,
                };
                let pane_id = args
                    .next()
                    .ok_or_else(|| format!("missing pane ID after {arg}"))?;
                let post_id = args
                    .next()
                    .ok_or_else(|| format!("missing posting ID after {arg}"))?;
                action = CliAction::NotificationEnded(
                    outcome,
                    pane_id.as_ref().to_string(),
                    post_id.as_ref().to_string(),
                );
            }
            _ => {
                return Err(format!(
                    "unknown argument: {arg}; run `dagadbm notify --help` for usage"
                ));
            }
        }
    }

    Ok(action)
}

fn set_action(action: &mut CliAction, next: CliAction, arg: &str) -> Result<(), String> {
    if *action != CliAction::Event {
        return Err(format!("cannot combine {arg} with another command"));
    }

    *action = next;
    Ok(())
}

pub(crate) fn print_usage() {
    println!(
        "dagadbm notify {}\n\nUsage:\n  dagadbm notify\n  dagadbm notify --test\n  dagadbm notify --init-config\n  dagadbm notify --cleanup\n  dagadbm notify --clear-terminal-bindings\n\nOptions:\n  --test                    Send a test notification for the focused pane\n  --init-config             Write a commented config with every default\n  --cleanup                 Remove stale generated state files\n  --clear-terminal-bindings Remove all saved workspace-terminal bindings\n  -h, --help                Show this help",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_actions() {
        assert_eq!(
            parse_cli_args(Vec::<&str>::new()).unwrap(),
            CliAction::Event
        );
        assert_eq!(parse_cli_args(["--test"]).unwrap(), CliAction::Test);
        assert_eq!(parse_cli_args(["--help"]).unwrap(), CliAction::Help);
        assert_eq!(parse_cli_args(["-h"]).unwrap(), CliAction::Help);
        assert_eq!(
            parse_cli_args(["--init-config"]).unwrap(),
            CliAction::InitConfig
        );
        assert_eq!(parse_cli_args(["--cleanup"]).unwrap(), CliAction::Cleanup);
        assert_eq!(
            parse_cli_args(["--clear-terminal-bindings"]).unwrap(),
            CliAction::ClearTerminalBindings
        );
    }

    #[test]
    fn parses_pane_arguments() {
        assert_eq!(
            parse_cli_args(["--check-pane-visibility", "w1:p2"]).unwrap(),
            CliAction::CheckPaneVisibility("w1:p2".to_string())
        );
        assert_eq!(
            parse_cli_args(["--notification-clicked", "w1:p2", "1-2-3"]).unwrap(),
            CliAction::NotificationEnded(
                Outcome::Clicked,
                "w1:p2".to_string(),
                "1-2-3".to_string()
            )
        );
        assert_eq!(
            parse_cli_args(["--notification-closed", "w1:p2", "1-2-3"]).unwrap(),
            CliAction::NotificationEnded(Outcome::Closed, "w1:p2".to_string(), "1-2-3".to_string())
        );
        assert_eq!(
            parse_cli_args(["--notification-ended", "w1:p2", "1-2-3"]).unwrap(),
            CliAction::NotificationEnded(Outcome::Ended, "w1:p2".to_string(), "1-2-3".to_string())
        );
    }

    #[test]
    fn rejects_unknown_or_combined_cli_args() {
        assert!(parse_cli_args(["--wat"]).is_err());
        assert!(parse_cli_args(["--test", "--help"]).is_err());
        assert!(parse_cli_args(["--check-pane-visibility"]).is_err());
        assert!(parse_cli_args(["--notification-closed", "w1:p2"]).is_err());
        assert!(parse_cli_args(["--test", "--notification-ended", "w1:p2", "1"]).is_err());
    }
}
