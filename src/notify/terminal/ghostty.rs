//! Ghostty: a Herdr client's container is the Ghostty terminal whose working
//! directory is the client's.
//!
//! Ghostty exposes no terminal id to the processes it runs, so the adapter
//! matches on the client's working directory. AppleScript's `focus` switches to
//! that terminal's tab and brings its window forward; when several terminals
//! share the directory, the first match wins. It needs Ghostty 1.3 or later,
//! whose AppleScript dictionary has `terminal` and `focus`.

use super::{FocusCommand, HerdrClient, TerminalAdapter};

pub(super) struct Ghostty;

impl TerminalAdapter for Ghostty {
    fn bundle_id(&self) -> &'static str {
        "com.mitchellh.ghostty"
    }

    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand> {
        // Matching by directory would focus an unrelated Ghostty terminal for a
        // client that runs in another terminal.
        if client
            .env("TERM_PROGRAM")
            .is_some_and(|program| program != "ghostty")
        {
            return None;
        }
        let cwd = client
            .cwd
            .as_deref()
            .filter(|cwd| !cwd.chars().any(char::is_control))?;
        let cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
        // `error` makes osascript fail when nothing matches, so the core tries
        // other clients instead of treating the click as handled.
        let script = format!(
            "tell application \"Ghostty\"\n\
             set matches to every terminal whose working directory is \"{cwd}\"\n\
             if (count of matches) is 0 then error \"no matching terminal\"\n\
             focus (item 1 of matches)\n\
             end tell"
        );
        Some(FocusCommand::new("/usr/bin/osascript", ["-e", &script]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GHOSTTY: (&str, &str) = ("TERM_PROGRAM", "ghostty");

    fn client(cwd: Option<&str>, pairs: &[(&str, &str)]) -> HerdrClient {
        let client = HerdrClient::from_pairs(pairs);
        match cwd {
            Some(cwd) => client.with_cwd(cwd),
            None => client,
        }
    }

    fn script(command: &FocusCommand) -> String {
        command.args[1].to_string_lossy().into_owned()
    }

    #[test]
    fn focuses_the_ghostty_terminal_in_the_client_directory() {
        let command = Ghostty
            .focus_command(&client(Some("/Users/me/code"), &[GHOSTTY]))
            .expect("focus command");
        assert_eq!(command.program, "/usr/bin/osascript");
        assert_eq!(command.args[0], "-e");
        assert!(script(&command).contains("whose working directory is \"/Users/me/code\""));
        assert!(script(&command).contains("focus (item 1 of matches)"));
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_the_directory() {
        let command = Ghostty
            .focus_command(&client(Some("/tmp/a\"b\\c"), &[GHOSTTY]))
            .expect("focus command");
        assert!(script(&command).contains("is \"/tmp/a\\\"b\\\\c\""));
    }

    #[test]
    fn requires_a_plain_working_directory() {
        assert_eq!(Ghostty.focus_command(&client(None, &[GHOSTTY])), None);
        assert_eq!(
            Ghostty.focus_command(&client(Some("/tmp/a\nb"), &[GHOSTTY])),
            None
        );
    }

    #[test]
    fn skips_clients_of_other_terminals() {
        assert!(Ghostty
            .focus_command(&client(Some("/Users/me/code"), &[]))
            .is_some());
        assert_eq!(
            Ghostty.focus_command(&client(
                Some("/Users/me/code"),
                &[("TERM_PROGRAM", "WezTerm")]
            )),
            None
        );
    }
}
