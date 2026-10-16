use std::process::Command;

/// The sub-plugin name every runtime name starts with (ADR 14).
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

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dagadbm"));
    command.arg("notify");
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

    write_fixed_frontmost(&temp_dir, "com.example.terminal");

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let output = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(&notifier_log).unwrap(),
        "--remove\nherdr-dagadbm-notify-w1-p2\n"
    );
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

    let output = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
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
        r#"{"workspaces":{"w1":"com.mitchellh.ghostty"}}"#
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn visible_focused_pane_removes_its_pending_notification() {
    let temp_dir = temp_test_dir();

    let frontmost_state = temp_dir.join("frontmost-bundle-id");

    write_state_frontmost(&temp_dir);

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nif [ \"$1\" = \"agent\" ] && [ \"$2\" = \"get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":true,\"pane_id\":\"w1:p2\"}}}'\nelse\n  printf '%s\\n' '{\"result\":{\"panes\":[{\"focused\":true,\"pane_id\":\"w1:p2\"}]}}'\nfi\n",
    );

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFIER_LOG\"\nif [ \"$1\" = \"--remove\" ]; then\n  touch \"$REMOVE_SIGNAL\"\n  exit 0\nfi\nfor _ in 1 2 3 4 5; do\n  [ -e \"$REMOVE_SIGNAL\" ] && exit 0\n  sleep 1\ndone\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let remove_signal = temp_dir.join("removed");
    let path = path_with_temp_dir(&temp_dir);

    // Zero configuration: establish the w1 binding first via a real
    // pane.focused event while the fake frontmost app is the terminal.
    fs::write(&frontmost_state, "com.example.terminal\n").unwrap();
    let learn = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
        )
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(learn.status.success());

    // Now the user is elsewhere; the pending notification should auto-remove
    // once they switch back to the bound terminal.
    fs::write(&frontmost_state, "com.example.other\n").unwrap();

    let child = binary()
        .arg("--test")
        .env("HERDR_BIN_PATH", &herdr)
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
    assert!(notifier_output.contains("--remove\nherdr-dagadbm-notify-w1-p2\n"));
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn test_mode_notifies_even_when_pane_is_visible_and_status_filtered() {
    let temp_dir = temp_test_dir();

    // The frontmost app is the configured terminal, so the pane counts as
    // visible and a normal event would be skipped.
    write_fixed_frontmost(&temp_dir, "com.example.terminal");

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nif [ \"$1\" = \"agent\" ] && [ \"$2\" = \"get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":true,\"pane_id\":\"w1:p2\"}}}'\nelse\n  printf '%s\\n' '{\"result\":{\"panes\":[{\"focused\":true,\"pane_id\":\"w1:p2\"}]}}'\nfi\n",
    );

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let output = binary()
        .arg("--test")
        // A filter that excludes the hardcoded test status must not suppress
        // a test notification either.
        .env("HERDR_BIN_PATH", &herdr)
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

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\nif [ \"$1 $2\" = \"agent get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":false,\"pane_id\":\"w1:p2\",\"cwd\":\"/worktrees/api/src\",\"tab_id\":\"w1:t7\",\"terminal_title\":\"◐ Tidy up the parser tests\",\"terminal_title_stripped\":\"Tidy up the parser tests\"}}}'\nelif [ \"$1 $2\" = \"tab get\" ]; then\n  printf '%s\\n' '{\"result\":{\"tab\":{\"label\":\"status\",\"number\":7}}}'\nelif [ \"$1 $2\" = \"workspace list\" ]; then\n  printf '%s\\n' '{\"result\":{\"workspaces\":[{\"workspace_id\":\"w1\",\"label\":\"sample-repo\"}]}}'\nelif [ \"$1 $2\" = \"worktree list\" ]; then\n  printf '%s\\n' '{\"result\":{\"source\":{\"repo_root\":\"/repo\",\"source_checkout_path\":\"/repo\"},\"worktrees\":[{\"branch\":\"main\",\"is_detached\":false,\"path\":\"/repo\"},{\"branch\":\"feature/api\",\"is_detached\":false,\"path\":\"/worktrees/api\"}]}}'\nfi\n",
    );

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
    let output = binary()
        .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"blocked","agent":"Codex","title":"Implement plugin"}}"#,
        )
        .env("HERDR_BIN_PATH", &herdr)
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("HERDR_LOG", &herdr_log)
        .env("GIT_LOG", &git_log)
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    // The notifier runs in a detached script. The fake notifier renames its
    // log into place, so the file appears only once all arguments are written.
    let notifier_output = wait_for_notifier_output(&notifier_log);

    assert!(notifier_output.contains("--title=⌂ sample-repo ▸ status\n"));
    // The pane's terminal title is the message; the event's own title is not
    // shown next to it.
    assert!(notifier_output.contains("Tidy up the parser tests"));
    assert!(!notifier_output.contains("Implement plugin"));
    assert!(notifier_output.contains("--subtitle"));
    // The pane sits in a linked worktree, so the branch is that worktree's,
    // not the `main` of the checkout Herdr reports as the source.
    assert!(notifier_output.contains("Codex · feature/api*"));
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

    // `worktree list` never answers in time. The trailing `:` keeps the shell
    // from exec-ing into `sleep`, so the hang is a descendant holding stdout.
    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nif [ \"$1 $2\" = \"agent get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":false,\"pane_id\":\"w1:p2\",\"cwd\":\"/worktrees/api/src\",\"tab_id\":\"w1:t7\",\"terminal_title\":\"◐ Tidy up the parser tests\",\"terminal_title_stripped\":\"Tidy up the parser tests\"}}}'\nelif [ \"$1 $2\" = \"tab get\" ]; then\n  printf '%s\\n' '{\"result\":{\"tab\":{\"label\":\"status\",\"number\":7}}}'\nelif [ \"$1 $2\" = \"workspace list\" ]; then\n  printf '%s\\n' '{\"result\":{\"workspaces\":[{\"workspace_id\":\"w1\",\"label\":\"sample-repo\"}]}}'\nelif [ \"$1 $2\" = \"worktree list\" ]; then\n  sleep 30; :\nfi\n",
    );

    let git = temp_dir.join("git");
    write_executable(
        &git,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$GIT_LOG\"\nprintf ' 2 files changed, 4 insertions(+), 4 deletions(-)\\n'\n",
    );

    let notifier_log = write_logging_notifier(&temp_dir);
    let git_log = temp_dir.join("git.log");
    let path = path_with_temp_dir(&temp_dir);
    let started = std::time::Instant::now();
    let output = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"blocked","agent":"Codex"}}"#,
        )
        .env("HERDR_BIN_PATH", &herdr)
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
    // time. Every other call would answer at once, but the budget is spent.
    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\nif [ \"$1 $2\" = \"agent get\" ]; then\n  sleep 30; :\nelif [ \"$1 $2\" = \"workspace list\" ]; then\n  printf '%s\\n' '{\"result\":{\"workspaces\":[{\"workspace_id\":\"w1\",\"label\":\"sample-repo\"}]}}'\nfi\n",
    );

    let notifier_log = write_logging_notifier(&temp_dir);
    let herdr_log = temp_dir.join("herdr.log");
    let path = path_with_temp_dir(&temp_dir);
    let started = std::time::Instant::now();
    let output = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"blocked","agent":"Codex"}}"#,
        )
        .env("HERDR_BIN_PATH", &herdr)
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
        "agent get w1:p2\n"
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
    let focused_pane_state = temp_dir.join("focused-pane-id");
    fs::write(&focused_pane_state, "w1:p1\n").unwrap();

    write_state_frontmost(&temp_dir);

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nif [ \"$1\" = \"agent\" ] && [ \"$2\" = \"get\" ]; then\n  focused=false\n  [ \"$(cat \"$FOCUSED_PANE_STATE\")\" = \"w1:p2\" ] && focused=true\n  printf '{\"result\":{\"agent\":{\"focused\":%s,\"pane_id\":\"w1:p2\"}}}\\n' \"$focused\"\nelse\n  printf '{\"result\":{\"panes\":[{\"focused\":true,\"pane_id\":\"%s\"}]}}\\n' \"$(cat \"$FOCUSED_PANE_STATE\")\"\nfi\n",
    );

    let notifier = temp_dir.join("alerter");
    write_executable(
        &notifier,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFIER_LOG\"\nif [ \"$1\" = \"--remove\" ]; then\n  exit 0\nfi\nsleep 3\n",
    );

    let notifier_log = temp_dir.join("notifier.log");
    let path = path_with_temp_dir(&temp_dir);
    let output = binary()
        .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"done"}}"#,
        )
        .env("HERDR_BIN_PATH", &herdr)
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("FRONTMOST_STATE", &frontmost_state)
        .env("FOCUSED_PANE_STATE", &focused_pane_state)
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
    fs::write(&focused_pane_state, "w1:p2\n").unwrap();
    fs::write(&frontmost_state, "com.example.terminal\n").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(3));

    let notifier_output = fs::read_to_string(&notifier_log).unwrap_or_default();
    assert!(!notifier_output.contains("--remove\nherdr-dagadbm-notify-w1-p2\n"));
    wait_for_detached_notifier(&temp_dir.join("state"));
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
    write_terminal_binding(&temp_dir.join("state"), "w2", "com.example.terminal");
    write_executable(
        &temp_dir.join("open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$OPEN_LOG\"\n",
    );
    let (socket_path, socket_handle) = focus_socket(
        "w2:p7",
        r#"{"id":"dagadbm:focus","result":{"type":"pane_info","pane":{"pane_id":"w2:p7","workspace_id":"w2"}}}"#,
    );
    let output = binary()
        .args(["--focus-pane", "w2:p7"])
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("OPEN_LOG", &open_log)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    socket_handle.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(open_log).unwrap(),
        "-b com.example.terminal\n"
    );
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
fn focus_click_reports_a_socket_that_never_answers() {
    let temp_dir = temp_test_dir();
    write_terminal_binding(&temp_dir.join("state"), "w2", "com.example.terminal");
    write_executable(&temp_dir.join("open"), "#!/bin/sh\nexit 0\n");

    let socket_path = unique_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_in_thread = stop.clone();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Hold the connection open without answering: the client has to give up
        // on its own read timeout, not on this side closing the socket. The
        // bounded hold turns a missing timeout into a failed assertion instead
        // of a test that hangs forever.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while !stop_in_thread.load(std::sync::atomic::Ordering::Relaxed)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(stream);
    });

    let started = std::time::Instant::now();
    let output = binary()
        .args(["--focus-pane", "w2:p7"])
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    let elapsed = started.elapsed();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("timed out after 5s"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!temp_dir
        .join("state/herdr-dagadbm-notify-focus-origin-w2.marker")
        .exists());
    assert!(
        elapsed < std::time::Duration::from_secs(8),
        "client waited {elapsed:?}"
    );

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    handle.join().unwrap();
    fs::remove_file(&socket_path).unwrap();
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn notification_content_click_runs_the_focus_helper() {
    let temp_dir = temp_test_dir();
    let herdr = temp_dir.join("herdr");
    let notifier_log = temp_dir.join("notifier.log");
    let open_log = temp_dir.join("open.log");
    write_terminal_binding(&temp_dir.join("state"), "w2", "com.example.terminal");
    write_executable(
        &herdr,
        r#"#!/bin/sh
case "$1 $2" in
  'pane list') echo '{"result":{"panes":[{"focused":true,"pane_id":"w2:p7"}]}}' ;;
  'agent get') echo '{"result":{"agent":{"focused":false,"pane_id":"w2:p7"}}}' ;;
  *) exit 1 ;;
