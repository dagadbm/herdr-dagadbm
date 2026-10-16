//! A real Herdr server, isolated from the user's own, shared by every test in
//! one test binary.
//!
//! Setup happens once, on the first call to [`HerdrServer::shared`]: the server
//! runs as the named session `herdr-dagadbm-test` under a throwaway `HOME`, so
//! it has its own socket, state and config, and loads none of the user's
//! plugins. Teardown happens once, when the test binary exits: a watchdog
//! process waits for it, stops the server and deletes the throwaway `HOME`.
//! Rust runs no destructor for a `static`, so the watchdog is what makes the
//! teardown happen, even after a panic or Ctrl-C.
//!
//! Tests run in parallel against the one server, so each test creates its own
//! workspace with [`HerdrServer::workspace`] and only touches that. Herdr has
//! one focused pane per server, so a test that depends on focus holds
//! [`HerdrServer::focus`] for as long as it runs. Otherwise a workspace
//! created at startup, the lobby, keeps the focus, and every test pane counts
//! as unfocused.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use std::os::unix::process::CommandExt;

const SESSION: &str = "herdr-dagadbm-test";
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// The variables Herdr sets inside its own panes. A test run from a Herdr pane
/// inherits them, and they would point the test server or its CLI calls at the
/// user's live session.
const INHERITED_HERDR_VARS: &[&str] = &[
    "HERDR_SOCKET_PATH",
    "HERDR_BIN_PATH",
    "HERDR_SESSION",
    "HERDR_ENV",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
];

pub struct HerdrServer {
    herdr_bin: PathBuf,
    home: PathBuf,
    socket: PathBuf,
    lobby: String,
}

/// A workspace created for one test, with its first tab and pane.
pub struct Workspace {
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
}

