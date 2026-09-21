// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageCoordinationError, UsageCoordinationErrorKind, UsageCredentialSourceIdentity,
    UsageRefreshPhase, usage_credential_material_fingerprint,
};
use jackin_usage::coordinator::{ProviderProbeOutcome, UsageCapabilitySet, UsageProviderExecutor};
use jackin_usage::host::{
    CachedProviderCredentialResolver, UsageDiscoveryScope, discover_usage_sources,
    ensure_usage_broker_with_executor, validate_usage_sources,
};

#[test]
fn resolved_launch_inventory_is_closed_to_manifest_not_catalog() {
    let config = CapsuleConfig {
        instances: vec![
            "work@claude".to_owned(),
            "work@codex".to_owned(),
            "work@claude".to_owned(),
        ],
        usage_capabilities: BTreeMap::from([(
            "unlisted".to_owned(),
            capability("unlisted-account"),
        )]),
        ..CapsuleConfig::default()
    };

    assert_eq!(
        resolved_launch_usage_inventory(&config).instances,
        ["work@claude", "work@codex"]
    );
}

#[test]
fn launch_usage_capabilities_preserve_account_identity_and_provider_surface() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    for (id, provider) in [
        ("personal-openai", AiProvider::OpenAi),
        ("work-openai", AiProvider::OpenAi),
        ("routed-zai", AiProvider::Zai),
    ] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: id.to_owned(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: "fixture-key".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
    }

    let mut launch_config = CapsuleConfig {
        instances: vec![
            "personal-codex".to_owned(),
            "work-codex".to_owned(),
            "routed-codex".to_owned(),
        ],
        agents: BTreeMap::from([
            ("personal-codex".to_owned(), "codex".to_owned()),
            ("work-codex".to_owned(), "codex".to_owned()),
            ("routed-codex".to_owned(), "codex".to_owned()),
        ]),
        accounts: BTreeMap::from([
            ("personal-codex".to_owned(), "personal-openai".to_owned()),
            ("work-codex".to_owned(), "work-openai".to_owned()),
            ("routed-codex".to_owned(), "routed-zai".to_owned()),
        ]),
        ..CapsuleConfig::default()
    };

    populate_launch_usage_capabilities(&config, &mut launch_config);

    assert_eq!(
        launch_config.usage_capabilities,
        BTreeMap::from([
            (
                "personal-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "personal-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "work-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "work-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "routed-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "routed-zai".to_owned(),
                    surface_id: "zai".to_owned(),
                },
            ),
        ])
    );
}

#[test]
fn staged_scope_pins_zhipu_source_identity_and_material() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    config.accounts.insert(
        "zhipu".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Zhipu".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/field".to_owned(),
                    path: "Vault/Item/Field".to_owned(),
                    account: Some("work".to_owned()),
                    on_demand: false,
                }),
                base_url: None,
                model: None,
            },
        },
    );
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "zhipu-opencode".to_owned(),
        agent: jackin_core::Agent::Opencode,
        account_id: "zhipu".to_owned(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "Zhipu".to_owned(),
        synthesized: false,
    }];
    let credentials = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([(
        "zhipu-opencode".to_owned(),
        jackin_protocol::InstanceCredentialEnv {
            agent: "opencode".to_owned(),
            account_id: "zhipu".to_owned(),
            env: BTreeMap::from([("ZHIPU_API_KEY".to_owned(), "S1".to_owned())]),
        },
    )]));

    let scope = usage_credential_scope_for_staged_launch(&config, &instances, &credentials)?;
    assert_eq!(scope.sources.len(), 1);
    let proof = scope.sources.iter().next().expect("one Zhipu proof");
    assert_eq!(proof.key, "ZHIPU_API_KEY");
    assert_eq!(proof.account_id, "zhipu");
    assert_eq!(proof.surface_id, "zai");
    assert_eq!(
        proof.source,
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://vault/item/field".to_owned(),
            account: Some("work".to_owned()),
        }
    );
    assert_eq!(
        proof.material_fingerprint,
        usage_credential_material_fingerprint("S1")
    );
    Ok(())
}

