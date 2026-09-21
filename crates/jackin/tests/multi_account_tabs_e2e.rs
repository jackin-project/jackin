// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Multi-account isolation in ONE real container: two `codex` instances on
//! distinct OpenAI API-key accounts (different canary key/endpoint/model) plus
//! one `opencode` instance on a profile account, all booted by a single
//! `default_launch` list.
//!
//! Proof is in-container, never tab labels: each fake agent dumps its own
//! process environment to the shared workspace, and the test asserts per-tab
//! credential/endpoint/model identity, independent state roots, and the
//! absence of unselected credential canaries. The same run then exercises
//! split, new tab (both through the agent picker), client-kill + `hardline`
//! reconnect, and container-remove + `hardline` restore, asserting the
//! per-pane account bindings survive every step.

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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jackin_core::JackinPaths;
use jackin_runtime::runtime::snapshot::fetch_snapshot;
use tempfile::tempdir;

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
use pty_runner::{PtyFileSentinel, PtyScriptStep, run_in_pty_until_file};
use util::{cleanup_role, run};

const ROLE_KEY: &str = "jackin-e2e/s3-multi";
const ROLE_CONTAINER_PREFIX: &str = "jackin-jackin-e2e__s3-multi";
const WORKSPACE: &str = "s3";

const CANARY_A: &str = "s3-canary-openai-A-readonly";
const CANARY_B: &str = "s3-canary-openai-B-readonly";
const CANARY_C: &str = "s3-canary-opencode-C-readonly";
const ENDPOINT_A: &str = "https://openai-a.example.invalid/v1";
const ENDPOINT_B: &str = "https://openai-b.example.invalid/v1";
const MODEL_A: &str = "gpt-s3-a";
const MODEL_B: &str = "gpt-s3-b";

struct E2eRoleCleanup;

impl Drop for E2eRoleCleanup {
    fn drop(&mut self) {
        cleanup_role(ROLE_KEY, ROLE_CONTAINER_PREFIX);
    }
}

