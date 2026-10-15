//! WezTerm: a Herdr client's container is the pane in `WEZTERM_PANE`.
//!
//! `wezterm cli activate-pane` switches to that pane's tab. The `wezterm`
//! binary sits next to the GUI binary in `WEZTERM_EXECUTABLE_DIR`, and the
//! CLI finds the running GUI through its default socket.

use std::path::Path;

use super::{FocusCommand, HerdrClient, TerminalAdapter};

pub(super) struct Wezterm;

impl TerminalAdapter for Wezterm {
    fn bundle_id(&self) -> &'static str {
        "com.github.wez.wezterm"
    }

    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand> {
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

    const DIR: (&str, &str) = (
        "WEZTERM_EXECUTABLE_DIR",
        "/Applications/WezTerm.app/Contents/MacOS",
    );

    #[test]
    fn activates_the_client_wezterm_pane() {
        let client = HerdrClient::from_pairs(&[("WEZTERM_PANE", "7"), DIR]);
        assert_eq!(
            Wezterm.focus_command(&client),
            Some(FocusCommand::new(
                "/Applications/WezTerm.app/Contents/MacOS/wezterm",
                ["cli", "activate-pane", "--pane-id", "7"],
            ))
        );
    }

    #[test]
    fn requires_a_numeric_pane_id_and_the_executable_dir() {
        assert_eq!(
            Wezterm.focus_command(&HerdrClient::from_pairs(&[("WEZTERM_PANE", "7")])),
            None
        );
        assert_eq!(
            Wezterm.focus_command(&HerdrClient::from_pairs(&[("WEZTERM_PANE", "x"), DIR])),
            None
        );
        assert_eq!(
            Wezterm.focus_command(&HerdrClient::from_pairs(&[DIR])),
            None
        );
    }
}
