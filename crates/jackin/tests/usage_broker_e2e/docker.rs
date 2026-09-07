// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::symlink;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use jackin_protocol::usage_broker::{UsageBrokerResponse, UsageCoordinationErrorKind};
use jackin_usage::coordinator::{
    AccountStateEnvelope, AccountStateStore, FileAccountStateStore, ProviderProbeOutcome,
    UsageCoordinatorConfig, UsageProviderExecutor,
};
use jackin_usage::host::{UsageBrokerClient, UsageBrokerConfig, ensure_usage_broker_with_executor};

use super::*;

const CAPSULE_IMAGE: &str = "python:3.14-alpine";
const TUNNEL_PROXY_SCRIPT: &str = r#"
import json, os, socket, sys

path = "/jackin/run/usage.sock"
try:
    os.unlink(path)
except FileNotFoundError:
    pass
listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
listener.bind(path)
os.chmod(path, 0o600)
listener.listen(128)
with open("/jackin/run/proxy-ready", "w", encoding="utf-8") as marker:
    marker.write("ready")

request_id = 1
while True:
    client, _ = listener.accept()
    chunks = []
    while True:
        chunk = client.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
        if b"\n" in chunk:
            break
    request = json.loads(b"".join(chunks))
    tunneled = {"request_id": request_id, "request": request}
    sys.stdout.write(json.dumps(tunneled, separators=(",", ":")) + "\n")
    sys.stdout.flush()
    response = json.loads(sys.stdin.readline())
    assert response["request_id"] == request_id, response
    client.sendall((json.dumps(response["response"], separators=(",", ":")) + "\n").encode())
    client.close()
    request_id += 1
"#;
const CAPSULE_SCRIPT: &str = r#"
import json, os, socket, time

path = "/jackin/run/usage.sock"
assert not os.path.exists("/jackin/usage-shared")
assert not any(key.startswith("JACKIN_USAGE_") and key.endswith("_DIR") for key in os.environ)

def call(operation):
    request = {
        "protocol_version": "v1",
        "build_id": os.environ["JACKIN_USAGE_E2E_BUILD"],
        "operation": operation,
    }
    for attempt in range(200):
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(30)
        try:
            client.connect(path)
            break
        except (ConnectionRefusedError, FileNotFoundError):
            client.close()
            if attempt == 199:
                raise
            time.sleep(0.025)
    client.sendall((json.dumps(request, separators=(",", ":")) + "\n").encode())
    client.shutdown(socket.SHUT_WR)
    chunks = []
    while True:
        chunk = client.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
    client.close()
    return json.loads(b"".join(chunks))

mode = os.environ["JACKIN_USAGE_E2E_MODE"]
if mode == "gated-refresh":
    with open("/jackin/run/client-ready", "w", encoding="utf-8") as marker:
        marker.write("ready")
    deadline = time.monotonic() + 45
    while not os.path.exists("/jackin/run/client-go"):
        assert time.monotonic() < deadline, "client launch barrier timed out"
        time.sleep(0.01)
    mode = "refresh"
if mode == "refresh":
    initial = call({
        "operation": "refresh_for_surface",
        "surface_id": "claude",
        "observed_generation": 0,
        "force": True,
    })
    assert initial["status"] == "state", initial
    with open("/jackin/run/requested", "w", encoding="utf-8") as marker:
        marker.write(str(initial["state"]["generation"]))
    response = call({
        "operation": "join_for_surface",
        "surface_id": "claude",
        "generation": initial["state"]["generation"],
        "timeout_ms": 30000,
    })
elif mode == "request":
    response = call({
        "operation": "refresh_for_surface",
        "surface_id": "claude",
        "observed_generation": 0,
        "force": True,
    })
    assert response["status"] == "state", response
    assert response["state"]["phase"] in ("queued", "updating"), response
elif mode == "unauthorized":
    response = call({
        "operation": "current",
        "capability": {"account_id": "account-b", "surface_id": "claude"},
    })
    assert response["status"] == "error", response
    assert response["error"]["kind"] == "unauthorized", response
    missing = call({
        "operation": "current_for_surface",
        "surface_id": "codex",
    })
    assert missing["status"] == "error", missing
    assert missing["error"]["kind"] == "unauthorized", missing
else:
    raise AssertionError(mode)

print(json.dumps(response, separators=(",", ":")))
"#;

struct GateProvider {
    root: PathBuf,
    calls: Arc<AtomicUsize>,
}

impl UsageProviderExecutor for GateProvider {
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        fs::write(
            self.root.join(format!(
                "provider-started-{}-{generation}",
                capability.account_id
            )),
            b"started\n",
        )
        .map_or_else(
            |_| ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fake provider barrier is unavailable".to_owned(),
                retry_at_epoch: None,
            },
            |()| {
                wait_until(Duration::from_secs(30), || {
                    self.root.join("release").exists()
                });
                ProviderProbeOutcome::success(quota_view())
            },
        )
    }
}

