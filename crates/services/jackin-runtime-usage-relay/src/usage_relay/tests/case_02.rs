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
