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
fn instance_map_preserves_same_account_aliases_and_requires_allowed_authority() {
    let alias = UsageAccountCapability {
        account_id: "configured-account".into(),
        surface_id: "codex".into(),
    };
    let authority = UsageAccountCapability {
        account_id: "canonical-account".into(),
        surface_id: "codex".into(),
    };
    let launch_config = CapsuleConfig {
        instances: vec![
            "first-instance".into(),
            "second-instance".into(),
            "unknown-instance".into(),
        ],
        accounts: BTreeMap::from([
            ("first-instance".into(), "configured-account".into()),
            ("second-instance".into(), "configured-account".into()),
            ("unknown-instance".into(), "unknown-account".into()),
        ]),
        usage_capabilities: BTreeMap::from([
            ("first-instance".into(), alias.clone()),
            ("second-instance".into(), alias.clone()),
            ("unknown-instance".into(), alias),
        ]),
        ..CapsuleConfig::default()
    };
    let canonical = CanonicalLaunchUsageCapabilities {
        by_account_surface: BTreeMap::from([(
            ("configured-account".into(), "codex".into()),
            authority.clone(),
        )]),
    };
    let allowed = BTreeSet::from([authority.clone()]);
    assert_eq!(
        canonical.for_instances(&launch_config, &allowed),
        BTreeMap::from([
            ("first-instance".into(), authority.clone()),
            ("second-instance".into(), authority),
        ])
    );
    assert!(
        canonical
            .for_instances(&launch_config, &BTreeSet::new())
            .is_empty()
    );
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
        None,
        BTreeMap::new(),
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
    assert!(sources.credential_scope.profiles.is_empty());
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
        count_quota: None,
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
        remaining_money: None,
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
        None,
        BTreeMap::new(),
        None,
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_response else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let denied_capability = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            instance_id: "test-instance".to_owned(),
            capability: capability("denied"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
        None,
        BTreeMap::new(),
        None,
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_capability else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let refresh = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed.clone(),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
        Some("test-instance".to_owned()),
    )
    .await;
    let UsageBrokerResponse::State { state } = refresh else {
        panic!("allowed capability returned error");
    };
    let terminal = dispatch(
        UsageBrokerOperation::JoinForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed.clone(),
            generation: state.generation,
            timeout_ms: 2_000,
        },
        broker,
        allowlist,
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
        Some("test-instance".to_owned()),
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
        None,
        BTreeMap::new(),
        None,
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
            instance_capabilities: BTreeMap::new(),
            credential_scope: UsageCredentialScope::default(),
            inventory: None,
            persistence_context: None,
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
            instance_id: "test-instance".to_owned(),
            capability: capability("acc-d"),
        },
        UsageBrokerOperation::JoinForCapability {
            instance_id: "test-instance".to_owned(),
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
            None,
            BTreeMap::new(),
            None,
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
            instance_id: "test-instance".to_owned(),
            capability: capability("acc-a-evil"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
        None,
        BTreeMap::new(),
        None,
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
            instance_id: "test-instance".to_owned(),
            capability: capability("acc-a"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        empty,
        UsageCredentialScope::default(),
        None,
        BTreeMap::new(),
        None,
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
                instance_id: id.to_owned(),
                capability: capability(id),
                observed_generation: 0,
                force: true,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
            None,
            BTreeMap::from([(id.to_owned(), capability(id))]),
            Some(id.to_owned()),
        )
        .await;
        let UsageBrokerResponse::State { state } = refresh else {
            panic!("admitted {id} returned error");
        };
        let terminal = dispatch(
            UsageBrokerOperation::JoinForCapability {
                instance_id: id.to_owned(),
                capability: capability(id),
                generation: state.generation,
                timeout_ms: 2_000,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
            None,
            BTreeMap::from([(id.to_owned(), capability(id))]),
            Some(id.to_owned()),
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

#[tokio::test]
async fn host_relay_rejects_missing_wrong_and_mismatched_instance_before_broker_connect()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let socket = temp.path().join("broker.sock");
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let broker = UsageBrokerClient::at(socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("allowed");
    let sibling = capability("sibling");
    let instances = BTreeMap::from([
        ("test-instance".to_owned(), allowed.clone()),
        ("sibling-instance".to_owned(), sibling.clone()),
    ]);
    let allowlist = UsageCapabilitySet::new([allowed.clone(), sibling.clone()]);
    for (selector, operation_instance, requested) in [
        (None, "test-instance", allowed.clone()),
        (
            Some("unknown-instance"),
            "unknown-instance",
            allowed.clone(),
        ),
        (Some("test-instance"), "sibling-instance", allowed.clone()),
        (Some("test-instance"), "test-instance", sibling.clone()),
    ] {
        for operation in [
            UsageBrokerOperation::CurrentForCapability {
                instance_id: operation_instance.to_owned(),
                capability: requested.clone(),
            },
            UsageBrokerOperation::RefreshForCapability {
                instance_id: operation_instance.to_owned(),
                capability: requested.clone(),
                observed_generation: 0,
                force: true,
            },
            UsageBrokerOperation::JoinForCapability {
                instance_id: operation_instance.to_owned(),
                capability: requested.clone(),
                generation: 1,
                timeout_ms: 50,
            },
        ] {
            let response = dispatch(
                operation,
                broker.clone(),
                allowlist.clone(),
                UsageCredentialScope::default(),
                None,
                instances.clone(),
                selector.map(str::to_owned),
            )
            .await;
            let UsageBrokerResponse::Error { error } = response else {
                panic!("invalid instance selector returned state");
            };
            assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
        }
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "invalid instance selectors must make zero broker connections"
    );
    Ok(())
}

#[tokio::test]
async fn host_relay_rejects_expired_request_before_broker_connect() -> Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("expired");
    let (mut input, relay_reader) = tokio::io::duplex(64 * 1024);
    let (relay_writer, _output_reader) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));

    let request = relay_tunnel_request(
        1,
        unix_epoch_millis_now().saturating_sub(1),
        UsageBrokerOperation::RefreshForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed,
            observed_generation: 0,
            force: true,
        },
    );
    write_relay_frame(&mut input, &request).await?;
    drop(input);

    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(relay_result.is_err(), "EOF should end the host relay");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            broker_listener.accept()
        )
        .await
        .is_err(),
        "expired request must not connect to the broker"
    );
    Ok(())
}

