// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn colliding_usage_unix_identities_fail_the_launch_closed() {
    let shared = jackin_protocol::SessionIdentity {
        uid: 2000,
        gid: 2000,
    };
    let mut launch_config = CapsuleConfig {
        instances: vec!["personal@codex".to_owned(), "work@codex".to_owned()],
        accounts: BTreeMap::from([
            ("personal@codex".to_owned(), "personal-openai".to_owned()),
            ("work@codex".to_owned(), "work-openai".to_owned()),
        ]),
        usage_capabilities: BTreeMap::from([
            (
                "personal@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "personal-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "work@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "work-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
        ]),
        instance_identities: BTreeMap::from([
            ("personal@codex".to_owned(), shared),
            ("work@codex".to_owned(), shared),
        ]),
        ..CapsuleConfig::default()
    };
    let canonical = CanonicalLaunchUsageCapabilities {
        by_account_surface: BTreeMap::from([
            (
                ("personal-openai".to_owned(), "codex".to_owned()),
                UsageAccountCapability {
                    account_id: "provider-personal".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                ("work-openai".to_owned(), "codex".to_owned()),
                UsageAccountCapability {
                    account_id: "provider-work".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
        ]),
    };

    let error = canonical
        .apply_to_launch_config(&mut launch_config)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("multiple usage instances share Unix identity"),
        "unexpected error: {error:?}"
    );
}

#[test]
fn distinct_usage_unix_identities_pass_the_launch_guard() {
    let mut launch_config = CapsuleConfig {
        instances: vec!["personal@codex".to_owned(), "work@codex".to_owned()],
        accounts: BTreeMap::from([
            ("personal@codex".to_owned(), "personal-openai".to_owned()),
            ("work@codex".to_owned(), "work-openai".to_owned()),
        ]),
        usage_capabilities: BTreeMap::from([
            (
                "personal@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "personal-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "work@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "work-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
        ]),
        instance_identities: BTreeMap::from([
            (
                "personal@codex".to_owned(),
                jackin_protocol::SessionIdentity {
                    uid: 2000,
                    gid: 2000,
                },
            ),
            (
                "work@codex".to_owned(),
                jackin_protocol::SessionIdentity {
                    uid: 2001,
                    gid: 2001,
                },
            ),
        ]),
        ..CapsuleConfig::default()
    };
    let canonical = CanonicalLaunchUsageCapabilities {
        by_account_surface: BTreeMap::from([
            (
                ("personal-openai".to_owned(), "codex".to_owned()),
                UsageAccountCapability {
                    account_id: "provider-personal".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                ("work-openai".to_owned(), "codex".to_owned()),
                UsageAccountCapability {
                    account_id: "provider-work".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
        ]),
    };

    canonical
        .apply_to_launch_config(&mut launch_config)
        .unwrap();
}

#[test]
fn existing_relay_keeps_materialized_capability_while_new_relay_excludes_it() {
    let removed = UsageAccountCapability {
        account_id: "removed-account".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let existing = UsageCapabilitySet::new([removed.clone()]);
    let recreated = UsageCapabilitySet::new(std::iter::empty());

    existing.authorize(&removed).unwrap();
    assert_eq!(
        recreated.authorize(&removed).unwrap_err().kind,
        UsageCoordinationErrorKind::Unauthorized
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
    let guard = start_tunnel_process(
        request,
        broker,
        vec![capability("allowed")],
        UsageCredentialScope::default(),
    )?;

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
fn usage_mounts_preserve_backend_transport_contract() {
    let socket_dir = PathBuf::from("/host/jackin/sockets/fixture");

    let docker = docker_runtime_mount(&socket_dir).unwrap();
    assert_eq!(docker, "/host/jackin/sockets/fixture:/jackin/run");
    assert!(!docker.contains("usage-shared"));

    let apple = apple_runtime_mount(socket_dir.clone());
    assert_eq!(
        apple.source,
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME)
    );
    assert_eq!(
        apple.target,
        PathBuf::from(jackin_protocol::CAPSULE_CONFIG_PATH)
    );
    assert!(apple.readonly);
}

#[test]
fn apple_usage_tunnel_executes_the_guest_proxy_as_the_supervisor() {
    assert_eq!(
        apple_tunnel_args("fixture"),
        [
            "exec",
            "-i",
            "--user",
            "0:0",
            "fixture",
            "/jackin/runtime/jackin-capsule",
            "usage-relay-proxy",
        ]
    );
}

#[test]
fn docker_usage_tunnel_binds_the_immutable_container_id() {
    let container = ContainerHandle::new("role-name", "immutable-id").unwrap();
    let args = docker_tunnel_args(&container, &["usage-relay-proxy".to_owned()]);

    assert_eq!(args[2], "immutable-id");
    assert_ne!(args[2], container.name());
}

#[test]
fn forwarded_sources_include_only_provisioned_profiles_and_governed_env() {
    use jackin_core::Agent;
    use jackin_instance::{
        AgentRuntimeState, AuthProvisionOutcome, GithubProvisionOutcome, ProvisionedAuth, RoleState,
    };

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
        auth_outcomes: BTreeMap::from([
            (Agent::Claude, AuthProvisionOutcome::Synced),
            (Agent::Codex, AuthProvisionOutcome::HostMissing),
            (Agent::Amp, AuthProvisionOutcome::TokenMode),
        ]),
        auth_mount_paths: BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    };
    let resolved_env = jackin_env::ResolvedEnv {
        vars: vec![
            ("OPENAI_API_KEY".to_owned(), "secret".to_owned()),
            ("GOOGLE_API_KEY".to_owned(), "alias-secret".to_owned()),
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
        BTreeSet::from(["GOOGLE_API_KEY".to_owned(), "OPENAI_API_KEY".to_owned(),])
    );
}

#[test]
fn hermetic_layout_never_starts_or_queries_the_usage_broker() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    fs::create_dir_all(&paths.config_dir).unwrap();
    let config = format!(
        "version = \"{}\"\n\n[env]\nZAI_API_KEY = \"synthetic-zai-key\"\n",
        jackin_config::CURRENT_CONFIG_VERSION,
    );
    fs::write(&paths.config_file, &config).unwrap();
    let forwarded_sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::new(),
        selected_account_surfaces: BTreeMap::new(),
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["ZAI_API_KEY".to_owned()]),
        credential_scope: UsageCredentialScope::default(),
    };

    let (_, capabilities, _) =
        prepare_broker_client(&paths, Some("fixture"), "reviewer", &forwarded_sources).unwrap();

    assert!(capabilities.is_empty());
    assert!(!paths.data_dir.exists());
    assert_eq!(fs::read_to_string(&paths.config_file).unwrap(), config);
}

#[tokio::test]
async fn usage_relay_denies_every_monitor_operation_including_operator_claims() {
    use jackin_protocol::usage_monitor::{
        MonitorAccountBindingInput, MonitorConfig, MonitorOperation, MonitorPolicy,
        MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorScope,
        SpendRecordInput, SpendRecordSource, StatuslineObservation,
    };

    let temp = tempfile::tempdir().unwrap();
    // A broker client without a running broker proves denial occurs at the
    // relay boundary: any accidentally forwarded request returns Unavailable.
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("data")).client();
    let allowlist = UsageCapabilitySet::new([]);
    let binding_id = "operator-confirmed-binding".to_owned();

    let requests = [
        MonitorOperation::BindAccount {
            binding: MonitorAccountBindingInput {
                provider: MonitorProvider::Claude,
                account_id: "local-account".to_owned(),
                operator_label: "work Claude account".to_owned(),
                operator_confirmed: true,
                provider_account_id: None,
                experimental_collector_approved: false,
            },
        },
        MonitorOperation::ApprovePolicy {
            approval: MonitorPolicyApprovalInput {
                binding_id: binding_id.clone(),
                binding_revision: 1,
                goal_id: "approved-goal".to_owned(),
                new_policy: MonitorPolicy::StrictSgd,
                budget: Some(jackin_protocol::control::Money::new(5_000, "SGD", 2)),
                operator_label: "explicit strict approval".to_owned(),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: false,
                expected_revision: None,
            },
        },
        MonitorOperation::ApprovePolicy {
            approval: MonitorPolicyApprovalInput {
                binding_id: binding_id.clone(),
                binding_revision: 1,
                goal_id: "approved-goal".to_owned(),
                new_policy: MonitorPolicy::QuotaOnly,
                budget: None,
                operator_label: "explicit quota-only approval".to_owned(),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: true,
                expected_revision: None,
            },
        },
        MonitorOperation::Start {
            config: MonitorConfig {
                provider: MonitorProvider::Claude,
                purpose: MonitorPurpose::ObserveOnly,
                scope: MonitorScope::Session {
                    session_id: "unbound-session".to_owned(),
                },
                goal_id: None,
                expected_model: None,
                policy_revision: None,
                experimental_collector: false,
            },
            idempotency_key: "observe-only-start".to_owned(),
        },
        MonitorOperation::Start {
            config: MonitorConfig {
                provider: MonitorProvider::Claude,
                purpose: MonitorPurpose::DispatchGuard,
                scope: MonitorScope::BoundAccount {
                    binding_id: binding_id.clone(),
                    binding_revision: 1,
                    session_id: Some("bound-session".to_owned()),
                },
                goal_id: Some("approved-goal".to_owned()),
                expected_model: Some("claude-fixture".to_owned()),
                policy_revision: Some(1),
                experimental_collector: false,
            },
            idempotency_key: "dispatch-start".to_owned(),
        },
        MonitorOperation::Stop {
            monitor_id: "monitor-1".to_owned(),
        },
        MonitorOperation::Status {
            monitor_id: "monitor-1".to_owned(),
        },
        MonitorOperation::Doctor {
            provider: MonitorProvider::Claude,
        },
        MonitorOperation::Ingest {
            scope: MonitorScope::Session {
                session_id: "unbound-session".to_owned(),
            },
            observation: StatuslineObservation {
                schema_version:
                    jackin_protocol::usage_monitor::USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
                session_id: "unbound-session".to_owned(),
                ..StatuslineObservation::default()
            },
        },
        MonitorOperation::RecordSpend {
            record: SpendRecordInput {
                account_id: "local-account".to_owned(),
                billing_period_start_epoch: 1,
                billing_period_end_epoch: 2,
                amount: jackin_protocol::control::Money::new(0, "SGD", 2),
                evidence_at_epoch: None,
                verified: true,
                source: SpendRecordSource::OperatorReceipt,
            },
        },
        MonitorOperation::Refresh {
            monitor_id: "monitor-1".to_owned(),
        },
        MonitorOperation::ServiceStatus,
        MonitorOperation::ServiceStop,
        MonitorOperation::Watch {
            monitor_id: "monitor-1".to_owned(),
            after_sequence: 0,
            timeout_ms: 0,
        },
    ];

    for request in requests {
        let description = format!("{request:?}");
        let denied = dispatch(
            UsageBrokerOperation::Monitor { request },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::Error { error } = denied else {
            panic!("monitor operation was forwarded: {description}");
        };
        assert_eq!(
            error.kind,
            UsageCoordinationErrorKind::Unauthorized,
            "monitor operation crossed the relay boundary: {description}"
        );
    }
}
