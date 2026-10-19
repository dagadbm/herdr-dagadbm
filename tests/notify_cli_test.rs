use std::process::Command;

/// The sub-plugin name every runtime name starts with (ADR 13).
const PREFIX: &str = "herdr-dagadbm-notify";
const TERMINAL_MEMORY: &str = "herdr-dagadbm-notify-terminal-memory.json";

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::herdr_server::{HerdrServer, Workspace};

/// The default icon of the `[notify.<section>]` marker, read from the example config.
#[cfg(unix)]
fn default_icon(section: &str) -> String {
    let config: toml::Table = include_str!("../config.example.toml").parse().unwrap();
    config["notify"][section]["icon"]
        .as_str()
        .unwrap()
        .to_string()
}

#[cfg(unix)]
fn tab_icon() -> String {
    default_icon("tab_marker")
}

#[cfg(unix)]
fn workspace_icon() -> String {
    default_icon("workspace_marker")
}

#[cfg(unix)]
fn agent_icon() -> String {
    default_icon("agent_marker")
}

/// `label` with `icon` in front, as a marker puts it.
#[cfg(unix)]
fn marked(icon: &str, label: &str) -> String {
    format!("{icon} {label}")
}

/// The alerter group of a pane's notification.
#[cfg(unix)]
fn group(pane: &str) -> String {
    format!("{PREFIX}-{}", pane.replace(':', "-"))
}

#[cfg(unix)]
fn repost() -> String {
    format!("{PREFIX}.repost")
}

#[cfg(unix)]
fn focused_event(pane: &str) -> String {
    format!(r#"{{"event":"pane.focused","data":{{"pane_id":"{pane}"}}}}"#)
}

/// Runs the real `git` to set up a repository. Only test setup does this; the
/// plugin's own `git` calls go to a stub.
#[cfg(unix)]
fn real_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        // The user's git config could sign commits or prompt; ignore it.
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(std::process::Stdio::null())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dagadbm"));
    // The short waits of `Timings::integration_tests`. The generated scripts
    // pass the flag on to the commands they run.
    command.args(["notify", "--integration-tests"]);
    command
}