#[tokio::test]
async fn host_relay_owner_eof_cancels_pending_broker_exchange() -> Result<()> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("pending");
    let (mut input, relay_reader) = tokio::io::duplex(64 * 1024);
    let (relay_writer, _output_reader) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));

    let request = relay_tunnel_request(
        1,
        unix_epoch_millis_after(std::time::Duration::from_secs(10)),
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed,
        },
    );
    write_relay_frame(&mut input, &request).await?;
    let (broker_stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept()).await??;
    let mut broker_reader = tokio::io::BufReader::new(broker_stream);
    let mut request_line = String::new();
    broker_reader.read_line(&mut request_line).await?;
    assert!(!request_line.is_empty());

    drop(input);
    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(relay_result.is_err(), "EOF should end the host relay");
    assert!(
        broker_reader
            .get_mut()
            .write_all(b"response\n")
            .await
            .is_err(),
        "owner EOF must close the pending broker exchange"
    );
    Ok(())
}

#[tokio::test]
async fn host_relay_writer_loss_cancels_pending_broker_exchange() -> Result<()> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("writer-loss");
    let (mut input, relay_reader) = tokio::io::duplex(64 * 1024);
    let (relay_writer, output_reader) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));

    let expiry = unix_epoch_millis_after(std::time::Duration::from_secs(10));
    let first_request = relay_tunnel_request(
        1,
        expiry,
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed.clone(),
        },
    );
    write_relay_frame(&mut input, &first_request).await?;
    let (first_broker_stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept()).await??;
    let mut first_broker_reader = tokio::io::BufReader::new(first_broker_stream);
    let mut request_line = String::new();
    first_broker_reader.read_line(&mut request_line).await?;
    assert!(!request_line.is_empty());

    let second_request = relay_tunnel_request(
        2,
        expiry,
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed,
        },
    );
    write_relay_frame(&mut input, &second_request).await?;
    let (second_broker_stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept()).await??;
    let mut second_broker_reader = tokio::io::BufReader::new(second_broker_stream);
    request_line.clear();
    second_broker_reader.read_line(&mut request_line).await?;
    assert!(!request_line.is_empty());

    // Keep the first exchange owned by the host while the second response
    // provokes the relay writer failure.
    drop(output_reader);
    let response = UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "fixture broker unavailable".to_owned(),
        },
    };
    let mut response_bytes = serde_json::to_vec(&response)?;
    response_bytes.push(b'\n');
    second_broker_reader
        .get_mut()
        .write_all(&response_bytes)
        .await?;

    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(
        relay_result.is_err(),
        "writer loss should end the host relay"
    );
    assert!(
        first_broker_reader
            .get_mut()
            .write_all(b"response\n")
            .await
            .is_err(),
        "writer loss must close the pending broker exchange"
    );
    drop(input);
    Ok(())
}

