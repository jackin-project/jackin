// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::UsageRefreshPhase;
use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
use jackin_usage::host::ensure_usage_broker_with_executor;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

use super::*;

#[test]
fn resolved_launch_inventory_deduplicates_only_launch_agents() {
    let config = jackin_protocol::CapsuleConfig {
        agents: vec!["claude".to_owned(), "codex".to_owned(), "claude".to_owned()],
        ..jackin_protocol::CapsuleConfig::default()
    };

    assert_eq!(
        resolved_launch_usage_inventory(&config).agents,
        ["claude", "codex"]
    );
}

#[tokio::test]
async fn docker_relay_guard_requests_graceful_shutdown_before_detach() -> Result<()> {
    let (shutdown, shutdown_rx) = oneshot::channel();
    let (observed, observed_rx) = oneshot::channel();
    let task = jackin_telemetry::spawn::spawn_stream("usage_relay.test_shutdown", async move {
        drop(shutdown_rx.await);
        let _observed = observed.send(());
    });
    let guard = UsageRelayGuard {
        task: Some(task),
        socket_path: None,
        shutdown: Some(shutdown),
    };

    drop(guard);

    tokio::time::timeout(std::time::Duration::from_secs(1), observed_rx).await??;
    Ok(())
}

#[tokio::test]
async fn docker_relay_guard_closes_child_stdin_and_reaps_proxy() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let marker = temp.path().join("proxy-exited");
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        executor,
    )
    .map_err(|error| anyhow::anyhow!("{:?}: {}", error.kind, error.message))?;
    let args: Vec<std::ffi::OsString> = vec![
        "-c".into(),
        "cat >/dev/null; : > \"$1\"".into(),
        "usage-relay-test".into(),
        marker.as_os_str().to_owned(),
    ];
    let request = jackin_process::ExecRequest::new("sh", args)
        .stdin_mode(jackin_process::StdioMode::Capture)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Inherit);
    let guard = start_tunnel_process(request, broker, vec![capability("allowed")])?;

    drop(guard);

    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

#[test]
fn usage_mount_uses_only_existing_runtime_directory_for_both_backends() {
    let socket_dir = PathBuf::from("/host/jackin/sockets/fixture");

    let docker = docker_runtime_mount(&socket_dir).unwrap();
    assert_eq!(docker, "/host/jackin/sockets/fixture:/jackin/run");
    assert!(!docker.contains("usage-shared"));

    let apple = apple_runtime_mount(socket_dir.clone());
    assert_eq!(apple.source, socket_dir);
    assert_eq!(apple.target, PathBuf::from("/jackin/run"));
    assert!(!apple.readonly);
    assert!(!apple.source.to_string_lossy().contains("usage-shared"));
}

#[test]
fn forwarded_sources_include_only_provisioned_profiles_and_governed_env() {
    use crate::instance::{
        AgentRuntimeState, AuthProvisionOutcome, GithubProvisionOutcome, ProvisionedAuth, RoleState,
    };
    use jackin_core::Agent;

    let temp = tempfile::tempdir().unwrap();
    let state = RoleState {
        root: temp.path().join("role"),
        gh_config_dir: temp.path().join("role/.config/gh"),
        gh_provision_outcome: GithubProvisionOutcome::Skipped,
        agent_runtime: AgentRuntimeState {
            agent: Agent::Claude,
            model: None,
        },
        auth: ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::from([
            (Agent::Claude, AuthProvisionOutcome::Synced),
            (Agent::Codex, AuthProvisionOutcome::HostMissing),
            (Agent::Amp, AuthProvisionOutcome::TokenMode),
        ]),
    };
    let resolved_env = jackin_env::ResolvedEnv {
        vars: vec![
            ("OPENAI_API_KEY".to_owned(), "secret".to_owned()),
            ("UNRELATED".to_owned(), "value".to_owned()),
        ],
    };

    let sources = forwarded_sources_from_launch(&state, &resolved_env);
    assert_eq!(
        sources.profile_surface_ids,
        BTreeSet::from(["claude".to_owned()])
    );
    assert_eq!(
        sources.env_keys,
        BTreeSet::from(["OPENAI_API_KEY".to_owned()])
    );
}