#[test]
fn multi_account_tabs_isolate_and_preserve_bindings() {
    require_e2e_prereqs();
    let _serial = e2e_serial_lock();
    let _cleanup = E2eRoleCleanup;

    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let config_dir = home.join(".config/jackin");
    let role_source = temp.path().join("s3-role-source");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&workspace_dir).unwrap();

    seed_role_repo(&role_source);
    write_config(&config_dir, &home, &role_source, &workspace_dir);
    seed_opencode_profile(&home);
    seed_fake_agent(&home, "codex");
    seed_fake_agent(&home, "opencode");

    let jackin = std::env::var("CARGO_BIN_EXE_jackin").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .join("target/debug/jackin")
            .display()
            .to_string()
    });
    let construct_image = e2e_construct_image();
    let extra_env = [("JACKIN_CONSTRUCT_IMAGE", construct_image.as_str())];

    // Phase A+B: boot three tabs, then split (cx-b) and new tab (oc-c) via
    // the agent picker. Fake agents number themselves with an atomic
    // mkdir sequence so each picker step can wait for a unique marker.
    let completed = Arc::new(AtomicBool::new(false));
    let outcome: Arc<Mutex<Option<Result<PhaseAB, String>>>> = Arc::new(Mutex::new(None));
    let worker = {
        let completed = Arc::clone(&completed);
        let outcome = Arc::clone(&outcome);
        let home = home.clone();
        let workspace_dir = workspace_dir.clone();
        std::thread::spawn(move || {
            let observed = observe_boot_split_newtab(&home, &workspace_dir);
            *outcome.lock().unwrap() = Some(observed);
            completed.store(true, Ordering::Release);
        })
    };
    let script = [
        // All three boot agents are up once sequence #2 prints.
        PtyScriptStep {
            wait_for: "s3 agent #2 ready",
            input: "\x11\"",
        },
        PtyScriptStep {
            wait_for: "cx-b-inst",
            input: "cx-b-inst\r",
        },
        PtyScriptStep {
            wait_for: "s3 agent #3 ready",
            input: "\x11c",
        },
        PtyScriptStep {
            wait_for: "oc-c-inst",
            input: "oc-c-inst\r",
        },
    ];
    let output = run_in_pty_until_file(
        &jackin,
        &["load", ROLE_KEY, WORKSPACE],
        &home,
        &workspace_dir,
        &extra_env,
        &script,
        PtyFileSentinel {
            path: &workspace_dir.join("never-written.txt"),
            text: "unreachable",
            timeout: Duration::from_mins(14),
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
        .expect("observer thread must record an outcome");
    let phase_ab = match observed {
        Ok(phase) => phase,
        Err(error) => {
            let kept = temp.keep();
            panic!(
                "{error}\nfixture kept at {}\n{}",
                kept.display(),
                diagnostics::e2e_failure_context(&home, &stdout, &stderr)
            )
        }
    };
    assert_phase_ab(&phase_ab, &workspace_dir).unwrap();

    // Phase C: the killed client left the container running; `hardline`
    // reattaches to the same sessions with bindings intact.
    let container = phase_ab.container.clone();
    let reconnect_done = completed_reconnect(&home, &container, &phase_ab);
    let reconnected = run_in_pty_until_file(
        &jackin,
        &["hardline", container.as_str()],
        &home,
        &workspace_dir,
        &extra_env,
        &[],
        PtyFileSentinel {
            path: &workspace_dir.join("never-written.txt"),
            text: "unreachable",
            timeout: Duration::from_mins(4),
            accept_early_exit_after: None,
            stop_after: Some(&reconnect_done),
        },
    );
    drop(reconnected);
    assert!(
        reconnect_done.load(Ordering::Acquire),
        "reconnect must observe the preserved bindings before the timeout"
    );
    assert_reconnect_snapshot(&home, &container, &phase_ab, "reconnect").unwrap();

    // Phase D: removing the container makes `hardline` restore it; the
    // default launch set boots again with correct bindings.
    run("docker", &["rm", "-f", container.as_str()], None);
    let restore_outcome: RestoreOutcome = Arc::new(Mutex::new(None));
    let restore_worker = {
        let restore_outcome = Arc::clone(&restore_outcome);
        let home = home.clone();
        let workspace_dir = workspace_dir.clone();
        std::thread::spawn(move || {
            let observed = observe_restore(&home);
            if observed.is_ok() {
                std::fs::write(workspace_dir.join("s3-restored.txt"), "restored").unwrap();
            }
            *restore_outcome.lock().unwrap() = Some(observed);
        })
    };
    let restored = run_in_pty_until_file(
        &jackin,
        &["hardline", container.as_str()],
        &home,
        &workspace_dir,
        &extra_env,
        &[],
        PtyFileSentinel {
            path: &workspace_dir.join("s3-restored.txt"),
            text: "restored",
            timeout: Duration::from_mins(10),
            accept_early_exit_after: None,
            stop_after: None,
        },
    );
    drop(restored);
    restore_worker.join().expect("restore observer must finish");
    let bindings = restore_outcome
        .lock()
        .unwrap()
        .take()
        .expect("restore observer must record an outcome")
        .expect("restore must serve bound sessions");
    assert_restore_bindings(&bindings);
}

/// Shared restore-observer cell: `None` until the worker records its outcome.
type RestoreOutcome = Arc<Mutex<Option<Result<Vec<(String, String)>, String>>>>;

/// Bindings observed after boot + split + new tab.
struct PhaseAB {
    container: String,
    /// (instance config id, account id) per pane, in snapshot order.
    bindings: Vec<(String, String)>,
}

fn completed_reconnect(home: &Path, container: &str, phase_ab: &PhaseAB) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let worker_done = Arc::clone(&done);
    let home = home.to_path_buf();
    let container = container.to_owned();
    let want = phase_ab.bindings.clone();
    std::thread::spawn(move || {
        let paths = JackinPaths::resolve_with_env(&home, None, None);
        let deadline = Instant::now() + Duration::from_mins(3);
        while Instant::now() < deadline {
            if snapshot_bindings(&paths, &container).is_some_and(|got| got == want) {
                worker_done.store(true, Ordering::Release);
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    done
}

fn snapshot_bindings(paths: &JackinPaths, container: &str) -> Option<Vec<(String, String)>> {
    let snapshot = fetch_snapshot(paths, container).ok()??;
    let mut bindings = Vec::new();
    for tab in &snapshot.tabs {
        for pane in &tab.panes {
            bindings.push((
                pane.agent.clone().unwrap_or_else(|| "<shell>".to_owned()),
                pane.account_id
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ));
        }
    }
    Some(bindings)
}

fn observe_boot_split_newtab(home: &Path, workspace: &Path) -> Result<PhaseAB, String> {
    let paths = JackinPaths::resolve_with_env(home, None, None);
    let container = wait_for_value(Duration::from_mins(6), "the instance container", || {
        running_container_name()
    })?;
    // Three boot tabs first.
    wait_for_value(Duration::from_mins(4), "three bound boot sessions", || {
        let bindings = snapshot_bindings(&paths, &container)?;
        (bindings.len() == 3).then_some(bindings)
    })?;
    // Five env dumps: 3 boot + split + new tab.
    wait_for(Duration::from_mins(4), "five agent env dumps", || {
        env_dumps(workspace).len() == 5
    })?;
    let bindings = wait_for_value(Duration::from_mins(4), "five bound panes", || {
        let bindings = snapshot_bindings(&paths, &container)?;
        (bindings.len() == 5).then_some(bindings)
    })?;
    Ok(PhaseAB {
        container,
        bindings,
    })
}

fn assert_phase_ab(phase: &PhaseAB, workspace: &Path) -> Result<(), String> {
    // Boot order is launch-list order; split + new tab append.
    let got: Vec<(&str, &str)> = phase
        .bindings
        .iter()
        .map(|(agent, account)| (agent.as_str(), account.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("cx-a-inst", "cx-a"),
            ("cx-b-inst", "cx-b"),
            ("oc-c-inst", "oc-c"),
            ("cx-b-inst", "cx-b"),
            ("oc-c-inst", "oc-c"),
        ],
        "every pane must carry its instance + account binding"
    );

    let dumps = read_dumps(workspace);
    assert_eq!(dumps.len(), 5, "one env dump per agent process");
    let codex: Vec<&BTreeMap<String, String>> = dumps
        .values()
        .filter(|env| env.get("S3_FAKE_AGENT").is_some_and(|v| v == "codex"))
        .collect();
    let opencode: Vec<&BTreeMap<String, String>> = dumps
        .values()
        .filter(|env| env.get("S3_FAKE_AGENT").is_some_and(|v| v == "opencode"))
        .collect();
    assert_eq!(codex.len(), 3, "two boot codex + one split codex");
    assert_eq!(
        opencode.len(),
        2,
        "one boot opencode + one new-tab opencode"
    );

    // Same agent/provider, distinct runtime identity per account.
    let mut codex_homes = BTreeSet::new();
    let mut seen_canaries = BTreeSet::new();
    for env in &codex {
        let key = env.get("OPENAI_API_KEY").cloned().unwrap_or_default();
        let endpoint = env.get("OPENAI_BASE_URL").cloned().unwrap_or_default();
        let model = env
            .get("JACKIN_LANE_CODEX_MODEL")
            .cloned()
            .unwrap_or_default();
        let home = env.get("CODEX_HOME").cloned().unwrap_or_default();
        assert!(
            !home.is_empty(),
            "codex panes need isolated CODEX_HOME state roots"
        );
        codex_homes.insert(home);
        match key.as_str() {
            CANARY_A => {
                assert_eq!(endpoint, ENDPOINT_A, "account cx-a endpoint");
                assert_eq!(model, MODEL_A, "account cx-a model");
            }
            CANARY_B => {
                assert_eq!(endpoint, ENDPOINT_B, "account cx-b endpoint");
                assert_eq!(model, MODEL_B, "account cx-b model");
            }
            other => {
                return Err(format!(
                    "codex pane carries unexpected OPENAI_API_KEY: {other:?}"
                ));
            }
        }
        seen_canaries.insert(key);
        // No unselected canary may leak into this pane's environment.
        for (name, value) in env.iter() {
            assert!(
                !value.contains(CANARY_C),
                "codex pane leaks {CANARY_C} via {name}"
            );
        }
    }
    assert_eq!(
        seen_canaries,
        BTreeSet::from([CANARY_A.to_owned(), CANARY_B.to_owned()]),
        "both codex accounts must be live in the container"
    );
    // The two cx-a/cx-b panes (and the cx-b split) each need their own root.
    // Boot cx-a vs boot cx-b must differ; the split pane must differ from
    // its own boot twin too (three distinct roots total).
    assert_eq!(codex_homes.len(), 3, "independent codex state roots");

    for env in &opencode {
        let root = env.get("XDG_DATA_HOME").cloned().unwrap_or_default();
        assert!(
            !root.is_empty(),
            "opencode panes need an isolated XDG_DATA_HOME state root"
        );
        // The provisioned profile root must hold this account's auth only.
        let auth = docker_read(&phase.container, &format!("{root}/auth.json"))
            .or_else(|| docker_find_auth(&phase.container, &root));
        assert!(
            auth.is_some_and(|body| body.contains(CANARY_C)),
            "opencode state root must provision the oc-c credential"
        );
        for (name, value) in env.iter() {
            for forbidden in [CANARY_A, CANARY_B] {
                assert!(
                    !value.contains(forbidden),
                    "opencode pane leaks {forbidden} via {name}"
                );
            }
        }
    }
    Ok(())
}

fn assert_reconnect_snapshot(
    home: &Path,
    container: &str,
    phase: &PhaseAB,
    what: &str,
) -> Result<(), String> {
    let paths = JackinPaths::resolve_with_env(home, None, None);
    let Some(bindings) = snapshot_bindings(&paths, container) else {
        return Err(format!("{what}: no snapshot from container {container}"));
    };
    assert_eq!(
        bindings, phase.bindings,
        "{what} must preserve every pane binding"
    );
    Ok(())
}

fn observe_restore(home: &Path) -> Result<Vec<(String, String)>, String> {
    // Restore recreates the container and boots the default launch set;
    // session ids are fresh but every binding must be a known account.
    let new_container = wait_for_value(Duration::from_mins(6), "the restored container", || {
        running_container_name()
    })?;
    wait_for_value(Duration::from_mins(4), "restored bound sessions", || {
        let paths = JackinPaths::resolve_with_env(home, None, None);
        let bindings = snapshot_bindings(&paths, &new_container)?;
        (bindings.len() >= 3).then_some(bindings)
    })
}

fn assert_restore_bindings(bindings: &[(String, String)]) {
    let allowed = BTreeSet::from([
        ("cx-a-inst".to_owned(), "cx-a".to_owned()),
        ("cx-b-inst".to_owned(), "cx-b".to_owned()),
        ("oc-c-inst".to_owned(), "oc-c".to_owned()),
    ]);
    for binding in bindings {
        assert!(
            allowed.contains(binding),
            "restored pane carries unexpected binding {binding:?}"
        );
    }
    assert!(
        bindings.len() >= 3,
        "restore must boot at least the default launch set, got {bindings:?}"
    );
}

fn env_dumps(workspace: &Path) -> Vec<PathBuf> {
    let mut dumps = Vec::new();
    if let Ok(entries) = std::fs::read_dir(workspace) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("s3-env-") && name.ends_with(".txt") {
                dumps.push(entry.path());
            }
        }
    }
    dumps.sort();
    dumps
}

fn read_dumps(workspace: &Path) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for path in env_dumps(workspace) {
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let mut env = BTreeMap::new();
        for line in body.lines() {
            if let Some((key, value)) = line.split_once('=') {
                env.insert(key.to_owned(), value.to_owned());
            }
        }
        // The dump filename records which fake agent wrote it.
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let agent = stem
                .strip_prefix("s3-env-")
                .and_then(|rest| rest.rsplit_once('-').map(|(a, _)| a))
                .unwrap_or("?");
            env.insert("S3_FAKE_AGENT".to_owned(), agent.to_owned());
        }
        out.insert(path.display().to_string(), env);
    }
    out
}