#[tokio::test]
async fn host_relay_cancel_aborts_pending_broker_exchange() -> Result<()> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("cancelled");
    let (mut input, relay_reader) = tokio::io::duplex(64 * 1024);
    let (relay_writer, _output_reader) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));

    let request_id = 1;
    let request = relay_tunnel_request(
        request_id,
        unix_epoch_millis_after(std::time::Duration::from_secs(10)),
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed,
        },
    );
    write_relay_frame(&mut input, &request).await?;
    let (broker_stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept()).await??;
    let mut broker_reader = tokio::io::BufReader::new(broker_stream);
    let mut request_line = String::new();
    broker_reader.read_line(&mut request_line).await?;
    assert!(!request_line.is_empty());

    write_relay_message(&mut input, &UsageRelayTunnelMessage::Cancel { request_id }).await?;
    tokio::task::yield_now().await;
    assert!(
        broker_reader
            .get_mut()
            .write_all(b"response\n")
            .await
            .is_err(),
        "cancel must close the pending broker exchange"
    );

    drop(input);
    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(relay_result.is_err(), "EOF should end the host relay");
    Ok(())
}

#[tokio::test]
async fn host_relay_rejects_duplicate_live_request_ids_without_replacing_owner() -> Result<()> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("duplicate");
    let (mut input, relay_reader) = tokio::io::duplex(64 * 1024);
    let (relay_writer, _output_reader) = tokio::io::duplex(64 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));

    let request_id = 1;
    let expiry = unix_epoch_millis_after(std::time::Duration::from_secs(10));
    let request = relay_tunnel_request(
        request_id,
        expiry,
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "test-instance".to_owned(),
            capability: allowed.clone(),
        },
    );
    write_relay_frame(&mut input, &request).await?;
    let (broker_stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept()).await??;
    let mut broker_reader = tokio::io::BufReader::new(broker_stream);
    let mut request_line = String::new();
    broker_reader.read_line(&mut request_line).await?;
    assert!(!request_line.is_empty());

    write_relay_frame(&mut input, &request).await?;
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            broker_listener.accept()
        )
        .await
        .is_err(),
        "duplicate live request id must not spawn a replacement broker exchange"
    );

    write_relay_message(&mut input, &UsageRelayTunnelMessage::Cancel { request_id }).await?;
    tokio::task::yield_now().await;
    assert!(
        broker_reader
            .get_mut()
            .write_all(b"response\n")
            .await
            .is_err(),
        "cancel must still target the original request owner"
    );

    drop(input);
    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(relay_result.is_err(), "EOF should end the host relay");
    Ok(())
}

