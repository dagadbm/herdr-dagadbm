//! Ghostty: a Herdr client's container is the terminal whose working
//! directory is the client's.
//!
//! Ghostty (1.3 and later) exposes no terminal id to the processes it runs, so
//! the adapter matches on the client's working directory. AppleScript's
//! `focus` selects that terminal's tab and brings its window forward. When
//! several terminals share a directory, the first match wins.

use super::{FocusCommand, HerdrClient, TerminalAdapter};

pub(super) struct Ghostty;

impl TerminalAdapter for Ghostty {
    fn bundle_id(&self) -> &'static str {
        "com.mitchellh.ghostty"
    }

    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand> {
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

    #[test]
    fn focuses_the_terminal_in_the_client_directory() {
        let client = HerdrClient::from_pairs(&[]).with_cwd("/Users/me/code");
        let command = Ghostty.focus_command(&client).expect("command");
        assert_eq!(command.program, "/usr/bin/osascript");
        let script = command.args[1].to_string_lossy();
        assert!(script.contains("whose working directory is \"/Users/me/code\""));
        assert!(script.contains("focus (item 1 of matches)"));
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_the_directory() {
        let client = HerdrClient::from_pairs(&[]).with_cwd("/tmp/a\"b\\c");
        let command = Ghostty.focus_command(&client).expect("command");
        assert!(command.args[1]
            .to_string_lossy()
            .contains("is \"/tmp/a\\\"b\\\\c\""));
    }

    #[test]
    fn needs_a_plain_working_directory() {
        assert_eq!(Ghostty.focus_command(&HerdrClient::from_pairs(&[])), None);
        let client = HerdrClient::from_pairs(&[]).with_cwd("/tmp/a\nb");
        assert_eq!(Ghostty.focus_command(&client), None);
    }
}