struct FailureProvider {
    calls: Arc<AtomicUsize>,
    retry_at_epoch: i64,
}

impl UsageProviderExecutor for FailureProvider {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "provider rate limited".to_owned(),
            retry_at_epoch: Some(self.retry_at_epoch),
        }
    }
}

#[tokio::test]
async fn usage_broker_desktop_and_two_docker_capsules_make_one_provider_call() -> Result<()> {
    assert_desktop_capsule_singleflight(2).await
}

#[tokio::test]
async fn usage_broker_desktop_and_twenty_docker_capsules_make_one_provider_call() -> Result<()> {
    assert_desktop_capsule_singleflight(20).await
}

#[tokio::test]
async fn usage_broker_capsule_refresh_is_same_updating_generation_in_desktop() -> Result<()> {
    require_docker()?;
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let broker = broker_with_gate(&root, Arc::clone(&calls), 4)?;
    let capsule = start_capsule(&root, 0, broker.clone()).await?;
    let capsule_name = capsule.name.clone();
    let request = tokio::task::spawn_blocking(move || run_capsule(&capsule_name, "refresh"));

    wait_for_async(Duration::from_secs(30), || {
        root.join("relay-0/requested").exists()
    })
    .await;
    let desktop = broker.current(capability()).test_result()?;
    assert_eq!(desktop.generation, 1);
    assert_eq!(desktop.phase, UsageRefreshPhase::Updating);
    fs::write(root.join("release"), b"release\n")?;
    let terminal = broker
        .join(capability(), desktop.generation, Duration::from_secs(30))
        .test_result()?;
    let response = request.await??;
    assert_eq!(
        response,
        UsageBrokerResponse::State {
            state: Box::new(terminal)
        }
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(capsule);
    Ok(())
}

#[tokio::test]
async fn usage_broker_docker_capsule_cannot_access_another_account_or_global_tree() -> Result<()> {
    require_docker()?;
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let broker = broker_with_gate(&root, Arc::clone(&calls), 4)?;
    let capsule = start_capsule(&root, 0, broker).await?;
    let capsule_name = capsule.name.clone();
    let response =
        tokio::task::spawn_blocking(move || run_capsule(&capsule_name, "unauthorized")).await??;
    let UsageBrokerResponse::Error { error } = response else {
        bail!("unauthorized capsule received account state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(capsule);
    Ok(())
}

#[test]
fn usage_broker_timeout_holds_ownership_until_provider_returns() -> Result<()> {
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = UsageBrokerConfig::for_data_dir(root.join("data"));
    config.coordinator.provider_timeout = Duration::from_millis(25);
    let provider: Arc<dyn UsageProviderExecutor> = Arc::new(GateProvider {
        root: root.clone(),
        calls: Arc::clone(&calls),
    });
    let broker = ensure_usage_broker_with_executor(config, provider).test_result()?;
    let active = broker.refresh(capability(), 0, true).test_result()?;
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(&root, "provider-started-") == 1
    });
    let Err(error) = broker.join(capability(), active.generation, Duration::from_millis(50)) else {
        bail!("broker wait unexpectedly completed before provider timeout");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);
    std::thread::park_timeout(Duration::from_millis(50));
    assert_eq!(
        broker.current(capability()).test_result()?.phase,
        UsageRefreshPhase::Updating
    );
    fs::write(root.join("release"), b"release\n")?;
    let terminal = broker
        .join(capability(), active.generation, Duration::from_secs(5))
        .test_result()?;
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    let error = terminal.error.context("timeout terminal lacked error")?;
    assert_eq!(error.kind, UsageCoordinationErrorKind::ProviderTimeout);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn usage_broker_distinct_accounts_run_concurrently_within_bound() -> Result<()> {
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = UsageBrokerConfig::for_data_dir(root.join("data"));
    config.coordinator.max_concurrency = 2;
    let provider: Arc<dyn UsageProviderExecutor> = Arc::new(GateProvider {
        root: root.clone(),
        calls: Arc::clone(&calls),
    });
    let broker = ensure_usage_broker_with_executor(config, provider).test_result()?;
    let account_a = capability();
    let account_b = UsageAccountCapability {
        account_id: "account-b".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let first = broker.refresh(account_a.clone(), 0, true).test_result()?;
    let second = broker.refresh(account_b.clone(), 0, true).test_result()?;
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(&root, "provider-started-") == 2
    });
    fs::write(root.join("release"), b"release\n")?;
    assert_eq!(
        broker
            .join(account_a, first.generation, Duration::from_secs(5))
            .test_result()?
            .phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(
        broker
            .join(account_b, second.generation, Duration::from_secs(5))
            .test_result()?
            .phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters() -> Result<()> {
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let data_dir = root.join("data");
    let now = now_epoch();
    let retry_at = now + 300;
    let store = FileAccountStateStore::under_data_dir(&data_dir);
    let mut seeded = AccountStateEnvelope::idle(capability());
    seeded.generation = 1;
    seeded.phase = UsageRefreshPhase::Completed;
    seeded.terminal_result = Some(quota_view());
    seeded.last_good = seeded.terminal_result.clone();
    seeded.completed_at_epoch = Some(now);
    store.store(&seeded, now)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let provider: Arc<dyn UsageProviderExecutor> = Arc::new(FailureProvider {
        calls: Arc::clone(&calls),
        retry_at_epoch: retry_at,
    });
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(data_dir.clone()),
        provider,
    )
    .test_result()?;
    let active = broker.refresh(capability(), 1, true).test_result()?;
    let terminal = broker
        .join(capability(), active.generation, Duration::from_secs(5))
        .test_result()?;
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(terminal.retry_at_epoch, Some(retry_at));
    assert!(terminal.snapshot.is_some());
    let waiters = (0..8)
        .map(|_| {
            let broker = broker.clone();
            let terminal = terminal.clone();
            std::thread::spawn(move || -> Result<()> {
                assert_eq!(
                    broker
                        .join(capability(), terminal.generation, Duration::from_secs(5))
                        .test_result()?,
                    terminal
                );
                Ok(())
            })
        })
        .collect::<Vec<_>>();
    for waiter in waiters {
        waiter
            .join()
            .map_err(|_| anyhow::anyhow!("waiter panicked"))??;
    }
    let suppressed = broker
        .refresh(capability(), terminal.generation, true)
        .test_result()?;
    assert_eq!(suppressed.generation, terminal.generation);
    let envelope = store
        .load(&capability(), now)?
        .context("persisted account state was missing")?;
    assert_eq!(envelope.consecutive_failures, 1);
    assert_eq!(envelope.rate_limit_deadline_epoch, Some(retry_at));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn usage_broker_unavailable_state_makes_zero_provider_calls() -> Result<()> {
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let target = root.join("target");
    fs::create_dir(&target)?;
    let data_dir = root.join("data");
    symlink(&target, &data_dir)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let provider: Arc<dyn UsageProviderExecutor> = Arc::new(GateProvider {
        root,
        calls: Arc::clone(&calls),
    });
    let Err(error) =
        ensure_usage_broker_with_executor(UsageBrokerConfig::for_data_dir(data_dir), provider)
    else {
        bail!("broker accepted a symlinked state directory");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

async fn assert_desktop_capsule_singleflight(capsules: usize) -> Result<()> {
    require_docker()?;
    let temp = short_tempdir()?;
    let root = temp.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let broker = broker_with_gate(&root, Arc::clone(&calls), 4)?;
    // Container startup is outside provider execution. Every capsule must be
    // ready before opening the shared generation, so Docker provisioning cost
    // cannot consume the provider's timeout or replace concurrent waiters with
    // a sequence of short-lived clients.
    let mut containers = Vec::new();
    let mut relay_dirs = Vec::new();
    for index in 0..capsules {
        let container = start_capsule(&root, index, broker.clone()).await?;
        relay_dirs.push(container.relay_dir.clone());
        containers.push(container);
    }
    let mut tasks = Vec::new();
    for container in &containers {
        let container_name = container.name.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            run_capsule(&container_name, "gated-refresh")
        }));
    }
    // docker exec and Python startup are provisioning too. Readiness proves
    // every client process is already waiting before provider timing starts.
    wait_for_async(Duration::from_secs(45), || {
        relay_dirs
            .iter()
            .all(|dir| dir.join("client-ready").exists())
    })
    .await;
    let active = broker.refresh(capability(), 0, true).test_result()?;
    wait_for_async(Duration::from_secs(10), || {
        entries_with_prefix(&root, "provider-started-") == 1
    })
    .await;
    for directory in &relay_dirs {
        fs::write(directory.join("client-go"), b"go\n")?;
    }
    wait_for_async(Duration::from_secs(45), || {
        relay_dirs.iter().all(|dir| dir.join("requested").exists())
    })
    .await;
    let desktop = broker.current(capability()).test_result()?;
    assert_eq!(desktop.generation, active.generation);
    assert_eq!(desktop.phase, UsageRefreshPhase::Updating);
    fs::write(root.join("release"), b"release\n")?;
    let terminal = broker
        .join(capability(), active.generation, Duration::from_secs(30))
        .test_result()?;
    assert_eq!(
        terminal.phase,
        UsageRefreshPhase::Completed,
        "shared generation failed for {capsules} capsules: generation={}, error={:?}, provider_calls={}",
        terminal.generation,
        terminal.error,
        calls.load(Ordering::SeqCst)
    );
    let expected = UsageBrokerResponse::State {
        state: Box::new(terminal),
    };
    for task in tasks {
        assert_eq!(task.await??, expected);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(containers);
    Ok(())
}

fn broker_with_gate(
    root: &Path,
    calls: Arc<AtomicUsize>,
    max_concurrency: usize,
) -> Result<UsageBrokerClient> {
    let mut config = UsageBrokerConfig::for_data_dir(root.join("data"));
    config.coordinator = UsageCoordinatorConfig {
        max_concurrency,
        provider_timeout: Duration::from_secs(25),
        ..UsageCoordinatorConfig::default()
    };
    let provider: Arc<dyn UsageProviderExecutor> = Arc::new(GateProvider {
        root: root.to_path_buf(),
        calls,
    });
    ensure_usage_broker_with_executor(config, provider).test_result()
}

async fn start_capsule(
    root: &Path,
    index: usize,
    broker: UsageBrokerClient,
) -> Result<DockerCapsule> {
    let relay_dir = root.join(format!("relay-{index}"));
    fs::create_dir(&relay_dir)?;
    let name = format!("jackin-usage-e2e-{}-{index}", std::process::id());
    let mount = format!("type=bind,src={},dst=/jackin/run", relay_dir.display());
    let output = jackin_process::exec_async(&jackin_process::ExecRequest::new(
        "docker",
        [
            "run",
            "--detach",
            "--rm",
            "--name",
            &name,
            "--mount",
            &mount,
            CAPSULE_IMAGE,
            "sleep",
            "120",
        ],
    ))
    .await?;
    ensure!(
        output.success,
        "starting test Capsule failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let proxy_command = vec![
        "python".to_owned(),
        "-u".to_owned(),
        "-c".to_owned(),
        TUNNEL_PROXY_SCRIPT.to_owned(),
    ];
    let relay = jackin_runtime::usage_relay::start_docker_tunnel_with_command(
        &name,
        broker,
        vec![capability()],
        &proxy_command,
    )?;
    wait_for_async(Duration::from_secs(10), || {
        relay_dir.join("proxy-ready").exists()
    })
    .await;
    Ok(DockerCapsule {
        name,
        relay_dir,
        _relay: relay,
    })
}

struct DockerCapsule {
    name: String,
    relay_dir: PathBuf,
    _relay: jackin_runtime::usage_relay::UsageRelayGuard,
}

impl Drop for DockerCapsule {
    fn drop(&mut self) {
        let request = jackin_process::ExecRequest::new("docker", ["rm", "--force", &self.name]);
        drop(jackin_process::exec_sync(&request));
    }
}

fn run_capsule(container_name: &str, mode: &str) -> Result<UsageBrokerResponse> {
    let mode_env = format!("JACKIN_USAGE_E2E_MODE={mode}");
    let build_env = format!("JACKIN_USAGE_E2E_BUILD={}", env!("CARGO_PKG_VERSION"));
    let output = jackin_process::exec_sync(&jackin_process::ExecRequest::new(
        "docker",
        [
            "exec",
            "--env",
            &mode_env,
            "--env",
            &build_env,
            container_name,
            "python",
            "-c",
            CAPSULE_SCRIPT,
        ],
    ))?;
    ensure!(
        output.success,
        "container client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let response = stdout
        .lines()
        .last()
        .context("container client emitted no response")?;
    serde_json::from_str(response).context("decoding container broker response")
}

fn require_docker() -> Result<()> {
    let output = jackin_process::exec_sync(&jackin_process::ExecRequest::new(
        "docker",
        ["info", "--format", "{{.OperatingSystem}}"],
    ))
    .context("Docker command must be installed for this mandatory lane")?;
    ensure!(
        output.success,
        "Docker daemon must be running for this mandatory lane"
    );
    let operating_system = String::from_utf8(output.stdout)?;
    ensure!(
        !operating_system.trim().is_empty(),
        "Docker daemon reported an empty operating system"
    );
    Ok(())
}

fn short_tempdir() -> Result<tempfile::TempDir> {
    let preferred = Path::new("/private/tmp");
    let base = if preferred.is_dir() {
        preferred.to_path_buf()
    } else {
        std::env::temp_dir()
    };
    tempfile::Builder::new()
        .prefix("jue")
        .tempdir_in(base)
        .context("creating short usage broker E2E directory")
}

async fn wait_for_async(timeout: Duration, condition: impl Fn() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < timeout, "timed out waiting for barrier");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}
