//! WezTerm: a Herdr client's container is the WezTerm pane in `WEZTERM_PANE`.
//!
//! `wezterm cli activate-pane` switches to that pane's tab. It needs no
//! configuration: the `wezterm` binary sits next to the GUI binary in
//! `WEZTERM_EXECUTABLE_DIR`, and the CLI reaches the running GUI through its
//! default socket.

use std::path::Path;

use super::{FocusCommand, HerdrClient, TerminalAdapter};

pub(super) struct Wezterm;

impl TerminalAdapter for Wezterm {
    fn bundle_id(&self) -> &'static str {
        "com.github.wez.wezterm"
    }

    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand> {
        // A client started from a WezTerm shell in another terminal inherits
        // `WEZTERM_PANE`, and that pane may still exist, so activating it would
        // succeed on the wrong pane and stop the core from trying other clients.
        if client
            .env("TERM_PROGRAM")
            .is_some_and(|program| program != "WezTerm")
        {
            return None;
        }
        let pane_id = client
            .env("WEZTERM_PANE")
            .filter(|id| id.bytes().all(|b| b.is_ascii_digit()))?;
        let wezterm = Path::new(client.env("WEZTERM_EXECUTABLE_DIR")?).join("wezterm");
        Some(FocusCommand::new(
            wezterm,
            ["cli", "activate-pane", "--pane-id", pane_id],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXECUTABLE_DIR: (&str, &str) = (
        "WEZTERM_EXECUTABLE_DIR",
        "/Applications/WezTerm.app/Contents/MacOS",
    );

    fn client(pairs: &[(&str, &str)]) -> HerdrClient {
        HerdrClient::from_pairs(pairs)
    }

    #[test]
    fn focuses_the_client_wezterm_pane() {
        let command = Wezterm.focus_command(&client(&[
            ("WEZTERM_PANE", "7"),
            ("TERM_PROGRAM", "WezTerm"),
            EXECUTABLE_DIR,
        ]));
        assert_eq!(
            command,
            Some(FocusCommand::new(
                "/Applications/WezTerm.app/Contents/MacOS/wezterm",
                ["cli", "activate-pane", "--pane-id", "7"],
            ))
        );
    }

    #[test]
    fn requires_the_executable_dir_and_a_numeric_pane_id() {
        assert_eq!(
            Wezterm.focus_command(&client(&[("WEZTERM_PANE", "7")])),
            None
        );
        assert_eq!(
            Wezterm.focus_command(&client(&[("WEZTERM_PANE", "7; rm"), EXECUTABLE_DIR])),
            None
        );
        assert_eq!(Wezterm.focus_command(&client(&[EXECUTABLE_DIR])), None);
    }

    #[test]
    fn skips_clients_of_other_terminals_with_an_inherited_pane_id() {
        let pane = ("WEZTERM_PANE", "7");
        assert_eq!(
            Wezterm.focus_command(&client(&[
                pane,
                EXECUTABLE_DIR,
                ("TERM_PROGRAM", "iTerm.app")
            ])),
            None
        );
    }
}