#[tokio::test]
async fn host_relay_try_admission_refuses_requests_above_capacity() -> Result<()> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

    let temp = tempfile::tempdir()?;
    let broker_socket = temp.path().join("broker.sock");
    let broker_listener = tokio::net::UnixListener::bind(&broker_socket)?;
    let broker = UsageBrokerClient::at(broker_socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("saturated");
    let (mut input, relay_reader) = tokio::io::duplex(256 * 1024);
    let (relay_writer, output_reader) = tokio::io::duplex(256 * 1024);
    let relay = tokio::spawn(serve_stdio_tunnel(
        relay_reader,
        relay_writer,
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        BTreeMap::from([("test-instance".to_owned(), allowed.clone())]),
    ));
    let expiry = unix_epoch_millis_after(std::time::Duration::from_secs(10));
    for request_id in 1..=u64::try_from(TUNNEL_REQUEST_CAPACITY).unwrap() + 1 {
        let request = relay_tunnel_request(
            request_id,
            expiry,
            UsageBrokerOperation::CurrentForCapability {
                instance_id: "test-instance".to_owned(),
                capability: allowed.clone(),
            },
        );
        write_relay_frame(&mut input, &request).await?;
    }

    let mut broker_connections = Vec::with_capacity(TUNNEL_REQUEST_CAPACITY);
    for _ in 0..TUNNEL_REQUEST_CAPACITY {
        let (stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(1), broker_listener.accept())
                .await??;
        broker_connections.push(stream);
    }

    let mut output = tokio::io::BufReader::new(output_reader);
    let response: UsageRelayTunnelResponse = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        read_async_frame(&mut output),
    )
    .await??;
    assert_eq!(
        response.request_id,
        u64::try_from(TUNNEL_REQUEST_CAPACITY).unwrap() + 1
    );
    let UsageBrokerResponse::Error { error } = response.response else {
        panic!("saturated admission returned a non-error response");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);

    drop(input);
    drop(broker_connections);
    let relay_result = tokio::time::timeout(std::time::Duration::from_secs(1), relay).await??;
    assert!(relay_result.is_err(), "EOF should end the host relay");
    Ok(())
}

#[tokio::test]
async fn host_relay_pinned_frame_read_preserves_partial_bytes() -> Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let (mut writer, reader) = tokio::io::duplex(64 * 1024);
    let mut reader = tokio::io::BufReader::new(reader);
    writer.write_all(b"12").await?;
    let mut frame = Box::pin(read_async_frame_with_deadline::<_, u64>(&mut reader));

    let interrupted = tokio::select! {
        biased;
        _ = tokio::time::sleep(std::time::Duration::from_millis(1)) => true,
        result = &mut frame => {
            result?;
            false
        }
    };
    assert!(interrupted, "partial frame unexpectedly completed");

    writer.write_all(b"3\n").await?;
    let (value, _) = frame.await?;
    assert_eq!(value, 123);
    Ok(())
}

fn relay_tunnel_request(
    request_id: u64,
    expires_at_unix_ms: u64,
    operation: UsageBrokerOperation,
) -> UsageRelayTunnelRequest {
    UsageRelayTunnelRequest {
        instance_id: Some("test-instance".to_owned()),
        expires_at_unix_ms,
        request_id,
        request: UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            operation,
            launch_credential_scope: None,
        },
    }
}

async fn write_relay_frame<W>(writer: &mut W, value: &UsageRelayTunnelRequest) -> Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    write_relay_message(
        writer,
        &UsageRelayTunnelMessage::Request {
            request: Box::new(value.clone()),
        },
    )
    .await
}

async fn write_relay_message<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    Ok(())
}

fn unix_epoch_millis_now() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