fn docker_read(container: &str, path: &str) -> Option<String> {
    let output = Command::new("docker")
        .args(["exec", container, "cat", path])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn docker_find_auth(container: &str, root: &str) -> Option<String> {
    let output = Command::new("docker")
        .args([
            "exec",
            container,
            "sh",
            "-c",
            &format!("find {root} -name auth.json -exec cat {{}} + 2>/dev/null"),
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .filter(|body| !body.trim().is_empty())
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

fn write_config(config_dir: &Path, home: &Path, role_source: &Path, workspace_dir: &Path) {
    std::fs::create_dir_all(config_dir.join("workspaces")).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"version = "v1alpha12"

[accounts.cx-a]
name = "S3 Codex A"
provider = "openai"
[accounts.cx-a.credential]
type = "api_key"
value = "{CANARY_A}"
base_url = "{ENDPOINT_A}"
model = "{MODEL_A}"

[accounts.cx-b]
name = "S3 Codex B"
provider = "openai"
[accounts.cx-b.credential]
type = "api_key"
value = "{CANARY_B}"
base_url = "{ENDPOINT_B}"
model = "{MODEL_B}"

[accounts.oc-c]
name = "S3 Opencode C"
provider = "opencode"
[accounts.oc-c.credential]
type = "profile"
agent = "opencode"
directory = "{}"

[agent_configurations.cx-a-inst]
agent = "codex"
account = "cx-a"

[agent_configurations.cx-b-inst]
agent = "codex"
account = "cx-b"

[agent_configurations.oc-c-inst]
agent = "opencode"
account = "oc-c"

[roles."{ROLE_KEY}"]
git = "{}"
trusted = true
"#,
            home.join("opencode-c").display(),
            role_source.display(),
        ),
    )
    .unwrap();
    std::fs::write(
        config_dir
            .join("workspaces")
            .join(format!("{WORKSPACE}.toml")),
        format!(
            r#"version = "v1alpha10"
workdir = "/workspace"
default_role = "{ROLE_KEY}"
default_agent = "codex"
accounts = ["cx-a", "cx-b", "oc-c"]
default_launch = ["cx-a-inst", "cx-b-inst", "oc-c-inst"]

[[mounts]]
src = "{}"
dst = "/workspace"
readonly = false
isolation = "shared"
"#,
            workspace_dir.display(),
        ),
    )
    .unwrap();
}

fn seed_opencode_profile(home: &Path) {
    let root = home.join("opencode-c");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        format!(r#"{{"opencode-go":{{"type":"api","key":"{CANARY_C}"}}}}"#),
    )
    .unwrap();
    std::fs::write(root.join("S3_MARKER.txt"), "state-root-marker-opencode-c").unwrap();
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
agents = ["codex", "opencode"]

[identity]
name = "S3 Multi Account"

[codex]

[opencode]
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
            "Seed s3 multi-account e2e role",
        ],
        Some(path),
    );
}

