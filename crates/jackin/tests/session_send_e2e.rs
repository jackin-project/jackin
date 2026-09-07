// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! End-to-end proof of the capsule's `session.send` and `events` control
//! surface against a real Docker daemon: launch an instance, subscribe to its
//! event stream, type a prompt into the running agent session, and observe the
//! `Working` transition arrive on that stream.
//!
//! What makes this an integration test rather than a unit test is the loop it
//! closes. The bytes leave the host through the control protocol, land on a
//! real PTY inside the container, are observed by the capsule's own
//! terminal-observation arbitration, and come back as a state transition. No
//! part of that chain is stubbed — in particular the state is the capsule's
//! verdict, never the test's.
//!
//! The fake agent marks the work it starts with `OSC 133 ; C`, the
//! shell-integration pre-exec mark the capsule already reads as strong
//! working evidence, and keeps real work in flight while it holds that state.
//! That reuses the shipped detection design instead of teaching the test a
//! private signal the product does not have.

// Expects only apply when the e2e feature compiles the body; without it the
// crate is empty and unfulfilled-expect would fail `cargo clippy -p jackin`.
#![cfg_attr(
    feature = "e2e",
    expect(
        clippy::unwrap_used,
        clippy::disallowed_methods,
        reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
    )
)]
#![cfg(feature = "e2e")]

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jackin_core::JackinPaths;
use jackin_protocol::control::{AgentState, SessionEventKind};
use jackin_runtime::runtime::session_control::{SessionEvents, send_session_text};
use jackin_runtime::runtime::snapshot::fetch_snapshot;
use tempfile::tempdir;

// The `dind_e2e` helper modules are shared verbatim with that suite, which
// uses all of them; this suite drives one launch and needs only part. Each
// integration test is its own binary, so the unused half is genuinely dead
// here — and duplicating the harness to avoid saying so would be worse.
#[path = "dind_e2e/common.rs"]
mod common;
#[path = "dind_e2e/diagnostics.rs"]
mod diagnostics;
#[expect(
    dead_code,
    reason = "shared dind_e2e harness: this suite uses part of it"
)]
#[path = "dind_e2e/pty_runner.rs"]
mod pty_runner;
#[path = "dind_e2e/transcript.rs"]
mod transcript;
#[expect(
    dead_code,
    reason = "shared dind_e2e harness: this suite uses part of it"
)]
#[path = "dind_e2e/util.rs"]
mod util;

use common::{e2e_construct_image, e2e_serial_lock, require_e2e_prereqs};
use pty_runner::{PtyFileSentinel, run_in_pty_until_file};
use util::{cleanup_role, run};

const ROLE_KEY: &str = "jackin-e2e/session-send";
const ROLE_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__session-send";

/// What the host types into the running agent. The trailing carriage return is
/// the submit key: `session.send` writes the payload verbatim and appends
/// nothing, so a caller that wants the agent to act includes it.
const PROMPT: &str = "start the review\r";

/// How long the fake agent stays busy after a prompt. Long enough that the
/// capsule's status tick observes the working state and publishes it well
/// before the agent falls back to idle.
const AGENT_WORK_SECONDS: u32 = 20;

struct E2eRoleCleanup;

impl Drop for E2eRoleCleanup {
    fn drop(&mut self) {
        cleanup_role(ROLE_KEY, ROLE_CONTAINER_PREFIX);
    }
}