impl HerdrServer {
    /// The server for this test binary, started on first use.
    pub fn shared() -> &'static HerdrServer {
        static SERVER: OnceLock<HerdrServer> = OnceLock::new();
        SERVER.get_or_init(HerdrServer::start)
    }

    // The server and the watchdog outlive the test binary on purpose, so
    // nothing here waits for them.
    #[allow(clippy::zombie_processes)]
    fn start() -> HerdrServer {
        let herdr_bin = find_herdr();
        // macOS limits a socket path to about 104 bytes, and the socket lives
        // under `HOME`, so `HOME` has to be a short path in `/tmp`.
        let home = PathBuf::from(format!("/tmp/hdt-{}", std::process::id()));
        if home.exists() {
            fs::remove_dir_all(&home).unwrap();
        }
        fs::create_dir(&home).unwrap();
        let socket = home
            .join(".config/herdr/sessions")
            .join(SESSION)
            .join("herdr.sock");
        let mut server = HerdrServer {
            herdr_bin,
            home,
            socket,
            lobby: String::new(),
        };

        let log = fs::File::create(server.home.join("server.log")).unwrap();
        let mut command = server.isolated(Command::new(&server.herdr_bin));
        command
            .arg("server")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            // Its own process group, so a Ctrl-C to the test run leaves the
            // server to the watchdog instead of killing it half-way.
            .process_group(0);
        command.spawn().unwrap();
        server.start_watchdog();
        server.wait_until_ready();
        // The first workspace takes the focus. It belongs to no test.
        server.lobby = server.workspace().workspace_id;
        server
    }

    /// The environment for anything that talks to this server: the user's
    /// Herdr variables removed, `HOME` and the session set to the test ones.
    fn isolated(&self, mut command: Command) -> Command {
        for name in INHERITED_HERDR_VARS {
            command.env_remove(name);
        }
        command
            .env("HOME", &self.home)
            .env("HERDR_SESSION", SESSION);
        command
    }

    /// Stops the server and deletes `HOME` once the test binary has exited.
    #[allow(clippy::zombie_processes)]
    fn start_watchdog(&self) {
        let script = r#"
            while kill -0 "$1" 2>/dev/null; do sleep 0.2; done
            HOME="$2" HERDR_SESSION="$3" "$4" server stop >/dev/null 2>&1
            rm -r "$2"
        "#;
        let mut command = self.isolated(Command::new("/bin/sh"));
        command
            .args(["-c", script, "herdr-dagadbm-test-watchdog"])
            .arg(std::process::id().to_string())
            .arg(&self.home)
            .arg(SESSION)
            .arg(&self.herdr_bin)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        command.spawn().unwrap();
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + START_TIMEOUT;
        while Instant::now() < deadline {
            if self.socket.exists() && self.try_herdr(&["status", "server"]).is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "the test Herdr server did not start within {START_TIMEOUT:?}; its log:\n{}",
            fs::read_to_string(self.home.join("server.log")).unwrap_or_default()
        );
    }

    /// Runs the real `herdr` CLI against this server and returns its JSON
    /// reply. Panics when the command fails or prints no JSON.
    pub fn herdr(&self, args: &[&str]) -> serde_json::Value {
        let output = self.herdr_output(args);
        assert!(
            output.status.success(),
            "herdr {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "herdr {args:?} printed no JSON ({error}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }

    /// Runs the real `herdr` CLI against this server for a command that
    /// prints nothing, such as `pane report-agent`.
    pub fn herdr_quiet(&self, args: &[&str]) {
        let output = self.herdr_output(args);
        assert!(
            output.status.success(),
            "herdr {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn try_herdr(&self, args: &[&str]) -> Option<String> {
        let output = self.herdr_output(args);
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn herdr_output(&self, args: &[&str]) -> std::process::Output {
        self.isolated(Command::new(&self.herdr_bin))
            .args(args)
            .output()
            .unwrap()
    }

    /// Creates a workspace for one test. Its IDs differ between tests, because
    /// every test shares this server.
    pub fn workspace(&self) -> Workspace {
        let reply = self.herdr(&["workspace", "create", "--cwd", "/tmp"]);
        let pane = &reply["result"]["root_pane"];
        let id = |field: &str| pane[field].as_str().unwrap().to_string();
        Workspace {
            workspace_id: id("workspace_id"),
            tab_id: id("tab_id"),
            pane_id: id("pane_id"),
        }
    }

    /// Creates a workspace with a label and a working directory.
    pub fn workspace_in(&self, label: &str, cwd: &Path) -> Workspace {
        let cwd = cwd.to_str().unwrap();
        let reply = self.herdr(&["workspace", "create", "--label", label, "--cwd", cwd]);
        let pane = &reply["result"]["root_pane"];
        let id = |field: &str| pane[field].as_str().unwrap().to_string();
        Workspace {
            workspace_id: id("workspace_id"),
            tab_id: id("tab_id"),
            pane_id: id("pane_id"),
        }
    }

    /// Adds a pane next to `pane`, in the same tab, and returns its ID.
    pub fn split(&self, pane: &str) -> String {
        let reply = self.herdr(&["pane", "split", pane, "--direction", "right"]);
        let new = &reply["result"]["pane"]["pane_id"];
        new.as_str()
            .unwrap_or_else(|| panic!("no pane in the split reply: {reply}"))
            .to_string()
    }

    /// Makes the pane an agent, so `agent get` answers for it.
    pub fn report_agent(&self, pane: &str, agent: &str, state: &str) {
        self.herdr_quiet(&[
            "pane",
            "report-agent",
            pane,
            "--source",
            "test",
            "--agent",
            agent,
            "--state",
            state,
        ]);
    }

    pub fn rename_tab(&self, tab: &str, label: &str) {
        self.herdr(&["tab", "rename", tab, label]);
    }

    pub fn tab_label(&self, tab: &str) -> String {
        let reply = self.herdr(&["tab", "get", tab]);
        reply["result"]["tab"]["label"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The pane that has the server's focus.
    pub fn focused_pane(&self) -> String {
        let reply = self.herdr(&["pane", "list"]);
        let panes = reply["result"]["panes"].as_array().unwrap();
        let focused = panes.iter().find(|pane| pane["focused"] == true);
        focused.unwrap()["pane_id"].as_str().unwrap().to_string()
    }

    /// Sets the pane's terminal title through its shell, and waits until
    /// Herdr reports it. The pane must be an agent already.
    pub fn set_terminal_title(&self, pane: &str, title: &str) {
        let command = format!("printf '\\033]0;{title}\\007'");
        self.herdr_quiet(&["pane", "run", pane, &command]);
        let deadline = Instant::now() + START_TIMEOUT;
        while Instant::now() < deadline {
            let reply = self.herdr(&["agent", "get", pane]);
            if reply["result"]["agent"]["terminal_title"] == title {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the terminal title of {pane} never became {title:?}");
    }

    /// Gives the focus to `workspace` until the guard is dropped, then back
    /// to the lobby. Only one test holds the focus at a time.
    pub fn focus(&self, workspace: &Workspace) -> FocusGuard<'_> {
        static FOCUS: Mutex<()> = Mutex::new(());
        // A test that panicked while holding the lock must not fail the rest.
        let lock = FOCUS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.herdr(&["workspace", "focus", &workspace.workspace_id]);
        FocusGuard {
            server: self,
            _lock: lock,
        }
    }

    /// A `herdr` that logs each call, one line of arguments to `$HERDR_LOG`,
    /// then runs the real one against this server. It sets the socket itself,
    /// so it never reaches the user's session, even when the caller has no
    /// `HERDR_SOCKET_PATH`. Pass it to [`HerdrServer::connect_logged`].
    pub fn logging_herdr(&self, dir: &Path) -> PathBuf {
        self.wrapper(
            dir,
            "[ -n \"$HERDR_LOG\" ] && printf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\n",
            "",
        )
    }

    /// A `herdr` that records each call in `log`, its arguments joined and
    /// ended by `|`, one call per line, then runs the real one against this
    /// server. For code that runs in the test process, where `$HERDR_LOG`
    /// cannot differ between tests.
    pub fn recording_herdr(&self, dir: &Path, log: &Path) -> PathBuf {
        self.wrapper(
            dir,
            &format!(
                "printf '%s|' \"$@\" >> '{log}'\necho >> '{log}'\n",
                log = log.display()
            ),
            "",
        )
    }

    /// Like [`HerdrServer::logging_herdr`], but one command, such as
    /// `worktree list`, hangs for 30 s instead of answering. Every other call
    /// reaches the real server. The trailing `:` keeps the shell from
    /// exec-ing into `sleep`, so the hang is a descendant holding stdout.
    pub fn hanging_herdr(&self, dir: &Path, command: &str) -> PathBuf {
        self.wrapper(
            dir,
            "[ -n \"$HERDR_LOG\" ] && printf '%s\\n' \"$*\" >> \"$HERDR_LOG\"\n",
            &format!("if [ \"$1 $2\" = '{command}' ]; then sleep 30; :; exit 0; fi\n"),
        )
    }

    /// A shell script that runs `log`, then `before`, then the real `herdr`
    /// against this server.
    fn wrapper(&self, dir: &Path, log: &str, before: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("herdr");
        let script = format!(
            "#!/bin/sh\n{log}{before}HERDR_SOCKET_PATH='{}' exec '{}' \"$@\"\n",
            self.socket.display(),
            self.herdr_bin.display()
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    /// Like [`HerdrServer::connect`], but through a `herdr` from
    /// [`HerdrServer::logging_herdr`].
    pub fn connect_logged(&self, command: &mut Command, logging_herdr: &Path) {
        self.connect(command);
        command.env("HERDR_BIN_PATH", logging_herdr);
    }

    /// Points the plugin binary at this server, the way Herdr does when it
    /// runs a hook: `HERDR_BIN_PATH` and `HERDR_SOCKET_PATH`.
    pub fn connect(&self, command: &mut Command) {
        for name in INHERITED_HERDR_VARS {
            command.env_remove(name);
        }
        command
            .env("HERDR_BIN_PATH", &self.herdr_bin)
            .env("HERDR_SOCKET_PATH", &self.socket);
    }
}

/// Holds the server's focus on one workspace. See [`HerdrServer::focus`].
pub struct FocusGuard<'a> {
    server: &'a HerdrServer,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for FocusGuard<'_> {
    fn drop(&mut self) {
        // Ignore a failure here: a panic inside `drop` while the test is
        // already panicking would abort the whole run.
        let _ = self
            .server
            .herdr_output(&["workspace", "focus", &self.server.lobby]);
    }
}

/// The installed `herdr`. The tests need it, so a missing one fails them
/// instead of skipping them.
fn find_herdr() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("herdr"))
        .find(|candidate| is_executable(candidate))
        .unwrap_or_else(|| panic!("these tests need the real `herdr` on PATH"))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}