#[test]
fn hermetic_layout_never_starts_host_usage_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    fs::create_dir_all(&paths.config_dir).unwrap();
    let config = format!(
        "version = \"{}\"\n\n[env]\nZAI_API_KEY = \"synthetic-zai-key\"\n",
        jackin_config::CURRENT_CONFIG_VERSION,
    );
    fs::write(&paths.config_file, &config).unwrap();
    let forwarded_sources = ForwardedUsageSources {
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["ZAI_API_KEY".to_owned()]),
    };

    let (_, capabilities) =
        prepare_broker_client(&paths, Some("fixture"), "reviewer", &forwarded_sources);

    assert!(capabilities.is_empty());
    assert!(!paths.data_dir.exists());
    assert_eq!(fs::read_to_string(&paths.config_file).unwrap(), config);
}

struct CountingExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(quota_view())
    }
}

fn capability(account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn quota_view() -> FocusedUsageView {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut view = FocusedUsageView::unavailable("claude", i64::try_from(now).unwrap_or(i64::MAX));
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "allowed@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(55),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}

#[tokio::test]
async fn usage_relay_authorizes_only_exact_forwarded_account() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let socket = temp.path().join("usage.sock");
    let allowed = capability("allowed");
    let denied = capability("denied");
    let relay = start(socket.clone(), broker, vec![allowed.clone()]).unwrap();

    let denied_response = send(
        &socket,
        UsageBrokerOperation::Refresh {
            capability: denied,
            observed_generation: 0,
            force: true,
        },
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_response else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let denied_surface = send(
        &socket,
        UsageBrokerOperation::RefreshForSurface {
            surface_id: "codex".to_owned(),
            observed_generation: 0,
            force: true,
        },
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_surface else {
        panic!("denied surface returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let refresh = send(
        &socket,
        UsageBrokerOperation::RefreshForSurface {
            surface_id: "claude".to_owned(),
            observed_generation: 0,
            force: true,
        },
    )
    .await;
    let UsageBrokerResponse::State { state } = refresh else {
        panic!("allowed capability returned error");
    };
    let terminal = send(
        &socket,
        UsageBrokerOperation::JoinForSurface {
            surface_id: "claude".to_owned(),
            generation: state.generation,
            timeout_ms: 2_000,
        },
    )
    .await;
    let UsageBrokerResponse::State { state } = terminal else {
        panic!("allowed generation join returned error");
    };
    assert_eq!(state.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let metadata = fs::metadata(&socket).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    relay.abort();
}

#[tokio::test]
async fn usage_relay_bind_failure_is_inactive_and_never_probes() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let long_dir = temp.path().join("x".repeat(120));
    fs::create_dir(&long_dir).unwrap();
    let socket = long_dir.join("usage.sock");

    let guard = start_guard(socket.clone(), broker, vec![capability("allowed")]);

    assert!(guard.task.is_none());
    assert!(!socket.exists());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn usage_relay_impossible_socket_path_skips_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::resolve_with_env(temp.path(), None, None);
    let socket_dir = temp.path().join("x".repeat(120));

    let guard = prepare_for_container(UsageRelayLaunch {
        paths: &paths,
        workspace_name: Some("fixture"),
        role_key: "role",
        forwarded_sources: ForwardedUsageSources {
            profile_surface_ids: BTreeSet::from(["claude".to_owned()]),
            env_keys: BTreeSet::new(),
        },
        socket_dir,
    })
    .await
    .unwrap();

    assert!(guard.task.is_none());
    assert!(!guard.socket_path.as_ref().unwrap().exists());
}

async fn send(socket: &Path, operation: UsageBrokerOperation) -> UsageBrokerResponse {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        operation,
    };
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).await.unwrap();
    let mut response = Vec::new();
    BufReader::new(stream)
        .read_until(b'\n', &mut response)
        .await
        .unwrap();
    response.pop();
    serde_json::from_slice(&response).unwrap()
}