#[test]
fn session_send_reaches_the_pty_and_events_report_the_working_transition() {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup;

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("session-send-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_role_repo(&role_source);
    write_config(&config_dir.join("config.toml"), &role_source);
    seed_prompt_reader_agent(&home);

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });

    // The control work runs on a side thread while the PTY keeps the launch
    // (and therefore the instance) alive. It reports back through `outcome`
    // and releases the PTY runner through `completed`.
    let completed = Arc::new(AtomicBool::new(false));
    let outcome: Arc<Mutex<Option<Result<Observed, String>>>> = Arc::new(Mutex::new(None));
    let worker = {
        let completed = Arc::clone(&completed);
        let outcome = Arc::clone(&outcome);
        let home = home.clone();
        let workspace_dir = workspace_dir.clone();
        std::thread::spawn(move || {
            let observed = observe_session_send(&home, &workspace_dir);
            *outcome.lock().unwrap() = Some(observed);
            completed.store(true, Ordering::Release);
        })
    };

    let target = format!("{}:/workspace", workspace_dir.display());
    let args = ["load", ROLE_KEY, &target, "--agent", "claude"];
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];
    let output = run_in_pty_until_file(
        &jackin,
        &args,
        &home,
        &workspace_dir,
        &extra_env,
        &[],
        PtyFileSentinel {
            // Deliberately a path nothing ever writes: this launch is torn
            // down by `stop_after`, when the observing thread is done, not by
            // a file the agent produces. The instance has to stay up for the
            // whole subscription.
            path: &workspace_dir.join("never-written.txt"),
            text: "unreachable",
            timeout: Duration::from_mins(8),
            accept_early_exit_after: None,
            stop_after: Some(&completed),
        },
    );
    worker.join().expect("observer thread must finish");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let observed = outcome
        .lock()
        .unwrap()
        .take()
        .expect("observer thread must record an outcome")
        .unwrap_or_else(|error| {
            panic!(
                "{error}\n{}",
                diagnostics::e2e_failure_context(&home, &stdout, &stderr)
            )
        });

    // 1. The daemon accepted the send and reported the byte count it wrote.
    assert_eq!(
        observed.bytes_sent,
        PROMPT.len() as u64,
        "the daemon must report writing the whole payload, verbatim"
    );

    // 2. The bytes actually reached the agent's PTY. The agent writes what it
    //    read into the bound workspace, so this is checked outside the
    //    terminal transcript.
    let received =
        std::fs::read_to_string(workspace_dir.join("agent-received.txt")).unwrap_or_default();
    assert!(
        received.contains(PROMPT.trim_end_matches('\r')),
        "the agent should have read the prompt from its PTY, got {received:?}\n{}",
        diagnostics::e2e_failure_context(&home, &stdout, &stderr)
    );

    // 3. The `Working` transition arrived on the event stream, for the session
    //    addressed, with the state the capsule arbitrated.
    assert_eq!(observed.working.session, observed.session);
    assert_eq!(observed.working.state, AgentState::Working);
    assert!(
        matches!(
            observed.working.kind,
            SessionEventKind::StateChanged { .. } | SessionEventKind::Subscribed
        ),
        "the Working record must be a transition or the subscription baseline, got {:?}",
        observed.working.kind
    );
}

/// What the observing thread proved.
struct Observed {
    session: u64,
    bytes_sent: u64,
    working: jackin_protocol::control::SessionEventRecord,
}

fn observe_session_send(home: &Path, workspace_dir: &Path) -> Result<Observed, String> {
    let paths = JackinPaths::resolve_with_env(home, None, None);

    // The agent writes this once its PTY is live and it is reading stdin.
    // Sending before that would race the runtime's own startup.
    wait_for(Duration::from_mins(6), "the agent to come up", || {
        workspace_dir.join("agent-ready.txt").exists()
    })?;
    let container = wait_for_value(Duration::from_mins(2), "the instance container", || {
        running_container_name()
    })?;

    let session = wait_for_value(Duration::from_mins(2), "a session in the snapshot", || {
        fetch_snapshot(&paths, &container)
            .ok()
            .flatten()
            .and_then(|snapshot| {
                snapshot
                    .tabs
                    .iter()
                    .flat_map(|tab| tab.panes.iter())
                    .map(|pane| pane.session_id)
                    .next()
            })
    })?;

    // Subscribe before sending: the transition this test is about is caused by
    // the send, so the stream has to be open first or the proof is a race.
    let mut events = SessionEvents::subscribe(&paths, &container, Some(session))
        .map_err(|error| format!("subscribing to the event stream failed: {error:#}"))?;
    let baseline = events
        .next_event(Duration::from_secs(30))
        .map_err(|error| format!("reading the subscription baseline failed: {error:#}"))?
        .ok_or_else(|| "the subscription produced no baseline record".to_owned())?;
    if baseline.session != session {
        return Err(format!(
            "a session-filtered subscription delivered session {}",
            baseline.session
        ));
    }

    let (bytes_sent, _transport) = send_session_text(&paths, &container, session, PROMPT)
        .map_err(|error| format!("session.send failed: {error:#}"))?;

    let working = events
        .wait_for_state(
            session,
            AgentState::Working,
            Instant::now() + Duration::from_mins(2),
        )
        .map_err(|error| format!("no Working transition arrived on the event stream: {error:#}"))?;

    Ok(Observed {
        session,
        bytes_sent,
        working,
    })
}