fn unix_epoch_millis_after(duration: std::time::Duration) -> u64 {
    unix_epoch_millis_now().saturating_add(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}

#[tokio::test(start_paused = true)]
async fn admitted_host_request_first_polled_after_expiry_never_connects() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let socket = temp.path().join("expired-dispatch.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let broker = UsageBrokerClient::at(socket, env!("CARGO_PKG_VERSION").to_owned());
    let allowed = capability("expired-admitted");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    let call = dispatch_with_admission(
        UsageBrokerOperation::RefreshForCapability {
            capability: allowed.clone(),
            instance_id: "admitted".to_owned(),
            observed_generation: 0,
            force: true,
        },
        broker,
        UsageCapabilitySet::new([allowed.clone()]),
        UsageCredentialScope::default(),
        None,
        None,
        Some(deadline),
        BTreeMap::from([("admitted".to_owned(), allowed)]),
        Some("admitted".to_owned()),
    );
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    let UsageBrokerResponse::Error { error } = call.await else {
        panic!("expired dispatch succeeded");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "expired admitted work must not connect or dispatch provider work"
    );
    Ok(())
}

fn prepared_profile_scope_fixture() -> (
    AppConfig,
    Vec<jackin_config::ResolvedInstance>,
    BTreeMap<String, crate::instance::ProvisionedInstanceAuth>,
) {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, AuthForwardMode};
    use jackin_core::{Agent, ProfileCredentialSourceIdentity, ProfileCredentialSourceMaterial};
    let mut config = AppConfig::default();
    config.accounts.insert(
        "selected".to_owned(),
        AccountConfig {
            enabled: true,
            name: "selected".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::Profile {
                agent: Agent::Hermes,
                directory: PathBuf::from("/absent-profile-source"),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "selected-hermes".to_owned(),
        agent: Agent::Hermes,
        account_id: "selected".to_owned(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "selected".to_owned(),
        synthesized: false,
    }];
    let slots = BTreeMap::from([(
        "selected-hermes".to_owned(),
        crate::instance::ProvisionedInstanceAuth {
            agent: Agent::Hermes,
            account_id: "selected".to_owned(),
            mode: AuthForwardMode::Sync,
            home_dir: None,
            credential_paths: Vec::new(),
            forward_auth: true,
            profile_material: Some(ProfileCredentialSourceMaterial {
                source: ProfileCredentialSourceIdentity {
                    agent: Agent::Hermes,
                    descriptor_fingerprint: "a".repeat(64),
                },
                material_revision: "b".repeat(64),
            }),
            slot_suffix: None,
            container_home_rel: "hermes".to_owned(),
            container_store_rel: "hermes".to_owned(),
            folder_target: "/home/agent/hermes".to_owned(),
            cache_source_dir: None,
            container_cache_rel: None,
        },
    )]);
    (config, instances, slots)
}

#[test]
fn prepared_profile_scope_uses_exact_instance_account_and_provider() -> Result<()> {
    let (config, instances, mut slots) = prepared_profile_scope_fixture();
    let selected = slots["selected-hermes"]
        .profile_material
        .clone()
        .expect("fixture material");
    let mut unrelated = slots["selected-hermes"].clone();
    unrelated.account_id = "unselected".to_owned();
    slots.insert("unselected-hermes".to_owned(), unrelated);
    let profiles = usage_profile_scope_for_slots(&config, &instances, &slots)?;
    assert_eq!(
        profiles,
        BTreeSet::from([UsageProfileSourceProof {
            instance_id: "selected-hermes".to_owned(),
            account_id: "selected".to_owned(),
            surface_id: "zai".to_owned(),
            source: selected.source,
            material_revision: selected.material_revision,
        }])
    );
    Ok(())
}

#[test]
fn prepared_profile_scope_rejects_missing_or_mismatched_slot() {
    let (config, instances, slots) = prepared_profile_scope_fixture();
    assert!(usage_profile_scope_for_slots(&config, &instances, &BTreeMap::new()).is_err());
    for mismatch in ["account", "agent", "key"] {
        let mut slots = slots.clone();
        match mismatch {
            "account" => slots.get_mut("selected-hermes").unwrap().account_id = "other".to_owned(),
            "agent" => slots.get_mut("selected-hermes").unwrap().agent = jackin_core::Agent::Codex,
            "key" => {
                let slot = slots.remove("selected-hermes").unwrap();
                slots.insert("other-instance".to_owned(), slot);
            }
            _ => unreachable!(),
        }
        assert!(
            usage_profile_scope_for_slots(&config, &instances, &slots).is_err(),
            "{mismatch}"
        );
    }
}

#[test]
fn prepared_profile_scope_rejects_missing_or_invalid_forwarded_material() {
    let (config, instances, slots) = prepared_profile_scope_fixture();
    for mismatch in ["missing", "agent", "descriptor", "revision"] {
        let mut slots = slots.clone();
        let slot = slots.get_mut("selected-hermes").unwrap();
        match mismatch {
            "missing" => slot.profile_material = None,
            "agent" => {
                slot.profile_material.as_mut().unwrap().source.agent = jackin_core::Agent::Codex
            }
            "descriptor" => {
                slot.profile_material
                    .as_mut()
                    .unwrap()
                    .source
                    .descriptor_fingerprint = "A".repeat(64)
            }
            "revision" => slot
                .profile_material
                .as_mut()
                .unwrap()
                .material_revision
                .clear(),
            _ => unreachable!(),
        }
        assert!(
            usage_profile_scope_for_slots(&config, &instances, &slots).is_err(),
            "{mismatch}"
        );
    }
}

#[test]
fn prepared_profile_scope_grants_nothing_without_forwarding() -> Result<()> {
    let (config, instances, slots) = prepared_profile_scope_fixture();
    for ignore in [false, true] {
        let mut slots = slots.clone();
        let slot = slots.get_mut("selected-hermes").unwrap();
        slot.profile_material = None;
        if ignore {
            slot.forward_auth = false;
            slot.mode = jackin_config::AuthForwardMode::Ignore;
        } else {
            slot.forward_auth = false;
        }
        assert!(usage_profile_scope_for_slots(&config, &instances, &slots)?.is_empty());
    }
    Ok(())
}

#[test]
fn prepared_profile_scope_never_grants_antigravity_presence() -> Result<()> {
    let (mut config, mut instances, mut slots) = prepared_profile_scope_fixture();
    let account = config.accounts.get_mut("selected").unwrap();
    account.provider = jackin_config::AiProvider::Google;
    let jackin_config::AccountCredential::Profile { agent, .. } = &mut account.credential else {
        unreachable!()
    };
    *agent = jackin_core::Agent::Antigravity;
    instances[0].agent = jackin_core::Agent::Antigravity;
    let slot = slots.get_mut("selected-hermes").unwrap();
    slot.agent = jackin_core::Agent::Antigravity;
    slot.profile_material = None;
    assert!(usage_profile_scope_for_slots(&config, &instances, &slots)?.is_empty());
    slot.account_id = "other".to_owned();
    assert!(usage_profile_scope_for_slots(&config, &instances, &slots).is_err());
    Ok(())
}

#[test]
fn prepared_profile_scope_failure_preserves_existing_scope_transactionally() -> Result<()> {
    let (config, mut instances, mut slots) = prepared_profile_scope_fixture();
    let existing_material = slots["selected-hermes"]
        .profile_material
        .clone()
        .expect("fixture material");
    let mut invalid_instance = instances[0].clone();
    invalid_instance.config_id = "invalid-hermes".to_owned();
    instances.push(invalid_instance);
    let mut invalid_slot = slots["selected-hermes"].clone();
    invalid_slot.profile_material = None;
    slots.insert("invalid-hermes".to_owned(), invalid_slot);
    let state = crate::instance::RoleState {
        root: PathBuf::from("/unused-private-fixture-state"),
        gh_config_dir: PathBuf::from("/unused-private-fixture-gh"),
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Hermes,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth { slots },
        auth_outcomes: BTreeMap::new(),
        auth_mount_paths: BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    };
    let mut scope = UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            instance_id: "existing-env-instance".to_owned(),
            account_id: "existing-env-account".to_owned(),
            surface_id: "codex".to_owned(),
            key: "OPENAI_API_KEY".to_owned(),
            source: UsageCredentialSourceIdentity::Literal,
            material_fingerprint: usage_credential_material_fingerprint("fixture-existing-env"),
        }]),
        profiles: BTreeSet::from([UsageProfileSourceProof {
            instance_id: "existing-profile-instance".to_owned(),
            account_id: "existing-profile-account".to_owned(),
            surface_id: "zai".to_owned(),
            source: existing_material.source,
            material_revision: existing_material.material_revision,
        }]),
    };
    let before = scope.clone();
    let before_bytes = serde_json::to_vec(&scope)?;
    // First admitted instance is valid; second fails after its proof would
    // have been added by an incremental, nontransactional implementation.
    let error =
        merge_usage_profile_scope_for_prepared_launch(&config, &instances, &state, &mut scope)
            .expect_err("second admitted instance has no captured profile material");
    assert!(error.to_string().contains("invalid-hermes"));
    assert_eq!(scope, before);
    assert_eq!(serde_json::to_vec(&scope)?, before_bytes);
    Ok(())
}