#[test]
fn help_and_version_print_to_stdout() {
    let help = binary().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    assert!(help.stderr.is_empty());

    let version = Command::new(env!("CARGO_BIN_EXE_dagadbm"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
    assert!(version.stderr.is_empty());
}

#[test]
fn no_event_is_quiet_without_any_output() {
    let output = binary().output().unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn test_mode_reports_bad_configured_herdr_binary() {
    let output = binary()
        .arg("--test")
        .env("HERDR_BIN_PATH", "/definitely/missing-herdr")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("configured Herdr binary"));
}

#[cfg(unix)]
#[test]
fn focus_event_removes_notification_for_foreground_terminal() {
    let temp_dir = temp_test_dir();
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let pane = &workspace.pane_id;

    // A blocked agent whose tab carries the marker, so the focus has
    // something to restore in the real Herdr.
    herdr.herdr_quiet(&[
        "pane",
        "report-agent",
        pane,
        "--source",
        "test",
        "--agent",
        "claude",
        "--state",
        "blocked",
    ]);
    herdr.herdr(&[
        "tab",
        "rename",
        &workspace.tab_id,
        &marked(&tab_icon(), "work"),
    ]);

    write_fixed_frontmost(&temp_dir, "com.example.terminal");

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(r#"{{"event":"pane.focused","data":{{"pane_id":"{pane}"}}}}"#),
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&notifier_log).unwrap(),
        format!("--remove\n{PREFIX}-{}\n", pane.replace(':', "-"))
    );
    let tab = herdr.herdr(&["tab", "get", &workspace.tab_id]);
    assert_eq!(tab["result"]["tab"]["label"], "work");
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn focus_event_learns_terminal_from_cfbundleidentifier() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");

    write_executable(
        &temp_dir.join("lsappinfo"),
        "#!/bin/sh\ncase \"$1\" in\n  front) printf '%s\\n' 'ASN:0x0-0x1:' ;;\n  *) printf '%s\\n' '\"CFBundleIdentifier\"=\"com.mitchellh.ghostty\"' ;;\nesac\n",
    );
    write_executable(&temp_dir.join("alerter"), "#!/bin/sh\nexit 0\n");
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();

    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(
                r#"{{"event":"pane.focused","data":{{"pane_id":"{}"}}}}"#,
                workspace.pane_id
            ),
        )
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(state_dir.join(TERMINAL_MEMORY)).unwrap(),
        format!(
            r#"{{"workspaces":{{"{}":"com.mitchellh.ghostty"}}}}"#,
            workspace.workspace_id
        )
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn visible_focused_pane_removes_its_pending_notification() {
    let temp_dir = temp_test_dir();

    let frontmost_state = temp_dir.join("frontmost-bundle-id");

    write_state_frontmost(&temp_dir);

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let pane = &workspace.pane_id;
    herdr.report_agent(pane, "claude", "blocked");
    let _focus = herdr.focus(&workspace);

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFIER_LOG\"\nif [ \"$1\" = \"--remove\" ]; then\n  touch \"$REMOVE_SIGNAL\"\n  exit 0\nfi\nfor _ in 1 2 3 4 5; do\n  [ -e \"$REMOVE_SIGNAL\" ] && exit 0\n  sleep 1\ndone\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let remove_signal = temp_dir.join("removed");
    let path = path_with_temp_dir(&temp_dir);

    // Zero configuration: establish the workspace binding first via a real
    // pane.focused event while the fake frontmost app is the terminal.
    fs::write(&frontmost_state, "com.example.terminal\n").unwrap();
    let mut learn = binary();
    herdr.connect(&mut learn);
    let learn = learn
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env("HERDR_PLUGIN_EVENT_JSON", focused_event(pane))
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(learn.status.success());

    // Now the user is elsewhere; the pending notification should auto-remove
    // once they switch back to the bound terminal.
    fs::write(&frontmost_state, "com.example.other\n").unwrap();

    let mut child = binary();
    herdr.connect(&mut child);
    let child = child
        .arg("--test")
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("NOTIFIER_LOG", &notifier_log)
        .env("REMOVE_SIGNAL", &remove_signal)
        .env("PATH", path)
        .spawn()
        .unwrap();

    for _ in 0..500 {
        if notifier_log.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    fs::write(&frontmost_state, "com.example.terminal\n").unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let notifier_output = fs::read_to_string(&notifier_log).unwrap_or_default();
    assert!(notifier_output.contains(&format!("--remove\n{}\n", group(pane))));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn test_mode_notifies_even_when_pane_is_visible_and_status_filtered() {
    let temp_dir = temp_test_dir();

    // The frontmost app is the configured terminal, so the pane counts as
    // visible and a normal event would be skipped.
    write_fixed_frontmost(&temp_dir, "com.example.terminal");

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    herdr.report_agent(&workspace.pane_id, "claude", "blocked");
    let _focus = herdr.focus(&workspace);

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .arg("--test")
        // A filter that excludes the hardcoded test status must not suppress
        // a test notification either.
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(fs::read_to_string(&notifier_log)
        .unwrap_or_default()
        .contains("--title"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn normal_notification_names_the_pane_and_never_requests_an_explanation() {
    let temp_dir = temp_test_dir();

    // A real repository with a linked worktree on `feature/api`. Herdr's
    // `worktree list` reads it, so the pane works inside the worktree.
    let repo = temp_dir.join("repo");
    let worktree = temp_dir.join("api");
    fs::create_dir(&repo).unwrap();
    real_git(&repo, &["init", "-q", "-b", "main"]);
    real_git(&repo, &["commit", "-q", "--allow-empty", "-m", "initial"]);
    real_git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/api",
            worktree.to_str().unwrap(),
        ],
    );
    fs::create_dir(worktree.join("src")).unwrap();

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace_in("sample-repo", &worktree.join("src"));
    let pane = &workspace.pane_id;
    herdr.rename_tab(&workspace.tab_id, "status");
    herdr.report_agent(pane, "codex", "blocked");
    herdr.set_terminal_title(pane, "◐ Tidy up the parser tests");
    let logging_herdr = herdr.logging_herdr(&temp_dir);

    // The git probe is faked as well, so the test can hold the two properties
    // that keep it from interfering with an agent working in the same
    // repository: it never takes the index lock, and it parses a summary line
    // git is forced to print in English.
    let git = temp_dir.join("git");
    write_executable(
        &git,
        "#!/bin/sh\nprintf '%s LC_ALL=%s\\n' \"$*\" \"$LC_ALL\" >> \"$GIT_LOG\"\nprintf ' 2 files changed, 4 insertions(+), 4 deletions(-)\\n'\n",
    );

    // The default subtitle has no branch, so the config adds it back.
    let config_dir = temp_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "[notify]\nsubtitle = \"{agent} · {branch}{git_status}\"\n",
    )
    .unwrap();

    let notifier_log = write_logging_notifier(&temp_dir);
    let herdr_log = temp_dir.join("herdr.log");
    let git_log = temp_dir.join("git.log");
    let path = path_with_temp_dir(&temp_dir);
    let mut command = binary();
    herdr.connect_logged(&mut command, &logging_herdr);
    let output = command
        .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(
                r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"blocked","agent":"Codex","title":"Implement plugin"}}}}"#
            ),
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("HERDR_LOG", &herdr_log)
        .env("GIT_LOG", &git_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The notifier runs in a detached script. The fake notifier renames its
    // log into place, so the file appears only once all arguments are written.
    let notifier_output = wait_for_notifier_output(&notifier_log);

    assert!(
        notifier_output.contains("--title=⌂ sample-repo ▸ status\n"),
        "{notifier_output}"
    );
    // The pane's terminal title is the message; the event's own title is not
    // shown next to it.
    assert!(notifier_output.contains("Tidy up the parser tests"));
    assert!(!notifier_output.contains("Implement plugin"));
    assert!(notifier_output.contains("--subtitle"));
    // The pane sits in a linked worktree, so the branch is that worktree's,
    // not the `main` of the checkout Herdr reports as the source.
    assert!(
        notifier_output.contains("Codex · feature/api*"),
        "{notifier_output}"
    );
    // The dirty check must come from a probe that leaves the index alone and cannot
    // be confused by a translated git.
    let git_log = fs::read_to_string(&git_log).unwrap_or_default();
    assert!(git_log.contains("--no-optional-locks"), "{git_log}");
    assert!(git_log.contains("LC_ALL=C"), "{git_log}");
    assert!(!fs::read_to_string(&herdr_log)
        .unwrap_or_default()
        .contains("explain"));
    wait_for_detached_notifier(&temp_dir.join("state"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_hanging_worktree_lookup_delays_the_notification_by_the_budget_at_most() {
    let temp_dir = temp_test_dir();

    // A pane in a git work tree, so the plugin asks for its worktrees, and
    // `worktree list` never answers in time. Everything else is the real Herdr.
    let repo = temp_dir.join("repo");
    fs::create_dir(&repo).unwrap();
    real_git(&repo, &["init", "-q", "-b", "main"]);
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace_in("sample-repo", &repo);
    let pane = &workspace.pane_id;
    herdr.rename_tab(&workspace.tab_id, "status");
    herdr.report_agent(pane, "codex", "blocked");
    herdr.set_terminal_title(pane, "◐ Tidy up the parser tests");
    let hanging_herdr = herdr.hanging_herdr(&temp_dir, "worktree list");

    let git = temp_dir.join("git");
    write_executable(
        &git,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$GIT_LOG\"\nprintf ' 2 files changed, 4 insertions(+), 4 deletions(-)\\n'\n",
    );

    let notifier_log = write_logging_notifier(&temp_dir);
    let git_log = temp_dir.join("git.log");
    let path = path_with_temp_dir(&temp_dir);
    let started = std::time::Instant::now();
    let mut command = binary();
    herdr.connect_logged(&mut command, &hanging_herdr);
    let output = command
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(
                r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"blocked","agent":"Codex"}}}}"#
            ),
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("GIT_LOG", &git_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    let notifier_output = wait_for_notifier_output(&notifier_log);

    // What answered in time is still shown; the branch is dropped, and with it
    // the git probe, which has no branch to attribute changes to.
    assert!(notifier_output.contains("--message=× Tidy up the parser tests\n"));
    assert!(notifier_output.contains("Tidy up the parser tests"));
    assert!(
        notifier_output.contains("--title=⌂ sample-repo ▸ status\n"),
        "{notifier_output}"
    );
    assert!(!git_log.exists());
    wait_for_detached_notifier(&temp_dir.join("state"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_hanging_agent_lookup_still_sends_the_notification_within_the_budget() {
    let temp_dir = temp_test_dir();

    // `agent get` decides whether to notify at all, and it never answers in
    // time. Every other call would reach the real Herdr, but the budget is
    // spent.
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace_in("sample-repo", Path::new("/tmp"));
    let pane = &workspace.pane_id;
    herdr.report_agent(pane, "codex", "blocked");
    let hanging_herdr = herdr.hanging_herdr(&temp_dir, "agent get");

    let notifier_log = write_logging_notifier(&temp_dir);
    let herdr_log = temp_dir.join("herdr.log");
    let path = path_with_temp_dir(&temp_dir);
    let started = std::time::Instant::now();
    let mut command = binary();
    herdr.connect_logged(&mut command, &hanging_herdr);
    let output = command
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(
                r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"blocked","agent":"Codex"}}}}"#
            ),
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("HERDR_LOG", &herdr_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    let notifier_output = wait_for_notifier_output(&notifier_log);

    // An unknown focus sends rather than skips, with empty tokens for what
    // was not answered; the lookups behind the text are skipped, since the
    // budget is already spent.
    assert!(
        notifier_output.contains("--title=⌂  ▸ \n"),
        "{notifier_output}"
    );
    assert!(
        notifier_output.contains("--message=× \n"),
        "{notifier_output}"
    );
    assert_eq!(
        fs::read_to_string(&herdr_log).unwrap_or_default(),
        format!("agent get {pane}\n")
    );
    wait_for_detached_notifier(&temp_dir.join("state"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn unfocused_pane_does_not_start_a_visibility_monitor() {
    let temp_dir = temp_test_dir();

    let frontmost_state = temp_dir.join("frontmost-bundle-id");
    fs::write(&frontmost_state, "com.example.other\n").unwrap();

    write_state_frontmost(&temp_dir);

    // Another workspace holds the focus while the notification is sent.
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let pane = &workspace.pane_id;
    herdr.report_agent(pane, "claude", "idle");
    let elsewhere = herdr.workspace();
    let focus = herdr.focus(&elsewhere);

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFIER_LOG\"\nif [ \"$1\" = \"--remove\" ]; then\n  exit 0\nfi\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(
                r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"done"}}}}"#
            ),
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("NOTIFIER_LOG", &notifier_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    for _ in 0..500 {
        if notifier_log.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(notifier_log.exists());
    // The script holds no visibility monitor, so nothing watches for the
    // user coming back to the pane.
    wait_for_detached_notifier(&temp_dir.join("state"));
    let script = fs::read_dir(temp_dir.join("state"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "sh"))
        .expect("a generated focus script");
    let script = fs::read_to_string(script).unwrap();
    assert!(!script.contains("--check-pane-visibility"), "{script}");
    drop(focus);
    wait_for_detached_notifier(&temp_dir.join("state"));
    fs::remove_dir_all(temp_dir).unwrap();
}

/// Gives the workspace and its tab `name`, and a blocked agent in its pane.
#[cfg(unix)]
fn name_with_blocked_agent(herdr: &HerdrServer, workspace: &Workspace, name: &str) {
    herdr.rename_workspace(&workspace.workspace_id, name);
    herdr.rename_tab(&workspace.tab_id, name);
    herdr.report_agent(&workspace.pane_id, "claude", "blocked");
}

/// Two workspaces with one tab and one agent each. The user is looking at the
/// first one, in its terminal. Herdr reports each pane's focus, so nothing is
/// remembered about it: the focused agent's event is skipped, and the other
/// agent's event notifies and marks its own workspace only.
#[cfg(unix)]
#[test]
fn an_agent_notifies_for_its_own_pane_only_when_another_workspace_is_focused() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");
    let herdr = HerdrServer::shared();

    let seen = herdr.workspace();
    let away = herdr.workspace();
    name_with_blocked_agent(herdr, &seen, "seen");
    name_with_blocked_agent(herdr, &away, "away");
    let _focus = herdr.focus(&seen);

    write_fixed_frontmost(&temp_dir, "com.example.terminal");
    write_terminal_binding(&state_dir, &seen.workspace_id, "com.example.terminal");
    write_executable(
        &temp_dir.join("alerter"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFIER_LOG\"\n",
    );
    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);

    let blocked = |pane: &str| {
        let mut command = binary();
        herdr.connect(&mut command);
        let output = command
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env(
                "HERDR_PLUGIN_EVENT_JSON",
                format!(
                    r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"blocked","agent":"claude"}}}}"#
                ),
            )
            .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
            .env("NOTIFIER_LOG", &notifier_log)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    blocked(&seen.pane_id);
    wait_for_detached_notifier(&state_dir);
    assert!(
        !notifier_log.exists(),
        "the focused agent notified: {}",
        fs::read_to_string(&notifier_log).unwrap_or_default()
    );
    assert_eq!(herdr.workspace_label(&seen.workspace_id), "seen");
    assert_eq!(herdr.tab_label(&seen.tab_id), "seen");

    blocked(&away.pane_id);
    let notified = wait_for_notifier_output(&notifier_log);
    assert!(notified.contains(&group(&away.pane_id)), "{notified}");
    assert!(!notified.contains(&group(&seen.pane_id)), "{notified}");
    assert_eq!(
        herdr.workspace_label(&away.workspace_id),
        marked(&workspace_icon(), "away")
    );
    assert_eq!(herdr.tab_label(&away.tab_id), marked(&tab_icon(), "away"));
    assert_eq!(herdr.workspace_label(&seen.workspace_id), "seen");
    assert_eq!(herdr.tab_label(&seen.tab_id), "seen");

    wait_for_detached_notifier(&state_dir);
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
fn temp_test_dir() -> PathBuf {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temp_dir = std::env::temp_dir().join(format!(
        "dagadbm-test-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&temp_dir).unwrap();
    temp_dir
}

/// Waits for a detached focus script to remove its notifier temp files. The
/// script writes the notifier status after the fake notifier has logged, so
/// removing the state directory earlier races with that write and fails with
/// `DirectoryNotEmpty`.
#[cfg(unix)]
fn wait_for_detached_notifier(state_dir: &Path) {
    let pending = || {
        fs::read_dir(state_dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.contains(".result.") || name.contains(".status.")
            })
        })
    };
    for _ in 0..500 {
        if !pending() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("detached notifier script did not finish");
}

