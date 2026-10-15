//! Whether the pane's directory has changed versus `HEAD`, for `{git_status}`.
//!
//! The branch next to it comes from Herdr, not from here: see
//! `focus::worktree_branch`.

use std::time::Instant;

use crate::notify::util::command_stdout_until;

/// Whether the directory has tracked changes versus `HEAD`. False when the tree
/// is clean, or when git cannot answer by `deadline`, which is the same
/// best-effort contract the rest of the lookups follow.
pub(crate) fn is_dirty(cwd: &str, deadline: Instant) -> bool {
    // `--no-optional-locks` keeps the probe from refreshing the index. Git
    // takes `.git/index.lock` to do that, and a notification fires exactly when
    // an agent is likely to be running git in the same repository.
    command_stdout_until(
        "git",
        &[
            "--no-optional-locks",
            "-C",
            cwd,
            "diff",
            "--shortstat",
            "HEAD",
        ],
        &[("LC_ALL", "C")],
        deadline,
    )
    .is_some_and(|shortstat| shortstat_is_dirty(&shortstat))
}

/// A clean tree prints nothing at all, so any summary line means the tree is
/// dirty, even ` 1 file changed, 0 insertions(+), 0 deletions(-)` for a binary
/// file.
fn shortstat_is_dirty(shortstat: &str) -> bool {
    !shortstat.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_summary_line_means_a_dirty_tree() {
        assert!(shortstat_is_dirty(
            " 3 files changed, 12 insertions(+), 4 deletions(-)\n"
        ));
        assert!(shortstat_is_dirty(" 1 file changed\n"));
        assert!(!shortstat_is_dirty(""));
        assert!(!shortstat_is_dirty("\n"));
    }
}