#[test]
fn public_membership_admission_requires_the_live_issuer_after_catalog_rotation() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
    use jackin_instance::manifest::{DockerResources, InstanceManifest, NewInstanceManifest};
    use jackin_protocol::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    use jackin_protocol::usage_broker::UsageCatalogEntry;

    fn bind_inventory(
        paths: &JackinPaths,
        container: &ContainerHandle,
        inventory: &RelayUsageInventory,
    ) -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        let state_dir = paths.data_dir.join(container.name());
        fs::create_dir_all(&state_dir)?;
        fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))?;
        let manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: container.name(),
            workspace_name: Some("mine"),
            workspace_label: "Mine",
            workdir: "/workspace",
            host_workdir_fingerprint: "fixture",
            role_key: "role",
            role_display_name: "Role",
            agent_runtime: jackin_core::Agent::Codex,
            role_source_git: "fixture",
            role_source_ref: None,
            image_tag: "fixture",
            docker: DockerResources::from_container_name(container.name()),
            role_git_sha: None,
            base_image_ref: None,
            base_image_digest: None,
            supported_agents: vec![jackin_core::Agent::Codex],
        });
        manifest.write(&state_dir)?;
        persistence::save(
            paths,
            container.name(),
            container.id(),
            Some("mine"),
            "role",
            inventory.config_generation(),
            &[],
            &UsageCredentialScope::default(),
            &BTreeMap::new(),
            &inventory.authority(),
            inventory.unresolved_grants(),
        )
    }

    let root = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(root.path());
    fs::create_dir_all(&paths.config_dir)?;
    fs::create_dir_all(&paths.home_dir)?;
    let profile = root.path().join("profile-personal");
    fs::create_dir_all(&profile)?;
    fs::write(
        profile.join("auth.json"),
        r#"{"tokens":{"access_token":"fixture-personal","account_id":"provider-personal"}}"#,
    )?;
    let mut config = AppConfig::default();
    config.accounts.insert(
        "personal".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Personal".to_owned(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: jackin_core::Agent::Codex,
                directory: profile,
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    config.workspaces.insert(
        "mine".to_owned(),
        WorkspaceConfig {
            accounts: vec!["personal".to_owned()],
            workdir: root.path().to_string_lossy().into_owned(),
            ..WorkspaceConfig::default()
        },
    );
    fs::write(&paths.config_file, toml::to_string(&config)?)?;
    let original_config = fs::read(&paths.config_file)?;
    let resolver = CachedProviderCredentialResolver::new(RuntimeSecretSource);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: paths.config_dir.clone(),
            operator_home: paths.home_dir.clone(),
        },
        &resolver,
    )
    .map_err(anyhow::Error::msg)?;
    let discovery = validate_usage_sources(catalog, &resolver);
    let inventory =
        RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
    let authorities = inventory.authority();
    assert_eq!(authorities.len(), 1);
    let authority = &authorities[0];
    let container = ContainerHandle::new("jackin-membership-issuer", "immutable-fixture-id")?;
    bind_inventory(&paths, &container, &inventory)?;
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(paths.data_dir.clone()),
        executor.clone(),
    )
    .map_err(|error| anyhow::anyhow!("broker fixture failed: {}", error.message))?;
    let entry = UsageCatalogEntry {
        capability: authority.capability.clone(),
        canonical_identity: Some(UsageCanonicalAccountIdentity {
            surface_id: "codex".to_owned(),
            subject: UsageCanonicalAccountSubject::ProviderId("provider-personal".to_owned()),
        }),
        provenance_count: authority.provenance_count,
        revision: "source-v1".to_owned(),
    };
    let lease = broker
        .current_projection()
        .map_err(|error| anyhow::anyhow!("{}", error.message))?
        .projection_id;
    let issued = broker
        .reconcile_catalog_if_projection(
            Some(lease),
            "same-discovery-revision".to_owned(),
            vec![entry.clone()],
        )
        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
    let guest = inventory.filter_projection(issued.clone());
    let mut exhausted = guest.clone();
    exhausted.broker_generation = u64::MAX;
    assert!(
        jackin_protocol::control::UsageAccountMembershipV1::validate_current_projection(&exhausted)
            .is_err(),
        "an exhausted publication ordinal cannot authorize current membership"
    );
    assert_eq!(
        validate_usage_inventory_projection(&paths, &container, &guest)?,
        inventory.config_generation()
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // The identical host scope and saved proof cannot admit a publication when
    // its issuer is absent. A new data directory intentionally has no broker.
    let mut without_broker = paths.clone();
    without_broker.data_dir = root.path().join("missing-issuer-data");
    bind_inventory(&without_broker, &container, &inventory)?;
    let unavailable =
        validate_usage_inventory_projection(&without_broker, &container, &guest).unwrap_err();
    assert!(
        unavailable.to_string().contains("issuer unavailable"),
        "{unavailable:#}"
    );
    assert!(
        !without_broker.data_dir.join("usage-broker").exists(),
        "validation must not activate a broker"
    );

    let mut replacement = entry;
    replacement.revision = "source-v2".to_owned();
    let rotated = broker
        .reconcile_catalog_if_projection(
            Some(issued.projection_id.clone()),
            issued.discovery_revision.clone(),
            vec![replacement],
        )
        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
    assert_eq!(rotated.discovery_revision, issued.discovery_revision);
    assert_ne!(rotated.projection_id, issued.projection_id);
    assert_eq!(fs::read(&paths.config_file)?, original_config);
    let stale = validate_usage_inventory_projection(&paths, &container, &guest).unwrap_err();
    assert!(stale.to_string().contains("no longer current"), "{stale:#}");
    let fresh_guest = inventory.filter_projection(rotated.clone());
    assert_eq!(
        validate_usage_inventory_projection(&paths, &container, &fresh_guest)?,
        inventory.config_generation()
    );

    let retired = broker
        .reconcile_catalog_if_projection(
            Some(rotated.projection_id),
            rotated.discovery_revision,
            vec![],
        )
        .map_err(|error| anyhow::anyhow!("{}", error.message))?;
    assert_eq!(retired.discovery_revision, issued.discovery_revision);
    let stale = validate_usage_inventory_projection(&paths, &container, &fresh_guest).unwrap_err();
    assert!(stale.to_string().contains("no longer current"), "{stale:#}");
    assert_eq!(fs::read(&paths.config_file)?, original_config);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "membership admission must never refresh providers"
    );
    Ok(())
}