/// Installs a fake `alerter` that records its arguments, one per line, and
/// returns the log it writes; `NOTIFIER_LOG` has to point the plugin at it.
#[cfg(unix)]
fn write_logging_notifier(temp_dir: &Path) -> PathBuf {
    write_executable(
        &temp_dir.join("alerter"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG.tmp\"\nmv \"$NOTIFIER_LOG.tmp\" \"$NOTIFIER_LOG\"\n",
    );
    temp_dir.join("notifier.log")
}

/// The arguments the detached notifier was called with, once it has run.
#[cfg(unix)]
fn wait_for_notifier_output(notifier_log: &Path) -> String {
    for _ in 0..500 {
        let output = fs::read_to_string(notifier_log).unwrap_or_default();
        if !output.is_empty() {
            return output;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    String::new()
}

#[cfg(unix)]
fn path_with_temp_dir(temp_dir: &Path) -> String {
    format!(
        "{}:{}",
        temp_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

/// Fakes the frontmost-app lookup with a fixed bundle id. Tests cannot drive
/// the real window server, so the shim answers both `lsappinfo` calls the
/// plugin makes: `front` (an ASN) and `info -only bundleID <asn>`.
#[cfg(unix)]
fn write_fixed_frontmost(temp_dir: &Path, bundle_id: &str) {
    write_executable(
        &temp_dir.join("lsappinfo"),
        &format!(
            "#!/bin/sh\ncase \"$1\" in\n  front) printf '%s\\n' 'ASN:0x0-0x1:' ;;\n  *) printf 'bundleID=\"{bundle_id}\"\\n' ;;\nesac\n"
        ),
    );
}

/// Fakes the frontmost-app lookup so it answers from `$FRONTMOST_STATE`,
/// letting a test change the frontmost app while the binary is running.
#[cfg(unix)]
fn write_state_frontmost(temp_dir: &Path) {
    write_executable(
        &temp_dir.join("lsappinfo"),
        "#!/bin/sh\ncase \"$1\" in\n  front) printf '%s\\n' 'ASN:0x0-0x1:' ;;\n  *) printf 'bundleID=\"%s\"\\n' \"$(cat \"$FRONTMOST_STATE\")\" ;;\nesac\n",
    );
}

/// A short `/tmp` socket path. The bound path must stay under `sun_path`'s
/// ~104-byte limit, so it cannot live in the long per-test temp directory.
#[cfg(unix)]
fn unique_socket_path() -> PathBuf {
    static NEXT_SOCKET_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    PathBuf::from(format!(
        "/tmp/dagadbm-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

#[cfg(unix)]
fn focus_socket(expected_pane_id: &str, response: &str) -> (PathBuf, std::thread::JoinHandle<()>) {
    let socket_path = unique_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let cleanup_path = socket_path.clone();
    let expected_pane_id = expected_pane_id.to_string();
    let response = response.to_string();
    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "plugin did not connect to the Herdr socket"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(err) => panic!("failed to accept Herdr socket connection: {err}"),
            }
        };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        let mut request_line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut request_line)
            .unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        assert_eq!(request["id"], "dagadbm:focus");
        assert_eq!(request["method"], "pane.focus");
        assert_eq!(request["params"]["pane_id"], expected_pane_id);

        writeln!(stream, "{response}").unwrap();
        fs::remove_file(cleanup_path).unwrap();
    });
    (socket_path, handle)
}

#[cfg(unix)]
#[test]
fn focus_click_targets_an_arbitrary_pane_through_the_socket_api() {
    let temp_dir = temp_test_dir();
    let open_log = temp_dir.join("open.log");
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let target = herdr.split(&workspace.pane_id);
    write_terminal_binding(
        &temp_dir.join("state"),
        &workspace.workspace_id,
        "com.example.terminal",
    );
    write_executable(
        &temp_dir.join("open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$OPEN_LOG\"\n",
    );
    // The click moves the server's focus, so it holds the focus lock.
    let elsewhere = herdr.workspace();
    let _focus = herdr.focus(&elsewhere);
    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .args(["--focus-pane", &target])
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("OPEN_LOG", &open_log)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(open_log).unwrap(),
        "-b com.example.terminal\n"
    );
    assert_eq!(herdr.focused_pane(), target);
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn focus_click_reports_a_missing_plugin_socket() {
    let temp_dir = temp_test_dir();
    write_terminal_binding(&temp_dir.join("state"), "w2", "com.example.terminal");
    write_executable(&temp_dir.join("open"), "#!/bin/sh\nexit 0\n");

    let output = binary()
        .args(["--focus-pane", "w2:p7"])
        .env_remove("HERDR_SOCKET_PATH")
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HERDR_SOCKET_PATH is unavailable"));
    assert!(!temp_dir
        .join("state/herdr-dagadbm-notify-focus-origin-w2.marker")
        .exists());
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn focus_click_reports_socket_api_failures() {
    let temp_dir = temp_test_dir();
    write_terminal_binding(&temp_dir.join("state"), "w2", "com.example.terminal");
    write_executable(&temp_dir.join("open"), "#!/bin/sh\nexit 0\n");
    for (response, expected) in [
        (
            r#"{"id":"dagadbm:focus","error":{"code":"not_found","message":"pane not found"}}"#,
            "failed to focus pane: pane not found",
        ),
        ("not json", "invalid pane focus response"),
        (
            r#"{"id":"dagadbm:focus","result":{"type":"ok"}}"#,
            "missing pane_info result",
        ),
    ] {
        let (socket_path, socket_handle) = focus_socket("w2:p7", response);
        let output = binary()
            .args(["--focus-pane", "w2:p7"])
            .env("HERDR_SOCKET_PATH", &socket_path)
            .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
            .env("PATH", path_with_temp_dir(&temp_dir))
            .output()
            .unwrap();
        socket_handle.join().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert!(!temp_dir
            .join("state/herdr-dagadbm-notify-focus-origin-w2.marker")
            .exists());
    }
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn notification_content_click_runs_the_focus_helper() {
    let temp_dir = temp_test_dir();
    let notifier_log = temp_dir.join("notifier.log");
    let open_log = temp_dir.join("open.log");
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    herdr.report_agent(&workspace.pane_id, "claude", "blocked");
    write_terminal_binding(
        &temp_dir.join("state"),
        &workspace.workspace_id,
        "com.example.terminal",
    );
    // `--test` notifies for the focused pane, and the click focuses it.
    let _focus = herdr.focus(&workspace);
    write_executable(
        &temp_dir.join("alerter"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\necho '@CONTENTCLICKED'\n",
    );
    write_executable(
        &temp_dir.join("open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$OPEN_LOG\"\n",
    );
    write_executable(&temp_dir.join("lsappinfo"), "#!/bin/sh\nexit 1\n");
    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .arg("--test")
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("OPEN_LOG", &open_log)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(open_log).unwrap(),
        "-b com.example.terminal\n"
    );
    let notifier_args = fs::read_to_string(notifier_log).unwrap();
    assert!(notifier_args.contains("--message="), "{notifier_args}");
    assert!(notifier_args.contains("Click to return to this Herdr pane.\n"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn focus_click_without_terminal_binding_does_not_activate_or_focus() {
    let temp_dir = temp_test_dir();
    let herdr_log = temp_dir.join("herdr.log");
    let open_log = temp_dir.join("open.log");
    write_executable(
        &temp_dir.join("herdr"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HERDR_LOG\"\nexit 1\n",
    );
    write_executable(
        &temp_dir.join("open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$OPEN_LOG\"\nexit 1\n",
    );

    let output = binary()
        .args(["--focus-pane", "w2:p7"])
        .env("HERDR_BIN_PATH", temp_dir.join("missing-herdr"))
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("HERDR_LOG", &herdr_log)
        .env("OPEN_LOG", &open_log)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!herdr_log.exists());
    assert!(!open_log.exists());
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn notification_focus_does_not_overwrite_existing_terminal_binding() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");
    let frontmost_state = temp_dir.join("frontmost");
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let (workspace_id, pane) = (&workspace.workspace_id, &workspace.pane_id);
    write_terminal_binding(&state_dir, workspace_id, "com.mitchellh.ghostty");
    fs::write(&frontmost_state, "com.example.notification-app\n").unwrap();
    write_state_frontmost(&temp_dir);
    write_executable(&temp_dir.join("open"), "#!/bin/sh\nexit 0\n");
    // The click talks to a fake socket, so the test controls its answer.
    let (socket_path, socket_handle) = focus_socket(
        pane,
        &format!(
            r#"{{"id":"dagadbm:focus","result":{{"type":"pane_info","pane":{{"pane_id":"{pane}","workspace_id":"{workspace_id}"}}}}}}"#
        ),
    );

    let focus = binary()
        .args(["--focus-pane", pane])
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    socket_handle.join().unwrap();
    assert!(
        focus.status.success(),
        "{}",
        String::from_utf8_lossy(&focus.stderr)
    );

    // The focus event runs against the real test Herdr server.
    let mut event = binary();
    herdr.connect(&mut event);
    let event = event
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            format!(r#"{{"event":"pane.focused","data":{{"pane_id":"{pane}"}}}}"#),
        )
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    assert!(
        event.status.success(),
        "{}",
        String::from_utf8_lossy(&event.stderr)
    );

    assert_eq!(
        fs::read_to_string(state_dir.join(TERMINAL_MEMORY)).unwrap(),
        format!(r#"{{"workspaces":{{"{workspace_id}":"com.mitchellh.ghostty"}}}}"#)
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn obvious_non_terminal_does_not_overwrite_existing_terminal_binding() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");
    let frontmost_state = temp_dir.join("frontmost");
    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let workspace_id = &workspace.workspace_id;
    write_terminal_binding(&state_dir, workspace_id, "com.mitchellh.ghostty");
    fs::write(&frontmost_state, "com.google.Chrome\n").unwrap();
    write_state_frontmost(&temp_dir);

    let mut command = binary();
    herdr.connect(&mut command);
    let output = command
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env("HERDR_PLUGIN_EVENT_JSON", focused_event(&workspace.pane_id))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(state_dir.join(TERMINAL_MEMORY)).unwrap(),
        format!(r#"{{"workspaces":{{"{workspace_id}":"com.mitchellh.ghostty"}}}}"#)
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn clear_terminal_bindings_removes_old_activation_commands() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");
    write_terminal_binding(&state_dir, "w1", "com.mitchellh.ghostty");
    let script = state_dir.join("herdr-dagadbm-notify-focus-old.sh");
    fs::write(
        &script,
        "#!/bin/sh\n    open -b 'com.google.Chrome' >/dev/null 2>&1\n    exec focus\n",
    )
    .unwrap();

    let output = binary()
        .arg("--clear-terminal-bindings")
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!state_dir.join(TERMINAL_MEMORY).exists());
    assert!(!fs::read_to_string(script).unwrap().contains("open -b"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
fn write_terminal_binding(state_dir: &Path, workspace: &str, bundle_id: &str) {
    fs::create_dir_all(state_dir).unwrap();
    fs::write(
        state_dir.join(TERMINAL_MEMORY),
        format!(r#"{{"workspaces":{{"{workspace}":"{bundle_id}"}}}}"#),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn config_file_selects_statuses_and_templates() {
    let temp_dir = temp_test_dir();

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let pane = &workspace.pane_id;
    herdr.rename_tab(&workspace.tab_id, "status");
    herdr.report_agent(pane, "codex", "working");
    herdr.set_terminal_title(pane, "◐ Tidy up");
    herdr.herdr_quiet(&[
        "pane",
        "report-metadata",
        pane,
        "--source",
        "test",
        "--token",
        "ticket=ABC-1",
    ]);
    let config_dir = temp_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "[notify]\nnotify_on = [\"working\"]\ntitle = \"{agent} on {tab}\"\nsubtitle = \"\"\nmessage = \"{terminal_title} $ticket\"\naction_label = \"Go\"\nsound = \"Ping\"\ntimeout = 30\n",
    )
    .unwrap();

    let notifier_log = write_logging_notifier(&temp_dir);
    let run = |status: &str| {
        let mut command = binary();
        herdr.connect(&mut command);
        command
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env(
                "HERDR_PLUGIN_EVENT_JSON",
                format!(
                    r#"{{"data":{{"pane_id":"{pane}","agent_status":"{status}","agent":"Codex"}}}}"#
                ),
            )
            .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
            .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
            .env("NOTIFIER_LOG", &notifier_log)
            .env("PATH", path_with_temp_dir(&temp_dir))
            .output()
            .unwrap()
    };

    // `blocked` is a default status, but the config replaced the list.
    assert!(run("blocked").status.success());
    assert!(run("working").status.success());
    let notifier_output = wait_for_notifier_output(&notifier_log);

    assert_configured_notification(&notifier_output);
    wait_for_detached_notifier(&temp_dir.join("state"));
    fs::remove_dir_all(temp_dir).unwrap();
}

fn assert_configured_notification(notifier_output: &str) {
    assert!(
        notifier_output.contains("--title=Codex on status\n"),
        "{notifier_output}"
    );
    assert!(notifier_output.contains("--message=◐ Tidy up ABC-1\n"));
    assert!(!notifier_output.contains("--subtitle"));
    // The button is always `Focus`; a legacy `action_label` is ignored.
    assert!(notifier_output.contains("--actions\nFocus\n"));
    assert!(!notifier_output.contains("--close-label"));
    assert!(notifier_output.contains("--sound\nPing\n"));
    assert!(notifier_output.contains("--timeout\n30\n"));
    assert_eq!(notifier_output.matches("--title=").count(), 1);
}

#[cfg(unix)]
#[test]
fn init_config_writes_the_example_once() {
    let temp_dir = temp_test_dir();
    let config_dir = temp_dir.join("config");

    let first = binary()
        .arg("--init-config")
        .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
        .output()
        .unwrap();
    assert!(first.status.success());
    let written = fs::read_to_string(config_dir.join("config.toml")).unwrap();
    assert!(written.contains("[notify]"));

    fs::write(config_dir.join("config.toml"), "# mine\n").unwrap();
    let second = binary()
        .arg("--init-config")
        .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
        .output()
        .unwrap();
    assert!(second.status.success());
    assert_eq!(
        fs::read_to_string(config_dir.join("config.toml")).unwrap(),
        "# mine\n"
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn blocked_agent_marks_tab_and_agent_name_and_working_or_focus_restores_them() {
    let temp_dir = temp_test_dir();

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let (pane, tab) = (&workspace.pane_id, &workspace.tab_id);
    herdr.rename_tab(tab, "status");
    herdr.report_agent(pane, "claude", "blocked");
    let logging_herdr = herdr.logging_herdr(&temp_dir);

    // No status notifies, so only the markers are under test.
    let config_dir = temp_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "[notify]\nnotify_on = []\n").unwrap();

    let notifier_log = write_logging_notifier(&temp_dir);
    let herdr_log = temp_dir.join("herdr.log");
    let path = path_with_temp_dir(&temp_dir);
    let run = |event: &str, json: String| {
        let mut command = binary();
        herdr.connect_logged(&mut command, &logging_herdr);
        let output = command
            .env("HERDR_PLUGIN_EVENT", event)
            .env("HERDR_PLUGIN_EVENT_JSON", json)
            .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
            .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
            .env("NOTIFIER_LOG", &notifier_log)
            .env("HERDR_LOG", &herdr_log)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let log = fs::read_to_string(&herdr_log).unwrap_or_default();
        fs::remove_file(&herdr_log).ok();
        log
    };
    let status_event = |status: &str, agent: &str| {
        format!(
            r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"{status}","agent":"{agent}"}}}}"#
        )
    };

    let log = run(
        "pane.agent_status_changed",
        status_event("blocked", "claude"),
    );
    assert_eq!(herdr.tab_label(tab), marked(&tab_icon(), "status"));
    assert!(
        log.contains(&format!(
            "pane report-metadata {pane} --source dagadbm --display-agent {} claude\n",
            agent_icon()
        )),
        "{log}"
    );

    let log = run(
        "pane.agent_status_changed",
        status_event("working", &marked(&agent_icon(), "claude")),
    );
    assert_eq!(herdr.tab_label(tab), "status");
    assert!(
        log.contains(&format!(
            "pane report-metadata {pane} --source dagadbm --clear-display-agent\n"
        )),
        "{log}"
    );

    herdr.rename_tab(tab, &marked(&tab_icon(), "status"));
    run("pane.focused", focused_event(pane));
    assert_eq!(herdr.tab_label(tab), "status");

    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_repeated_agent_status_notifies_once_and_leaves_herdr_alone() {
    let temp_dir = temp_test_dir();

    let herdr = HerdrServer::shared();
    let workspace = herdr.workspace();
    let p2 = workspace.pane_id.clone();
    let p3 = herdr.split(&p2);
    herdr.report_agent(&p2, "claude", "blocked");
    herdr.report_agent(&p3, "claude", "blocked");
    let logging_herdr = herdr.logging_herdr(&temp_dir);

    // Appends one line per notification, so repeats are countable.
    let notifier_log = temp_dir.join("notifier.log");
    write_executable(
        &temp_dir.join("alerter"),
        "#!/bin/sh\nfor arg in \"$@\"; do\n  case \"$arg\" in herdr-dagadbm-notify-*) printf '%s\\n' \"$arg\" >> \"$NOTIFIER_LOG\";; esac\ndone\n",
    );

    let state_dir = temp_dir.join("state");
    let herdr_log = temp_dir.join("herdr.log");
    let path = path_with_temp_dir(&temp_dir);
    let sent = std::cell::Cell::new(0);
    // Runs one status event and returns the Herdr calls it made, after waiting
    // for the notifications it is expected to have sent so far.
    let run = |pane: &str, status: &str, agent: &str, notifies: bool| {
        let json = format!(
            r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"{status}","agent":"{agent}"}}}}"#
        );
        let mut command = binary();
        herdr.connect_logged(&mut command, &logging_herdr);
        let output = command
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env("HERDR_PLUGIN_EVENT_JSON", json)
            .env("HERDR_PLUGIN_CONFIG_DIR", temp_dir.join("config"))
            .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
            .env("NOTIFIER_LOG", &notifier_log)
            .env("HERDR_LOG", &herdr_log)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if notifies {
            sent.set(sent.get() + 1);
        }
        for _ in 0..500 {
            let lines = fs::read_to_string(&notifier_log)
                .unwrap_or_default()
                .lines()
                .count();
            if lines >= sent.get() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        wait_for_detached_notifier(&state_dir);
        let log = fs::read_to_string(&herdr_log).unwrap_or_default();
        fs::remove_file(&herdr_log).ok();
        log
    };

    let log = run(&p2, "blocked", "claude", true);
    assert!(
        log.contains(&format!("--display-agent {} claude\n", agent_icon())),
        "{log}"
    );

    // The echo of the agent marker: same status, marked agent name.
    let log = run(&p2, "blocked", &marked(&agent_icon(), "claude"), false);
    assert_eq!(log, "", "a repeat must not call Herdr");

    // Another pane keeps its own last status.
    run(&p3, "blocked", "claude", true);

    // A real change, and a return to blocked, each notify again.
    run(&p2, "working", &marked(&agent_icon(), "claude"), false);
    run(&p2, "blocked", "claude", true);

    // Give a wrongly sent extra notification time to show up.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let notified = fs::read_to_string(&notifier_log).unwrap_or_default();
    assert_eq!(
        notified.lines().collect::<Vec<_>>(),
        [group(&p2), group(&p3), group(&p2)],
        "{notified}"
    );

    fs::remove_dir_all(temp_dir).unwrap();
}

/// A Herdr session with notifications the test answers. It is a workspace
/// `repo` on the test Herdr server, with one tab labelled `status`. Tests name
/// panes `w1:p1`, `w1:p2` and so on; the first name used is the workspace's
/// pane, and each new name splits off another. Every pane is an unfocused
/// agent. The fake `alerter` logs each notification as one line of
/// `|`-terminated arguments, then waits for the test to write what alerter
/// would print into `answer-<group>`; `--list` prints the file `shown`.
#[cfg(unix)]
struct Session {
    dir: PathBuf,
    herdr: &'static HerdrServer,
    workspace: support::herdr_server::Workspace,
    logging_herdr: PathBuf,
    panes: std::cell::RefCell<std::collections::HashMap<String, String>>,
}

#[cfg(unix)]
impl Session {
    fn new(config: &str) -> Self {
        let dir = temp_test_dir();
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::write(dir.join("config/config.toml"), config).unwrap();
        let herdr = HerdrServer::shared();
        let workspace = herdr.workspace_in("repo", Path::new("/tmp"));
        herdr.rename_tab(&workspace.tab_id, "status");
        let logging_herdr = herdr.logging_herdr(&dir);
        write_executable(
            &dir.join("alerter"),
            r#"#!/bin/sh
case "$1" in
  --list)
    # A failing list: garbage on stdout and a non-zero exit.
    if [ -e "$STUB_DIR/list-fails" ]; then echo 'alerter: list failed'; exit 1; fi
    cat "$STUB_DIR/shown" 2>/dev/null; exit 0 ;;
  --remove) exit 0 ;;
esac
group=
prev=
for arg in "$@"; do
  [ "$prev" = "--group" ] && group=$arg
  prev=$arg
done
line=$(printf '%s|' "$@")
printf '%s\n' "$line" >> "$NOTIFIER_LOG"
answer="$STUB_DIR/answer-$group"
i=0
while [ ! -e "$answer" ] && [ "$i" -lt 300 ]; do
  sleep 0.05
  i=$((i + 1))
done
if [ -e "$answer" ]; then
  reply=$(cat "$answer")
  rm -f "$answer"
  # `@EXIT<n>`: alerter fails with status n and prints nothing.
  case "$reply" in @EXIT*) exit "${reply#@EXIT}" ;; esac
  printf '%s' "$reply"
fi
"#,
        );
        // The frontmost app comes from the file `frontmost`, so the real
        // lsappinfo never runs; `open` only logs.
        fs::write(dir.join("frontmost"), "com.example.other").unwrap();
        write_executable(
            &dir.join("lsappinfo"),
            "#!/bin/sh\ncase \"$1\" in\n  front) printf '%s\\n' 'ASN:0x0-0x1:' ;;\n  *) printf 'bundleID=\"%s\"\\n' \"$(cat \"$STUB_DIR/frontmost\")\" ;;\nesac\n",
        );
        write_executable(
            &dir.join("open"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$STUB_DIR/open.log\"\n",
        );
        Self {
            dir,
            herdr,
            workspace,
            logging_herdr,
            panes: Default::default(),
        }
    }

    /// The real pane ID behind a test's pane name, created on first use.
    fn pane(&self, name: &str) -> String {
        if let Some(pane) = self.panes.borrow().get(name) {
            return pane.clone();
        }
        let pane = if self.panes.borrow().is_empty() {
            self.workspace.pane_id.clone()
        } else {
            self.herdr.split(&self.workspace.pane_id)
        };
        self.herdr.report_agent(&pane, "codex", "blocked");
        self.panes
            .borrow_mut()
            .insert(name.to_string(), pane.clone());
        pane
    }

    fn workspace_id(&self) -> &str {
        &self.workspace.workspace_id
    }

    fn state_dir(&self) -> PathBuf {
        self.dir.join("state")
    }

    /// `dagadbm notify` in this session, with every stub on `PATH`.
    fn command(&self) -> Command {
        let mut command = binary();
        self.herdr.connect_logged(&mut command, &self.logging_herdr);
        command
            // A click would move the shared server's focus at any time, so
            // the click socket stays unset. The `herdr` wrapper still finds
            // the test server.
            .env_remove("HERDR_SOCKET_PATH")
            .env("HERDR_PLUGIN_STATE_DIR", self.state_dir())
            .env("HERDR_PLUGIN_CONFIG_DIR", self.dir.join("config"))
            .env("NOTIFIER_LOG", self.dir.join("notifier.log"))
            .env("STUB_DIR", &self.dir)
            .env("PATH", path_with_temp_dir(&self.dir));
        command
    }

    /// Runs `--notification-<outcome> <pane> <post_id>`, as a generated
    /// script reports how its notification ended.
    fn report(&self, outcome: &str, pane: &str, post_id: &str) -> std::process::Output {
        let pane = self.pane(pane);
        self.command()
            .args([&format!("--notification-{outcome}"), &pane, post_id])
            .output()
            .unwrap()
    }

    /// Sends a `blocked` event for the pane and waits for its notification.
    fn notify(&self, pane: &str) {
        let pane = &self.pane(pane);
        let before = self.posts().len();
        let output = self
            .command()
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env(
                "HERDR_PLUGIN_EVENT_JSON",
                format!(
                    r#"{{"event":"pane.agent_status_changed","data":{{"pane_id":"{pane}","agent_status":"blocked","agent":"Codex","title":"Fix {pane}"}}}}"#
                ),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.wait_for_posts(before + 1);
    }

    /// Makes the notification of `pane` end the way alerter reports `answer`.
    fn answer(&self, pane: &str, answer: &str) {
        let group = group(&self.pane(pane));
        let temp = self.dir.join(format!(".answer-{group}"));
        fs::write(&temp, answer).unwrap();
        fs::rename(temp, self.dir.join(format!("answer-{group}"))).unwrap();
    }

    /// Every notification posted so far, in order.
    fn posts(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("notifier.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn posts_for(&self, pane: &str) -> Vec<String> {
        let group = format!("--group|{}|", group(&self.pane(pane)));
        self.posts()
            .into_iter()
            .filter(|post| post.contains(&group))
            .collect()
    }

    fn wait_for_posts(&self, count: usize) {
        for _ in 0..1000 {
            if self.posts().len() >= count {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("expected {count} notifications, got {:?}", self.posts());
    }

    fn record_path(&self, pane: &str) -> PathBuf {
        self.state_dir().join(format!(
            "{PREFIX}.repost.record.{}.json",
            self.pane(pane).replace(':', "-")
        ))
    }

    fn record(&self, pane: &str) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(self.record_path(pane)).unwrap()).unwrap()
    }

    /// The ledger status of the pane's notification.
    fn status(&self, pane: &str) -> String {
        self.record(pane)["status"].as_str().unwrap().to_string()
    }

    /// The posting the pane's notification is on.
    fn post_id(&self, pane: &str) -> String {
        self.record(pane)["post_id"].as_str().unwrap().to_string()
    }

    fn click_marker(&self) -> PathBuf {
        self.state_dir().join(format!("{PREFIX}.repost.click"))
    }

    fn cleared_marker(&self, pane: &str) -> PathBuf {
        self.state_dir()
            .join(format!("{}.cleared", group(&self.pane(pane))))
    }

    /// Waits until the pane's record has `status`, and fails after 5 s.
    fn wait_for_status(&self, pane: &str, status: &str) {
        for _ in 0..500 {
            if self.record_path(pane).exists() && self.status(pane) == status {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("{pane} is {}, expected {status}", self.status(pane));
    }

    /// Waits until the pane's notification script has exited: its temp
    /// files, named after the `.cleared` marker, are gone.
    fn wait_for_script_end(&self, pane: &str) {
        let prefix = format!("{}.cleared.", group(&self.pane(pane)));
        for _ in 0..500 {
            let running = fs::read_dir(self.state_dir()).is_ok_and(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            });
            if !running {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the script of {pane} did not finish");
    }

    /// Forgets the pane's last agent status, so the next `blocked` event
    /// notifies again (ADR 10).
    fn forget_last_status(&self, pane: &str) {
        fs::remove_file(
            self.state_dir()
                .join(format!("{}.last-status", group(&self.pane(pane)))),
        )
        .unwrap();
    }

    /// Waits for every notification script to finish, then removes the
    /// session. Each pane's notification must have been answered.
    fn finish(self) {
        wait_for_detached_notifier(&self.state_dir());
        fs::remove_dir_all(&self.dir).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn legacy_button_labels_in_the_config_are_ignored() {
    let session = Session::new(
        "[notify]\naction_label = \"Go\"\nclose_label = \"Later\"\n\n[notify.status.blocked]\naction_label = \"Look\"\nclose_label = \"Nope\"\n",
    );

    session.notify("w1:p2");

    let post = &session.posts()[0];
    assert!(post.contains("|--actions|Focus|"), "{post}");
    assert!(!post.contains("--close-label"), "{post}");
    for label in ["Go", "Later", "Look", "Nope"] {
        assert!(!post.contains(&format!("|{label}|")), "{post}");
    }
    session.answer("w1:p2", "@TIMEOUT");
    session.finish();
}

#[cfg(unix)]
#[test]
fn the_title_names_the_tab_as_it_was_before_the_marker() {
    let session = Session::new("");

    session.notify("w1:p2");

    // The marker renamed the tab before the notification was rendered.
    assert_eq!(
        session.herdr.tab_label(&session.workspace.tab_id),
        marked(&tab_icon(), "status")
    );
    let post = &session.posts()[0];
    assert!(post.starts_with("--title=⌂ repo ▸ status|"), "{post}");
    // A timeout is the end of it: the notification is done.
    session.answer("w1:p2", "@TIMEOUT");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p2"), "done");
    session.finish();
}

#[cfg(unix)]
#[test]
fn a_click_reposts_the_notification_it_closed_silently_and_unchanged() {
    let session = Session::new(
        "[notify]\nsubtitle = \"{agent} · {pane}\"\nsound = \"Ping\"\ntimeout = 600\n",
    );
    session.notify("w1:p3");
    session.notify("w1:p4");
    session.notify("w1:p2");
    // p4 is still on screen, so the click leaves it alone.
    fs::write(
        session.dir.join("shown"),
        format!(
            r#"[{{"groupID":"{}","title":"⌂ repo ▸ status"}}]"#,
            group(&session.pane("w1:p4"))
        ),
    )
    .unwrap();

    // The click on p2 closes p3 too, as alerter 26.5 does.
    let clicked = std::time::Instant::now();
    session.answer("w1:p2", "Focus");
    session.answer("w1:p3", "@CLOSED");
    session.wait_for_posts(4);

    assert!(
        clicked.elapsed() >= std::time::Duration::from_millis(500),
        "reposted after {:?}",
        clicked.elapsed()
    );
    let p3 = session.posts_for("w1:p3");
    assert_eq!(p3.len(), 2, "{p3:?}");
    assert_silent_unchanged_repost(&session, &p3);
    assert_eq!(session.posts_for("w1:p4").len(), 1);
    assert_eq!(session.posts_for("w1:p2").len(), 1);
    assert_eq!(session.status("w1:p2"), "done");
    assert_eq!(session.status("w1:p3"), "pending");

    // The repost has a loop of its own: closing it now dismisses it.
    session.answer("w1:p3", "@CLOSED");
    session.answer("w1:p4", "@TIMEOUT");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p3"), "dismissed");
    assert_eq!(session.status("w1:p4"), "done");
    session.finish();
}

#[cfg(unix)]
#[test]
fn a_close_without_a_recent_click_is_dismissed_for_good() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p3"), "dismissed");

    // A later click on another notification does not bring it back.
    session.notify("w1:p2");
    session.answer("w1:p2", "Focus");
    wait_for_detached_notifier(&session.state_dir());
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert_eq!(session.posts_for("w1:p3").len(), 1);
    assert_eq!(session.status("w1:p3"), "dismissed");
    assert_eq!(session.status("w1:p2"), "done");
    session.finish();
}

#[cfg(unix)]
#[test]
fn a_close_within_a_second_of_a_click_is_not_dismissed() {
    let session = Session::new("");
    session.notify("w1:p3");

    fs::write(
        session.state_dir().join(format!("{PREFIX}.repost.click")),
        "",
    )
    .unwrap();
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());

    assert_eq!(session.status("w1:p3"), "pending");
    session.finish();
}

#[cfg(unix)]
#[test]
fn two_clicks_at_once_repost_a_closed_notification_once() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p1");
    session.notify("w1:p2");

    session.answer("w1:p1", "Focus");
    session.answer("w1:p2", "@CONTENTCLICKED");
    session.answer("w1:p3", "@CLOSED");
    session.wait_for_posts(4);
    // Time for a second, wrong repost to show up.
    std::thread::sleep(std::time::Duration::from_millis(700));

    assert_eq!(session.posts().len(), 4, "{:?}", session.posts());
    assert_eq!(session.posts_for("w1:p3").len(), 2);
    assert_eq!(session.status("w1:p1"), "done");
    assert_eq!(session.status("w1:p2"), "done");
    session.answer("w1:p3", "@TIMEOUT");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p3"), "done");
    session.finish();
}

/// Gap: the `pane.focused` path must resolve the record itself, not leave it
/// to the script's later report.
#[cfg(unix)]
#[test]
fn focusing_the_pane_ends_its_pending_notification_at_once() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");

    // The user switches to the terminal that runs Herdr and focuses p3.
    write_terminal_binding(
        &session.state_dir(),
        session.workspace_id(),
        "com.example.terminal",
    );
    fs::write(session.dir.join("frontmost"), "com.example.terminal").unwrap();
    let output = session
        .command()
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            focused_event(&session.pane("w1:p3")),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(session.dir.join("frontmost"), "com.example.other").unwrap();

    // Done before its script reports anything.
    assert_eq!(session.status("w1:p3"), "done");

    // The stub's `--remove` does not end the waiting instance, so the end is
    // reported by hand. A later click on p2 does not bring p3 back.
    session.answer("w1:p3", "@CLOSED");
    session.answer("w1:p2", "Focus");
    wait_for_detached_notifier(&session.state_dir());
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(session.posts_for("w1:p3").len(), 1);
    assert_eq!(session.status("w1:p3"), "done");
    session.finish();
}

/// Gap: `closed` reads the `.cleared` marker. Without it this close, with no
/// click around, would count as the user's.
#[cfg(unix)]
#[test]
fn a_close_after_the_pane_was_focused_is_done_not_dismissed() {
    let session = Session::new("");
    session.notify("w1:p3");
    fs::write(session.cleared_marker("w1:p3"), "").unwrap();

    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());

    assert_eq!(session.status("w1:p3"), "done");
    session.finish();
}

/// Gap: the re-post loop reads the `.cleared` marker. Without the check the
/// record is claimed and re-posted; the re-post script then exits at its own
/// `.cleared` check without showing anything, so the record stays pending.
#[cfg(unix)]
#[test]
fn a_click_ends_a_closed_notification_whose_pane_was_focused_meanwhile() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");
    // p3 was closed by the click and its pane focused before the click
    // looked; only the marker is written, so the focus path itself is not in
    // play.
    fs::write(session.click_marker(), "").unwrap();
    session.answer("w1:p3", "@CLOSED");
    // The marker lands after p3's own close report, which would otherwise
    // see it and end the record itself.
    session.wait_for_script_end("w1:p3");
    assert_eq!(session.status("w1:p3"), "pending");
    fs::write(session.cleared_marker("w1:p3"), "").unwrap();

    session.answer("w1:p2", "Focus");
    wait_for_detached_notifier(&session.state_dir());

    assert_eq!(session.status("w1:p3"), "done");
    assert_eq!(session.posts_for("w1:p3").len(), 1);
    session.finish();
}

/// Gap: a close that reports before the click marker lands. The settle wait
/// lets a marker written 30 ms later still count.
#[cfg(unix)]
#[test]
fn a_close_reported_just_before_the_click_marker_lands_stays_pending() {
    let session = Session::new("");
    session.notify("w1:p3");
    let post_id = session.post_id("w1:p3");

    let close = session
        .command()
        .args(["--notification-closed", &session.pane("w1:p3"), &post_id])
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    fs::write(session.click_marker(), "").unwrap();
    let output = close.wait_with_output().unwrap();

    assert!(output.status.success());
    assert_eq!(session.status("w1:p3"), "pending");
    session.answer("w1:p3", "@TIMEOUT");
    session.finish();
}

/// Gap: a click marker older than the click window does not make a close
/// collateral.
#[cfg(unix)]
#[test]
fn a_close_more_than_a_second_after_a_click_is_dismissed() {
    let session = Session::new("");
    session.notify("w1:p3");
    let marker = session.click_marker();
    fs::write(&marker, "").unwrap();
    fs::File::options()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_modified(SystemTime::now() - std::time::Duration::from_secs(2))
        .unwrap();

    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());

    assert_eq!(session.status("w1:p3"), "dismissed");
    session.finish();
}

/// A click on p2 closes p3 as alerter 26.5 does, yet p3 is not posted again:
/// `block` sets up the reason. p3 stays pending with its one posting.
#[cfg(unix)]
fn assert_click_does_not_repost(block: impl FnOnce(&Session, &str)) {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");
    let post_id = session.post_id("w1:p3");
    block(&session, &post_id);

    session.answer("w1:p2", "Focus");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert_eq!(session.posts_for("w1:p3").len(), 1, "{:?}", session.posts());
    assert_eq!(session.post_id("w1:p3"), post_id);
    assert_eq!(session.status("w1:p3"), "pending");
    session.finish();
}

/// Gap: a posting another click already claimed is not posted again, without
/// relying on two clicks racing.
#[cfg(unix)]
#[test]
fn a_claimed_posting_is_not_reposted() {
    assert_click_does_not_repost(|session, post_id| {
        fs::write(
            session.state_dir().join(format!(
                "{}.claim.{}.{post_id}",
                repost(),
                session.pane("w1:p3").replace(':', "-")
            )),
            "",
        )
        .unwrap();
    });
}

/// Gap: without alerter's list nothing is known, so nothing is re-posted.
#[cfg(unix)]
#[test]
fn a_failing_list_reposts_nothing() {
    assert_click_does_not_repost(|session, _| {
        fs::write(session.dir.join("list-fails"), "").unwrap();
    });
}

/// Gap: a script that outlived its posting reports with the old `post_id`;
/// the pane's newer record is untouched, whatever the report.
#[cfg(unix)]
#[test]
fn reports_from_an_old_posting_leave_the_new_record_alone() {
    let session = Session::new("");
    session.notify("w1:p3");
    let old_post_id = session.post_id("w1:p3");
    session.answer("w1:p3", "@TIMEOUT");
    wait_for_detached_notifier(&session.state_dir());
    session.forget_last_status("w1:p3");
    session.notify("w1:p3");
    let record = fs::read(session.record_path("w1:p3")).unwrap();
    assert_ne!(session.post_id("w1:p3"), old_post_id);

    for outcome in ["closed", "ended", "clicked"] {
        // The click cannot focus without a Herdr socket; only the ledger
        // matters here.
        let output = session.report(outcome, "w1:p3", &old_post_id);
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("panicked"),
            "{outcome}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(session.record_path("w1:p3")).unwrap(),
            record,
            "{outcome}"
        );
    }
    session.answer("w1:p3", "@TIMEOUT");
    session.finish();
}

/// Gap: alerter failing without any output ends the posting.
#[cfg(unix)]
#[test]
fn a_failing_alerter_ends_the_notification() {
    let session = Session::new("");
    session.notify("w1:p3");

    session.answer("w1:p3", "@EXIT3");
    wait_for_detached_notifier(&session.state_dir());

    assert_eq!(session.status("w1:p3"), "done");
    session.finish();
}

/// Gap: a dismissed record ignores every later report of its posting, even a
/// close right after a click.
#[cfg(unix)]
#[test]
fn a_dismissed_notification_stays_dismissed() {
    let session = Session::new("");
    session.notify("w1:p3");
    let post_id = session.post_id("w1:p3");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p3"), "dismissed");

    fs::write(session.click_marker(), "").unwrap();
    for outcome in ["closed", "ended"] {
        assert!(session.report(outcome, "w1:p3", &post_id).status.success());
        assert_eq!(session.status("w1:p3"), "dismissed", "{outcome}");
    }
    session.finish();
}

/// Gap: only pending records count as others to re-post. A dismissed one
/// must not even make the click wait for the click window. The claim check
/// would stop a wrong re-post, so the wait is what shows the filter.
#[cfg(unix)]
#[test]
fn a_click_with_only_dismissed_others_does_not_wait() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    assert_eq!(session.status("w1:p3"), "dismissed");
    session.notify("w1:p2");
    let post_id = session.post_id("w1:p2");

    let started = std::time::Instant::now();
    session.report("clicked", "w1:p2", &post_id);
    let took = started.elapsed();

    assert!(
        took < std::time::Duration::from_millis(400),
        "took {took:?}"
    );
    assert_eq!(session.status("w1:p2"), "done");
    session.answer("w1:p2", "@TIMEOUT");
    session.finish();
}

/// Gap: a report for a pane that has no record changes nothing and fails
/// nothing.
#[cfg(unix)]
#[test]
fn a_report_for_a_pane_without_a_record_is_harmless() {
    let session = Session::new("");

    for outcome in ["closed", "ended", "clicked"] {
        let output = session.report(outcome, "w1:p9", "1-2-3");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("panicked"), "{outcome}: {stderr}");
        // A click still tries to focus, which fails here without a Herdr
        // socket; the others succeed.
        if outcome != "clicked" {
            assert!(output.status.success(), "{outcome}: {stderr}");
        }
    }
    assert!(!session.record_path("w1:p9").exists());
    assert!(session.posts().is_empty());
    session.finish();
}

/// Gap: a click reported by a script whose posting is stale still focuses
/// its pane and re-posts the others, but never the clicked pane's own newer
/// notification. This is what the `pane_id == clicked_pane` skip in the
/// re-post loop is for: p2's newer record is pending, older than the click
/// window, and not listed.
#[cfg(unix)]
#[test]
fn a_stale_click_focuses_and_reposts_others_but_not_its_own_pane() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");
    let p2_record = fs::read(session.record_path("w1:p2")).unwrap();
    // p3 was closed by a click.
    fs::write(session.click_marker(), "").unwrap();
    session.answer("w1:p3", "@CLOSED");
    session.wait_for_status("w1:p3", "pending");

    write_terminal_binding(
        &session.state_dir(),
        session.workspace_id(),
        "com.example.terminal",
    );
    let p2 = session.pane("w1:p2");
    let (socket_path, socket_handle) = focus_socket(
        &p2,
        &format!(
            r#"{{"id":"dagadbm:focus","result":{{"type":"pane_info","pane":{{"pane_id":"{p2}","workspace_id":"{}"}}}}}}"#,
            session.workspace_id()
        ),
    );
    let output = session
        .command()
        .args(["--notification-clicked", &p2, "stale-post"])
        .env("HERDR_SOCKET_PATH", &socket_path)
        .output()
        .unwrap();
    socket_handle.join().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    session.wait_for_posts(3);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(session.posts_for("w1:p3").len(), 2);
    assert_eq!(session.posts_for("w1:p2").len(), 1);
    assert_eq!(fs::read(session.record_path("w1:p2")).unwrap(), p2_record);
    session.answer("w1:p2", "@TIMEOUT");
    session.answer("w1:p3", "@TIMEOUT");
    session.finish();
}

fn assert_silent_unchanged_repost(session: &Session, p3: &[String]) {
    assert!(p3[0].contains("|--sound|Ping|"), "{}", p3[0]);
    // The same stored text, the same group and timeout, and no sound.
    assert_eq!(p3[1], p3[0].replace("--sound|Ping|", ""));
    // The subtitle names the pane by the last part of its ID.
    let p3_name = session
        .pane("w1:p3")
        .rsplit(':')
        .next()
        .unwrap()
        .to_string();
    assert!(
        p3[1].contains(&format!("--subtitle=Codex · {p3_name}|")),
        "{}",
        p3[1]
    );
    // The re-post is silent and restarts the stored timeout in full (the
    // stub ignores `--timeout`, so only the argument is checked).
    assert!(!p3[1].contains("--sound"), "{}", p3[1]);
    assert!(p3[1].contains("|--timeout|600|"), "{}", p3[1]);
}