#[test]
fn staged_scope_audits_one_account_across_mixed_agent_consumers() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    config.accounts.insert(
        "shared-zai".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Shared Z.AI".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/shared/field".to_owned(),
                    path: "Vault/Shared/Field".to_owned(),
                    account: Some("work".to_owned()),
                    on_demand: false,
                }),
                base_url: None,
                model: Some("glm-4.5".to_owned()),
            },
        },
    );
    let instances = vec![
        jackin_config::ResolvedInstance {
            config_id: "shared-claude".to_owned(),
            agent: jackin_core::Agent::Claude,
            account_id: "shared-zai".to_owned(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Shared Claude".to_owned(),
            synthesized: false,
        },
        jackin_config::ResolvedInstance {
            config_id: "shared-codex".to_owned(),
            agent: jackin_core::Agent::Codex,
            account_id: "shared-zai".to_owned(),
            model: Some("glm-4.5".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "Shared Codex".to_owned(),
            synthesized: false,
        },
        jackin_config::ResolvedInstance {
            config_id: "shared-opencode".to_owned(),
            agent: jackin_core::Agent::Opencode,
            account_id: "shared-zai".to_owned(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Shared OpenCode".to_owned(),
            synthesized: false,
        },
    ];
    let credentials = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([
        (
            "shared-claude".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "claude".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
        (
            "shared-codex".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "codex".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::OPENAI_API_KEY_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
        (
            "shared-opencode".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "opencode".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::ZHIPU_API_KEY_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
    ]));

    let scope = usage_credential_scope_for_staged_launch(&config, &instances, &credentials)?;
    assert_eq!(scope.sources.len(), 3);
    assert_eq!(
        scope
            .sources
            .iter()
            .map(|proof| proof.key.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
            jackin_core::OPENAI_API_KEY_ENV_NAME,
            jackin_core::ZHIPU_API_KEY_ENV_NAME,
        ])
    );
    assert!(
        scope
            .sources
            .iter()
            .all(|proof| proof.account_id == "shared-zai" && proof.surface_id == "zai")
    );
    Ok(())
}

#[test]
fn launch_discovery_relay_uses_distinct_canonical_ids_for_same_surface() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let temp = tempfile::tempdir()?;
    let config_root = temp.path().join("config");
    let home = temp.path().join("home");
    fs::create_dir_all(&config_root)?;
    let mut config = AppConfig::default();
    for (id, name, account_id, token) in [
        (
            "personal-openai",
            "Personal",
            "provider-personal",
            "fixture-personal-token",
        ),
        ("work-openai", "Work", "provider-work", "fixture-work-token"),
    ] {
        let profile = temp.path().join(id);
        fs::create_dir_all(&profile)?;
        fs::write(
            profile.join("auth.json"),
            format!(r#"{{"tokens":{{"access_token":"{token}","account_id":"{account_id}"}}}}"#),
        )?;
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: name.to_owned(),
                provider: AiProvider::OpenAi,
                credential: AccountCredential::Profile {
                    agent: jackin_core::Agent::Codex,
                    directory: profile,
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    fs::write(config_root.join("config.toml"), toml::to_string(&config)?)?;

    let resolver = CachedProviderCredentialResolver::new(RuntimeSecretSource);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &resolver,
    )
    .map_err(anyhow::Error::msg)?;
    let discovery = validate_usage_sources(catalog, &resolver);
    let sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from([
            "personal-openai".to_owned(),
            "work-openai".to_owned(),
        ]),
        selected_account_surfaces: BTreeMap::from([
            ("personal-openai".to_owned(), "codex".to_owned()),
            ("work-openai".to_owned(), "codex".to_owned()),
        ]),
        profile_surface_ids: BTreeSet::from(["codex".to_owned()]),
        env_keys: BTreeSet::new(),
        credential_scope: UsageCredentialScope::default(),
    };
    let forwarded = forwarded_usage_capabilities(&discovery, "unrelated scope", &sources);
    assert_eq!(forwarded.len(), 2);
    assert!(
        forwarded
            .iter()
            .all(|capability| capability.surface_id == "codex")
    );

    let allowed = forwarded.iter().cloned().collect::<BTreeSet<_>>();
    let canonical = canonical_capabilities_for_launch(&discovery, &sources, &allowed);
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
        ..CapsuleConfig::default()
    };

    canonical
        .apply_to_launch_config(&mut launch_config)
        .unwrap();
    let personal = &launch_config.usage_capabilities["personal@codex"];
    let work = &launch_config.usage_capabilities["work@codex"];
    assert_eq!(personal.surface_id, "codex");
    assert_eq!(work.surface_id, "codex");
    assert_ne!(personal.account_id, "personal-openai");
    assert_ne!(work.account_id, "work-openai");
    assert_ne!(personal.account_id, work.account_id);
    assert!(matches!(
        UsageCapabilitySet::new(forwarded).authorize(personal),
        Ok(())
    ));
    assert!(matches!(
        UsageCapabilitySet::new(allowed).authorize(&UsageAccountCapability {
            account_id: "personal-openai".to_owned(),
            surface_id: "codex".to_owned(),
        }),
        Err(error) if error.kind == UsageCoordinationErrorKind::Unauthorized
    ));
    Ok(())
}

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
        auth_outcomes: BTreeMap::from([
            (Agent::Claude, AuthProvisionOutcome::Synced),
            (Agent::Codex, AuthProvisionOutcome::HostMissing),
            (Agent::Amp, AuthProvisionOutcome::TokenMode),
        ]),
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

struct CountingExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for CountingExecutor {
    fn authorize_credential_scope(
        &self,
        _capability: &UsageAccountCapability,
        _scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

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
async fn usage_relay_stdio_dispatch_scopes_exact_capability() {
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
    let allowed = capability("allowed");
    let allowlist = UsageCapabilitySet::new([allowed.clone()]);

    let denied_response = dispatch(
        UsageBrokerOperation::Refresh {
            capability: capability("denied"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_response else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let denied_capability = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("denied"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_capability else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let refresh = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: allowed.clone(),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::State { state } = refresh else {
        panic!("allowed capability returned error");
    };
    let terminal = dispatch(
        UsageBrokerOperation::JoinForCapability {
            capability: allowed,
            generation: state.generation,
            timeout_ms: 2_000,
        },
        broker,
        allowlist,
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::State { state } = terminal else {
        panic!("allowed generation join returned error");
    };
    assert_eq!(state.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn usage_relay_dispatch_denies_projection_for_surface() {
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
    let allowlist = UsageCapabilitySet::new([capability("allowed")]);

    let denied = dispatch(
        UsageBrokerOperation::CurrentProjectionForSurface,
        broker,
        allowlist,
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied else {
        panic!("for-surface projection returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn empty_capabilities_do_not_start_a_tunnel_child() {
    let temp = tempfile::tempdir().unwrap();
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("data")).client();
    let guard = start_apple_tunnel(
        "fixture",
        PreparedUsageRelay {
            broker,
            capabilities: vec![],
            canonical_launch_usage_capabilities: CanonicalLaunchUsageCapabilities::default(),
            credential_scope: UsageCredentialScope::default(),
        },
    )
    .unwrap();
    assert!(guard.task.is_none());
}

/// S2: the relay dispatch admits exactly the launch allowlist (W1 = A/B/C).
/// Forged IDs, same-surface siblings, and empty scopes are denied before any
/// provider work; every admitted account completes its own generation.
#[tokio::test]
async fn s2_relay_dispatch_admits_exactly_abc() {
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
    let allowlist = UsageCapabilitySet::new([
        capability("acc-a"),
        capability("acc-b"),
        capability("acc-c"),
    ]);

    // Forged D across every operation shape: denied, zero provider calls.
    for operation in [
        UsageBrokerOperation::Current {
            capability: capability("acc-d"),
        },
        UsageBrokerOperation::Refresh {
            capability: capability("acc-d"),
            observed_generation: 0,
            force: true,
        },
        UsageBrokerOperation::Join {
            capability: capability("acc-d"),
            generation: 1,
            timeout_ms: 50,
        },
        UsageBrokerOperation::CurrentForCapability {
            capability: capability("acc-d"),
        },
        UsageBrokerOperation::JoinForCapability {
            capability: capability("acc-d"),
            generation: 1,
            timeout_ms: 50,
        },
    ] {
        let denied = dispatch(
            operation,
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::Error { error } = denied else {
            panic!("forged acc-d returned state");
        };
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    }
    // Same-surface sibling: same provider surface, non-launched account.
    let denied = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("acc-a-evil"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied else {
        panic!("same-surface forgery returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // Empty scope denies even a well-formed, otherwise-known capability.
    let empty = UsageCapabilitySet::new([]);
    let denied = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("acc-a"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        empty,
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied else {
        panic!("empty scope returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // Each admitted account refreshes and completes independently.
    for id in ["acc-a", "acc-b", "acc-c"] {
        let refresh = dispatch(
            UsageBrokerOperation::RefreshForCapability {
                capability: capability(id),
                observed_generation: 0,
                force: true,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::State { state } = refresh else {
            panic!("admitted {id} returned error");
        };
        let terminal = dispatch(
            UsageBrokerOperation::JoinForCapability {
                capability: capability(id),
                generation: state.generation,
                timeout_ms: 2_000,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::State { state } = terminal else {
            panic!("admitted {id} join returned error");
        };
        assert_eq!(
            state.phase,
            UsageRefreshPhase::Completed,
            "{id} did not complete"
        );
        assert_eq!(state.capability.account_id, id);
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}