/// A fake agent that numbers itself with an atomic `mkdir` sequence,
/// dumps its own environment for the host to inspect, announces
/// readiness on stdout, then idles reading stdin.
fn fake_agent_script(agent: &str) -> String {
    format!(
        r#"if [ "${{1:-}}" = "--version" ]; then
  echo "{agent} 0.0.0-s3"
  exit 0
fi
if [ "${{1:-}}" = "install" ]; then
  mkdir -p "$HOME/.local/bin"
  cp "$0" "$HOME/.local/bin/{agent}"
  chmod 0755 "$HOME/.local/bin/{agent}"
  exit 0
fi
i=0
while ! mkdir "/workspace/s3-seq-$i" 2>/dev/null; do
  i=$((i + 1))
done
ME="{agent}-$$"
env | sort > "/workspace/s3-env-$ME.txt"
echo "s3 agent #$i ready: $ME"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "/workspace/s3-got-$ME.txt"
done
"#
    )
}

fn seed_fake_agent(home: &Path, agent: &str) {
    let script = fake_agent_script(agent);
    let stub = home
        .join(".jackin")
        .join("cache")
        .join("agent-binaries-test-stub")
        .join(agent);
    std::fs::create_dir_all(stub.parent().unwrap()).unwrap();
    std::fs::write(&stub, format!("#!/bin/sh\nset -eu\n{script}")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();
    }
}
