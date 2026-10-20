//! Every wait the notify feature does, in one place, so the integration tests
//! can run the real binary with short ones (`--integration-tests`).

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timings {
    /// How long a click counts as recent: a close inside it is alerter closing
    /// the other notifications along with the clicked one (ADR 12). It is also
    /// how long a click waits before it posts those again.
    pub(crate) click_window: Duration,
    /// How long a close waits before it reads the click marker. A click and
    /// the close it causes report at about the same moment, and the click's
    /// stamp has to land first.
    pub(crate) close_settle: Duration,
    /// Time the Herdr and git calls behind one notification get, together: the
    /// `agent get` that decides whether to show it, and the lookups behind its
    /// text. Every one of them is best-effort, so a slow Herdr or a
    /// pathological repository costs at most this much delay, and the
    /// notification goes out with whatever was answered in time.
    pub(crate) lookup_budget: Duration,
    /// How long a notification click waits for Herdr's pane focus response. A
    /// real click runs detached, where an unanswered request would leave a
    /// stray process behind; `--test` runs in the foreground and would hang the
    /// action outright.
    pub(crate) focus_socket_timeout: Duration,
    /// How often a notification's visibility monitor checks its pane.
    pub(crate) monitor_poll: Duration,
    /// Set by `--integration-tests`. The generated scripts pass the flag on to
    /// the commands they run, so those use the same short waits.
    pub(crate) integration_tests: bool,
}

impl Timings {
    pub(crate) const fn production() -> Self {
        Self {
            click_window: Duration::from_secs(1),
            close_settle: Duration::from_millis(200),
            lookup_budget: Duration::from_secs(2),
            focus_socket_timeout: Duration::from_secs(5),
            monitor_poll: Duration::from_secs(2),
            integration_tests: false,
        }
    }

    pub(crate) const fn integration_tests() -> Self {
        Self {
            click_window: Duration::from_millis(500),
            close_settle: Duration::from_millis(100),
            lookup_budget: Duration::from_secs(3),
            focus_socket_timeout: Duration::from_millis(500),
            monitor_poll: Duration::from_millis(200),
            integration_tests: true,
        }
    }

    /// The flag to add to a command line so the command uses these timings.
    pub(crate) fn flag(&self) -> &'static str {
        if self.integration_tests {
            " --integration-tests"
        } else {
            ""
        }
    }
}

impl Default for Timings {
    fn default() -> Self {
        Self::production()
    }
}