esac
"#,
    );
    write_executable(
        &temp_dir.join("alerter"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NOTIFIER_LOG\"\necho '@CONTENTCLICKED'\n",
    );
    write_executable(
        &temp_dir.join("open"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$OPEN_LOG\"\n",
    );
    write_executable(&temp_dir.join("lsappinfo"), "#!/bin/sh\nexit 1\n");
    let (socket_path, socket_handle) = focus_socket(
        "w2:p7",
        r#"{"id":"dagadbm:focus","result":{"type":"pane_info","pane":{"pane_id":"w2:p7","workspace_id":"w2"}}}"#,
    );
    let output = binary()
        .arg("--test")
        .env("HERDR_BIN_PATH", &herdr)
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
        .env("NOTIFIER_LOG", &notifier_log)
        .env("OPEN_LOG", &open_log)
        .env("PATH", path_with_temp_dir(&temp_dir))
        .output()
        .unwrap();
    socket_handle.join().unwrap();
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
    write_terminal_binding(&state_dir, "w1", "com.mitchellh.ghostty");
    fs::write(&frontmost_state, "com.example.notification-app\n").unwrap();
    write_state_frontmost(&temp_dir);
    write_executable(&temp_dir.join("open"), "#!/bin/sh\nexit 0\n");
    let (socket_path, socket_handle) = focus_socket(
        "w1:p2",
        r#"{"id":"dagadbm:focus","result":{"type":"pane_info","pane":{"pane_id":"w1:p2","workspace_id":"w1"}}}"#,
    );

    let focus = binary()
        .args(["--focus-pane", "w1:p2"])
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

    let event = binary()
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
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
        r#"{"workspaces":{"w1":"com.mitchellh.ghostty"}}"#
    );
    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn obvious_non_terminal_does_not_overwrite_existing_terminal_binding() {
    let temp_dir = temp_test_dir();
    let state_dir = temp_dir.join("state");
    let frontmost_state = temp_dir.join("frontmost");
    let herdr = temp_dir.join("herdr");
    write_terminal_binding(&state_dir, "w1", "com.mitchellh.ghostty");
    fs::write(&frontmost_state, "com.google.Chrome\n").unwrap();
    write_state_frontmost(&temp_dir);
    write_executable(&herdr, "#!/bin/sh\nexit 0\n");

    let output = binary()
        .env("HERDR_BIN_PATH", &herdr)
        .env("HERDR_PLUGIN_STATE_DIR", &state_dir)
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
        )
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
        r#"{"workspaces":{"w1":"com.mitchellh.ghostty"}}"#
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

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nif [ \"$1 $2\" = \"agent get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":false,\"pane_id\":\"w1:p2\",\"tab_id\":\"w1:t7\",\"terminal_title\":\"◐ Tidy up\",\"metadata\":{\"ticket\":\"ABC-1\"}}}}'\nelif [ \"$1 $2\" = \"tab get\" ]; then\n  printf '%s\\n' '{\"result\":{\"tab\":{\"label\":\"status\",\"number\":7}}}'\nfi\n",
    );
    let config_dir = temp_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "[notify]\nnotify_on = [\"working\"]\ntitle = \"{agent} on {tab}\"\nsubtitle = \"\"\nmessage = \"{terminal_title} $ticket\"\naction_label = \"Go\"\nsound = \"Ping\"\ntimeout = 30\n",
    )
    .unwrap();

    let notifier_log = write_logging_notifier(&temp_dir);
    let run = |status: &str| {
        binary()
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env(
                "HERDR_PLUGIN_EVENT_JSON",
                format!(
                    r#"{{"data":{{"pane_id":"w1:p2","agent_status":"{status}","agent":"Codex"}}}}"#
                ),
            )
            .env("HERDR_BIN_PATH", &herdr)
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
    fs::remove_dir_all(temp_dir).unwrap();
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

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\nif [ \"$1 $2\" = \"agent get\" ]; then\n  printf '%s\\n' '{\"result\":{\"agent\":{\"focused\":false,\"pane_id\":\"w1:p2\",\"tab_id\":\"w1:t7\"}}}'\nelif [ \"$1 $2\" = \"tab get\" ]; then\n  printf '%s\\n' \"{\\\"result\\\":{\\\"tab\\\":{\\\"label\\\":\\\"$TAB_LABEL\\\",\\\"number\\\":7}}}\"\nfi\n",
    );

    // No status notifies, so only the markers are under test.
    let config_dir = temp_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "[notify]\nnotify_on = []\n").unwrap();

    let notifier_log = write_logging_notifier(&temp_dir);
    let herdr_log = temp_dir.join("herdr.log");
    let path = path_with_temp_dir(&temp_dir);
    let run = |event: &str, json: &str, tab_label: &str| {
        let output = binary()
            .env("HERDR_PLUGIN_EVENT", event)
            .env("HERDR_PLUGIN_EVENT_JSON", json)
            .env("HERDR_BIN_PATH", &herdr)
            .env("HERDR_PLUGIN_CONFIG_DIR", &config_dir)
            .env("HERDR_PLUGIN_STATE_DIR", temp_dir.join("state"))
            .env("NOTIFIER_LOG", &notifier_log)
            .env("HERDR_LOG", &herdr_log)
            .env("TAB_LABEL", tab_label)
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

    let log = run(
        "pane.agent_status_changed",
        r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"blocked","agent":"claude"}}"#,
        "status",
    );
    assert!(log.contains("tab rename w1:t7 ⚠ status\n"), "{log}");
    assert!(
        log.contains("pane report-metadata w1:p2 --source dagadbm --display-agent ⚠ claude\n"),
        "{log}"
    );

    let log = run(
        "pane.agent_status_changed",
        r#"{"event":"pane.agent_status_changed","data":{"pane_id":"w1:p2","agent_status":"working","agent":"⚠ claude"}}"#,
        "⚠ status",
    );
    assert!(log.contains("tab rename w1:t7 status\n"), "{log}");
    assert!(
        log.contains("pane report-metadata w1:p2 --source dagadbm --clear-display-agent\n"),
        "{log}"
    );

    let log = run(
        "pane.focused",
        r#"{"event":"pane.focused","data":{"pane_id":"w1:p2"}}"#,
        "⚠ status",
    );
    assert!(log.contains("tab rename w1:t7 status\n"), "{log}");

    fs::remove_dir_all(temp_dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_repeated_agent_status_notifies_once_and_leaves_herdr_alone() {
    let temp_dir = temp_test_dir();

    let herdr = temp_dir.join("herdr");
    write_executable(
        &herdr,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\nif [ \"$1 $2\" = \"agent get\" ]; then\n  printf '{\"result\":{\"agent\":{\"focused\":false,\"pane_id\":\"%s\",\"tab_id\":\"w1:t7\"}}}\\n' \"$3\"\nelif [ \"$1 $2\" = \"tab get\" ]; then\n  printf '%s\\n' '{\"result\":{\"tab\":{\"label\":\"status\",\"number\":7}}}'\nfi\n",
    );

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
        let output = binary()
            .env("HERDR_PLUGIN_EVENT", "pane.agent_status_changed")
            .env("HERDR_PLUGIN_EVENT_JSON", json)
            .env("HERDR_BIN_PATH", &herdr)
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

    let log = run("w1:p2", "blocked", "claude", true);
    assert!(log.contains("--display-agent ⚠ claude\n"), "{log}");

    // The echo of the agent marker: same status, marked agent name.
    let log = run("w1:p2", "blocked", "⚠ claude", false);
    assert_eq!(log, "", "a repeat must not call Herdr");

    // Another pane keeps its own last status.
    run("w1:p3", "blocked", "claude", true);

    // A real change, and a return to blocked, each notify again.
    run("w1:p2", "working", "⚠ claude", false);
    run("w1:p2", "blocked", "claude", true);

    // Give a wrongly sent extra notification time to show up.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let notified = fs::read_to_string(&notifier_log).unwrap_or_default();
    assert_eq!(
        notified.lines().collect::<Vec<_>>(),
        [
            "herdr-dagadbm-notify-w1-p2",
            "herdr-dagadbm-notify-w1-p3",
            "herdr-dagadbm-notify-w1-p2"
        ],
        "{notified}"
    );

    fs::remove_dir_all(temp_dir).unwrap();
}

/// A Herdr session with notifications the test answers: every pane is an
/// unfocused agent in tab `w1:t7` of workspace `repo`, and the tab label lives
/// in a file, so the tab marker really renames it. The fake `alerter` logs
/// each notification as one line of `|`-terminated arguments, then waits for
/// the test to write what alerter would print into `answer-<group>`; `--list`
/// prints the file `shown`.
#[cfg(unix)]
struct Session {
    dir: PathBuf,
}

#[cfg(unix)]
impl Session {
    fn new(config: &str) -> Self {
        let dir = temp_test_dir();
        fs::write(dir.join("tab-label"), "status").unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::write(dir.join("config/config.toml"), config).unwrap();
        write_executable(
            &dir.join("herdr"),
            r#"#!/bin/sh
case "$1 $2" in
  'agent get') printf '{"result":{"agent":{"focused":false,"pane_id":"%s","tab_id":"w1:t7"}}}\n' "$3" ;;
  'tab get') printf '{"result":{"tab":{"label":"%s","number":7}}}\n' "$(cat "$STUB_DIR/tab-label")" ;;
  'tab rename') printf '%s' "$4" > "$STUB_DIR/tab-label" ;;
  'workspace list') echo '{"result":{"workspaces":[{"workspace_id":"w1","label":"repo"}]}}' ;;
esac
"#,
        );
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
        Self { dir }
    }

    fn state_dir(&self) -> PathBuf {
        self.dir.join("state")
    }

    /// `dagadbm notify` in this session, with every stub on `PATH`.
    fn command(&self) -> Command {
        let mut command = binary();
        command
            .env("HERDR_BIN_PATH", self.dir.join("herdr"))
            .env("HERDR_PLUGIN_STATE_DIR", self.state_dir())
            .env("HERDR_PLUGIN_CONFIG_DIR", self.dir.join("config"))
            .env("NOTIFIER_LOG", self.dir.join("notifier.log"))
            .env("STUB_DIR", &self.dir)
            .env_remove("HERDR_SOCKET_PATH")
            .env("PATH", path_with_temp_dir(&self.dir));
        command
    }

    /// Runs `--notification-<outcome> <pane> <post_id>`, as a generated
    /// script reports how its notification ended.
    fn report(&self, outcome: &str, pane: &str, post_id: &str) -> std::process::Output {
        self.command()
            .args([&format!("--notification-{outcome}"), pane, post_id])
            .output()
            .unwrap()
    }

    /// Sends a `blocked` event for the pane and waits for its notification.
    fn notify(&self, pane: &str) {
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
        let group = format!("{PREFIX}-{}", pane.replace(':', "-"));
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
        let group = format!("--group|{PREFIX}-{}|", pane.replace(':', "-"));
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
            pane.replace(':', "-")
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
            .join(format!("{PREFIX}-{}.cleared", pane.replace(':', "-")))
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
        let prefix = format!("{PREFIX}-{}.cleared.", pane.replace(':', "-"));
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
    /// notifies again (ADR 11).
    fn forget_last_status(&self, pane: &str) {
        fs::remove_file(
            self.state_dir()
                .join(format!("{PREFIX}-{}.last-status", pane.replace(':', "-"))),
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
        fs::read_to_string(session.dir.join("tab-label")).unwrap(),
        "⚠ status"
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
        r#"[{"groupID":"herdr-dagadbm-notify-w1-p4","title":"⌂ repo ▸ status"}]"#,
    )
    .unwrap();

    // The click on p2 closes p3 too, as alerter 26.5 does.
    let clicked = std::time::Instant::now();
    session.answer("w1:p2", "Focus");
    session.answer("w1:p3", "@CLOSED");
    session.wait_for_posts(4);

    assert!(
        clicked.elapsed() >= std::time::Duration::from_secs(1),
        "reposted after {:?}",
        clicked.elapsed()
    );
    let p3 = session.posts_for("w1:p3");
    assert_eq!(p3.len(), 2, "{p3:?}");
    assert!(p3[0].contains("|--sound|Ping|"), "{}", p3[0]);
    // The same stored text, the same group and timeout, and no sound.
    assert_eq!(p3[1], p3[0].replace("--sound|Ping|", ""));
    assert!(p3[1].contains("--subtitle=Codex · p3|"), "{}", p3[1]);
    // The re-post is silent and restarts the stored timeout in full (the
    // stub ignores `--timeout`, so only the argument is checked).
    assert!(!p3[1].contains("--sound"), "{}", p3[1]);
    assert!(p3[1].contains("|--timeout|600|"), "{}", p3[1]);
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
    std::thread::sleep(std::time::Duration::from_millis(1500));

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
    write_terminal_binding(&session.state_dir(), "w1", "com.example.terminal");
    fs::write(session.dir.join("frontmost"), "com.example.terminal").unwrap();
    let output = session
        .command()
        .env("HERDR_PLUGIN_EVENT", "pane.focused")
        .env(
            "HERDR_PLUGIN_EVENT_JSON",
            r#"{"event":"pane.focused","data":{"pane_id":"w1:p3"}}"#,
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
/// lets a marker written 80 ms later still count.
#[cfg(unix)]
#[test]
fn a_close_reported_just_before_the_click_marker_lands_stays_pending() {
    let session = Session::new("");
    session.notify("w1:p3");
    let post_id = session.post_id("w1:p3");

    let close = session
        .command()
        .args(["--notification-closed", "w1:p3", &post_id])
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
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

/// Gap: a posting another click already claimed is not posted again, without
/// relying on two clicks racing.
#[cfg(unix)]
#[test]
fn a_claimed_posting_is_not_reposted() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");
    let post_id = session.post_id("w1:p3");
    fs::write(
        session
            .state_dir()
            .join(format!("{PREFIX}.repost.claim.w1-p3.{post_id}")),
        "",
    )
    .unwrap();

    session.answer("w1:p2", "Focus");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert_eq!(session.posts_for("w1:p3").len(), 1);
    assert_eq!(session.post_id("w1:p3"), post_id);
    assert_eq!(session.status("w1:p3"), "pending");
    session.finish();
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

/// Gap: without alerter's list nothing is known, so nothing is re-posted.
#[cfg(unix)]
#[test]
fn a_failing_list_reposts_nothing() {
    let session = Session::new("");
    session.notify("w1:p3");
    session.notify("w1:p2");
    fs::write(session.dir.join("list-fails"), "").unwrap();

    session.answer("w1:p2", "Focus");
    session.answer("w1:p3", "@CLOSED");
    wait_for_detached_notifier(&session.state_dir());
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert_eq!(session.posts_for("w1:p3").len(), 1, "{:?}", session.posts());
    assert_eq!(session.status("w1:p3"), "pending");
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
        took < std::time::Duration::from_millis(900),
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

    write_terminal_binding(&session.state_dir(), "w1", "com.example.terminal");
    let (socket_path, socket_handle) = focus_socket(
        "w1:p2",
        r#"{"id":"dagadbm:focus","result":{"type":"pane_info","pane":{"pane_id":"w1:p2","workspace_id":"w1"}}}"#,
    );
    let output = session
        .command()
        .args(["--notification-clicked", "w1:p2", "stale-post"])
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