fn running_container_name() -> Option<String> {
    let output = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("label=jackin.class={ROLE_KEY}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|name| !name.is_empty() && !name.ends_with("-dind"))
        .map(str::to_owned)
}

fn wait_for(timeout: Duration, what: &str, mut ready: impl FnMut() -> bool) -> Result<(), String> {
    wait_for_value(timeout, what, move || ready().then_some(()))
}

fn wait_for_value<T>(
    timeout: Duration,
    what: &str,
    mut probe: impl FnMut() -> Option<T>,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {what}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn write_config(path: &Path, role_source: &Path) {
    std::fs::write(
        path,
        format!(
            r#"version = "v1alpha10"

[accounts.e2e-claude]
name = "E2E Claude"
provider = "anthropic"
[accounts.e2e-claude.credential]
type = "api_key"
value = "synthetic-e2e-claude-key"
[account_bindings]
claude = "e2e-claude"

[roles."{ROLE_KEY}"]
git = "{}"
trusted = true
"#,
            role_source.display()
        ),
    )
    .unwrap();
}

fn seed_role_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(
        path.join("Dockerfile"),
        format!(
            "FROM {}\n",
            std::env::var("JACKIN_E2E_CONSTRUCT_IMAGE")
                .unwrap_or_else(|_| "projectjackin/construct:0.1-trixie".to_owned())
        ),
    )
    .unwrap();
    std::fs::write(
        path.join(jackin_core::MANIFEST_FILENAME),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude"]

[identity]
name = "Session Send"

[claude]
plugins = []
"#,
    )
    .unwrap();

    run("git", &["init"], Some(path));
    run("git", &["add", "."], Some(path));
    run(
        "git",
        &[
            "-c",
            "user.name=Jackin E2E",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Seed session-send e2e role",
        ],
        Some(path),
    );
}

/// A fake `claude` that reads its PTY and reacts the way a real agent does:
/// announce readiness, block on stdin, and on receiving a prompt mark the work
/// with `OSC 133 ; C` and stay genuinely busy until it marks completion with
/// `OSC 133 ; D`.
///
/// The mark is the load-bearing part. It is the shipped shell-integration
/// signal `Session::feed_pty` already scans and arbitration already grades as
/// strong working evidence, so the transition this test observes is produced
/// by the product's detection path, not by anything the test taught it.
fn prompt_reader_agent_script() -> String {
    format!(
        r#"if [ "${{1:-}}" = "--version" ]; then
  echo "claude 0.0.0-e2e"
  exit 0
fi
echo "jackin session-send e2e agent ready"
: > /workspace/agent-received.txt
: > /workspace/agent-ready.txt
while IFS= read -r line; do
  printf '%s\n' "$line" >> /workspace/agent-received.txt
  # OSC 133 ; C — pre-exec: work starts now.
  printf '\033]133;C\007'
  end=$(($(date +%s) + {AGENT_WORK_SECONDS}))
  while [ "$(date +%s)" -lt "$end" ]; do
    printf 'working\n'
    sleep 1
  done
  # OSC 133 ; D — the work finished.
  printf '\033]133;D;0\007'
done
"#
    )
}

fn seed_prompt_reader_agent(home: &Path) {
    let script = prompt_reader_agent_script();
    let body = format!(
        r#"if [ "${{1:-}}" = "install" ]; then
  mkdir -p "$HOME/.local/bin"
  cat > "$HOME/.local/bin/claude" <<'AGENT'
#!/bin/sh
set -eu
{script}
AGENT
  chmod 0755 "$HOME/.local/bin/claude"
  exit 0
fi
{script}
"#
    );
    let stub = home
        .join(".jackin")
        .join("cache")
        .join("agent-binaries-test-stub")
        .join("claude");
    std::fs::create_dir_all(stub.parent().unwrap()).unwrap();
    std::fs::write(&stub, format!("#!/bin/sh\nset -eu\n{body}")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();
    }
}
