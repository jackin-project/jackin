use std::sync::Mutex;

use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus};

use super::*;

mod identity_admission;
mod source_cache_authority;
mod typed_membership;

type ResolverCall = (Option<String>, Option<String>, Vec<String>);

#[derive(Default)]
struct FakeEnvResolver {
    calls: Mutex<Vec<ResolverCall>>,
}

struct NoEnvResolver;

impl ProviderCredentialEnvResolver for NoEnvResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

struct ConsentKeychainReader;

impl ProfileCredentialReader for ConsentKeychainReader {
    fn read(&self, _path: &Path) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }

    fn exists(&self, _path: &Path) -> bool {
        false
    }

    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        ProfileReadOutcome::ConsentRequired
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        ProfileReadOutcome::ConsentRequired
    }
}

#[derive(Default)]
struct RecordingProfileReader {
    reads: Mutex<BTreeMap<PathBuf, usize>>,
}

impl ProfileCredentialReader for RecordingProfileReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        *self
            .reads
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_default() += 1;
        match std::fs::read(path) {
            Ok(bytes) => ProfileReadOutcome::Bytes(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ProfileReadOutcome::Missing
            }
            Err(_) => ProfileReadOutcome::Denied,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        panic!("Claude is ignored in source-validation fixtures")
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }
}

struct SyntheticDatabaseOnlyReader;

impl ProfileCredentialReader for SyntheticDatabaseOnlyReader {
    fn read(&self, _path: &Path) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }

    fn exists(&self, path: &Path) -> bool {
        path.file_name().and_then(std::ffi::OsStr::to_str) == Some("opencode.db")
    }

    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        panic!("Claude is ignored in source-validation fixtures")
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }
}

impl ProviderCredentialEnvResolver for FakeEnvResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        self.calls.lock().unwrap().push((
            workspace.map(|workspace| workspace.as_str().to_owned()),
            role.map(str::to_owned),
            keys.iter().map(|key| key.name.to_owned()).collect(),
        ));
        keys.iter()
            .filter_map(|entry| match entry.owner {
                UsageCredentialOwner::Zai => Some(ProviderCredentialEnvResolution {
                    key: entry.name.to_owned(),
                    outcome: ProviderCredentialEnvOutcome::Resolved(OpaqueCredentialHandle::new(
                        "zai-shared",
                    )),
                }),
                UsageCredentialOwner::Minimax if workspace.is_some() => {
                    Some(ProviderCredentialEnvResolution {
                        key: entry.name.to_owned(),
                        outcome: ProviderCredentialEnvOutcome::Resolved(
                            OpaqueCredentialHandle::new("minimax-workspace-shared"),
                        ),
                    })
                }
                UsageCredentialOwner::OpenRouter => Some(ProviderCredentialEnvResolution {
                    key: entry.name.to_owned(),
                    outcome: ProviderCredentialEnvOutcome::Resolved(OpaqueCredentialHandle::new(
                        "openrouter-shared",
                    )),
                }),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn opencode_profile_requires_one_auth_entry_and_ignores_sibling_database() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("opencode");
    std::fs::create_dir_all(&root).unwrap();
    let auth = root.join("auth.json");
    let reader = RecordingProfileReader::default();

    std::fs::write(
        &auth,
        r#"{"anthropic":{"type":"api","key":"fixture-a"},"opencode-go":{"type":"api","key":"fixture-go"}}"#,
    )
    .unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Malformed
    ));

    std::fs::write(
        &auth,
        r#"{"opencode-go":{"type":"api","key":"fixture-go"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("opencode.db"), b"database fixture").unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Anonymous(Some(_))
    ));

    std::fs::remove_file(&auth).unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Malformed
    ));
}

#[test]
fn opencode_profile_database_only_uses_reader_abstraction() {
    let reader = SyntheticDatabaseOnlyReader;
    let auth = Path::new("/synthetic/opencode/auth.json");

    assert!(matches!(
        opencode_profile_identity(&reader, auth),
        ProfileValidation::Malformed
    ));
}

#[test]
fn disc_claude_keychain_consent_is_not_reported_missing() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = home.join(".claude");
    let catalog = UsageDiscoveryCatalog {
        config_generation: None,
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        sources: vec![DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Claude,
            agent: Agent::Claude,
            provider: "anthropic".to_owned(),
            selector: None,
            root,
            operator_home: home,
            account_label: Some("work".to_owned()),
            source_id: "source-0001".to_owned(),
            capability_id: "capability-1".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
            configured_account_ids: BTreeSet::from(["work".to_owned()]),
        }],
    };

    let validated =
        validate_usage_sources_with_reader(catalog, &NoEnvResolver, &ConsentKeychainReader);

    assert!(validated.accounts.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert_eq!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::KeychainConsentRequired
    );
    assert_eq!(
        validated.diagnostics[0].issue.id(),
        "keychain_consent_required"
    );
}

struct UnavailableKeychainReader;

impl ProfileCredentialReader for UnavailableKeychainReader {
    fn read(&self, _path: &Path) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }
    fn exists(&self, _path: &Path) -> bool {
        false
    }
    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        ProfileReadOutcome::Unavailable
    }
    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        ProfileReadOutcome::Unavailable
    }
}

#[test]
fn disc_claude_keychain_unavailable_is_not_absence_denial_or_consent() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let catalog = UsageDiscoveryCatalog {
        config_generation: None,
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        sources: vec![DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Claude,
            agent: Agent::Claude,
            provider: "anthropic".to_owned(),
            selector: None,
            root: home.join(".claude"),
            operator_home: home,
            account_label: Some("work".to_owned()),
            source_id: "source-unavailable".to_owned(),
            capability_id: "capability-unavailable".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
            configured_account_ids: BTreeSet::from(["work".to_owned()]),
        }],
    };
    let validated =
        validate_usage_sources_with_reader(catalog, &NoEnvResolver, &UnavailableKeychainReader);
    assert!(validated.accounts.is_empty());
    assert!(validated.bindings.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert_eq!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::CredentialUnavailable
    );
    assert_eq!(
        validated.diagnostics[0].issue.id(),
        "credential_unavailable"
    );
    assert_eq!(
        validated.diagnostics[0].issue.display_message(),
        "Credential access is temporarily unavailable"
    );
}

fn write_registry(config_root: &Path, entries: &[(&str, Agent, &Path)]) {
    let mut config = AppConfig::default();
    for (id, agent, directory) in entries {
        config.accounts.insert(
            (*id).to_owned(),
            jackin_config::AccountConfig {
                enabled: true,
                name: (*id).to_owned(),
                provider: AiProvider::for_agent(*agent)
                    .expect("registry fixtures use native-provider agents"),
                credential: AccountCredential::Profile {
                    agent: *agent,
                    directory: directory.to_path_buf(),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    std::fs::create_dir_all(config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
}

fn write_kimi_auth_route(directory: &Path, base_url: &str, oauth_host: &str) -> PathBuf {
    use sha2::Digest as _;

    let hash_input = format!(
        "{{\"oauthHost\":{},\"baseUrl\":{}}}",
        serde_json::to_string(oauth_host).unwrap(),
        serde_json::to_string(base_url).unwrap()
    );
    let digest = sha2::Sha256::digest(hash_input.as_bytes());
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let slot_key = format!("oauth/kimi-code-env-{}", &hash[..16]);
    let credential = directory.join(format!("credentials/kimi-code-env-{}.json", &hash[..16]));
    let config = format!(
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\nbase_url = \"{base_url}\"\n\
[providers.\"managed:kimi-code\".oauth]\nkey = \"{slot_key}\"\nstorage = \"file\"\noauth_host = \"{oauth_host}\"\n\
[models.\"kimi-k2\"]\nmodel = \"kimi-k2\"\nmax_context_size = 131072\n"
    );
    std::fs::create_dir_all(directory).unwrap();
    std::fs::create_dir_all(directory.join("credentials")).unwrap();
    std::fs::write(directory.join("config.toml"), config).unwrap();
    credential
}

#[test]
fn env_capability_ids_isolate_distinct_opaque_credentials() {
    let first = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-1"),
        key: "ZAI_API_KEY".to_owned(),
        dispatch_key: "ZAI_API_KEY".to_owned(),
    };
    let second = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-2"),
        key: "ZAI_API_KEY".to_owned(),
        dispatch_key: "ZAI_API_KEY".to_owned(),
    };

    let first_id = source_capability_id(HostSurfaceId::Zai, &first);
    let second_id = source_capability_id(HostSurfaceId::Zai, &second);
    assert_ne!(first_id, second_id);
    assert_eq!(first_id, source_capability_id(HostSurfaceId::Zai, &first));
    assert!(!first_id.contains("credential-1"));
    assert!(!second_id.contains("credential-2"));
}

#[test]
fn disc_registry_enumerates_registered_sources_without_ambient_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let home = temp.path().join("home");
    write_registry(
        &config_root,
        &[
            ("work", Agent::Codex, Path::new("/profiles/codex-work")),
            (
                "personal",
                Agent::Codex,
                Path::new("/profiles/codex-personal"),
            ),
        ],
    );
    write_codex_auth(
        &home.join(".codex"),
        "ambient",
        "e30",
        "unregistered-secret",
    );
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: home.clone(),
        },
        &resolver,
    )
    .unwrap();
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 2);
    assert!(
        catalog
            .candidates
            .iter()
            .all(|candidate| candidate.surface_id == "codex")
    );
    assert!(resolver.calls.lock().unwrap().is_empty());
    write_registry(&config_root, &[]);
    let empty = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &resolver,
    )
    .unwrap();
    assert!(empty.candidates.is_empty());
}

#[test]
fn disc_registry_api_sources_are_isolated_from_ambient_env_declarations() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&config_root).unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai-work".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-key".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    config
        .account_bindings
        .insert(Agent::Opencode, "zai-work".to_owned());
    config.env.insert(
        "MINIMAX_API_KEY".to_owned(),
        jackin_config::EnvValue::Plain("unregistered".to_owned()),
    );
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &resolver,
    )
    .unwrap();
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "zai");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(calls.len(), 5, "all compatible Z.AI routes must resolve");
    let mut isolated_alias_counts = BTreeMap::<String, usize>::new();
    for (_, _, keys) in calls.iter() {
        assert_eq!(keys.len(), 1, "each route must resolve one isolated alias");
        assert!(!jackin_core::is_account_env(&keys[0]));
        *isolated_alias_counts.entry(keys[0].clone()).or_default() += 1;
    }
    assert_eq!(
        isolated_alias_counts,
        BTreeMap::from([
            ("JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN".to_owned(), 1,),
            ("JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY".to_owned(), 1),
            ("JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY".to_owned(), 3),
        ])
    );
    assert!(
        !calls
            .iter()
            .any(|(_, _, keys)| { keys.iter().any(|key| key == "MINIMAX_API_KEY") })
    );

    let launch_keys = catalog
        .sources
        .iter()
        .find_map(|source| match source {
            DiscoveredCredentialSource::Env { launch_keys, .. } => Some(launch_keys.clone()),
            DiscoveredCredentialSource::Profile { .. }
            | DiscoveredCredentialSource::Capability { .. } => None,
        })
        .expect("one canonical provider source");
    assert_eq!(
        launch_keys,
        BTreeSet::from([
            "ANTHROPIC_AUTH_TOKEN".to_owned(),
            "OPENAI_API_KEY".to_owned(),
            "ZHIPU_API_KEY".to_owned(),
        ])
    );
    assert!(!format!("{catalog:?}").contains("fixture-key"));
}

#[test]
fn disc_synthesized_routes_keep_launch_keys_separate() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai-routes".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Z.AI routes".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-key".to_owned()),
                base_url: None,
                model: Some("glm-5".to_owned()),
            },
        },
    );
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    let (canonical_key, dispatch_key, launch_keys) = catalog
        .sources
        .iter()
        .find_map(|source| match source {
            DiscoveredCredentialSource::Env {
                key,
                dispatch_key,
                launch_keys,
                ..
            } => Some((key.as_str(), dispatch_key.as_str(), launch_keys.clone())),
            DiscoveredCredentialSource::Profile { .. }
            | DiscoveredCredentialSource::Capability { .. } => None,
        })
        .expect("one canonical provider source");
    assert_eq!(canonical_key, "ZAI_API_KEY");
    assert_eq!(dispatch_key, "ZAI_API_KEY");
    assert_eq!(
        launch_keys,
        BTreeSet::from([
            "ANTHROPIC_AUTH_TOKEN".to_owned(),
            "OPENAI_API_KEY".to_owned(),
            "ZHIPU_API_KEY".to_owned(),
        ])
    );
    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.bindings.len(), 1);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
}

#[test]
fn disc_registry_openrouter_api_key_maps_to_usage_surface_and_governed_env() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&config_root).unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "openrouter-work".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "OpenRouter work".to_owned(),
            provider: AiProvider::OpenRouter,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-openrouter-key".to_owned()),
                base_url: None,
                model: Some("openai/gpt-5".to_owned()),
            },
        },
    );
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &resolver,
    )
    .unwrap();

    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "openrouter");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    assert_eq!(
        resolver.calls.lock().unwrap()[0].2,
        vec!["JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY"]
    );
    assert!(!jackin_core::is_account_env(
        &resolver.calls.lock().unwrap()[0].2[0]
    ));
    assert!(!format!("{catalog:?}").contains("fixture-openrouter-key"));

    let validated = validate_usage_sources(catalog, &resolver);
    let capabilities = crate::host::usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].surface_id, "openrouter");
}

#[test]
fn disc_scope_capsule_uses_only_forwarded_capabilities() {
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "opencode".to_owned(),
                    capability_id: "cap-2".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "bogus".to_owned(),
                    capability_id: "skipped".to_owned(),
                    account_label: None,
                },
            ],
        },
        &resolver,
    )
    .unwrap();

    // Duplicate claude capabilities dedup; every known surface (not just the
    // Desktop glance order) is admitted; unknown ids still skip.
    assert_eq!(catalog.candidates.len(), 2);
    assert_eq!(catalog.candidates[0].surface_id, "claude");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ForwardedCapability
    );
    assert_eq!(catalog.candidates[1].surface_id, "opencode");
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_scope_capsule_admits_newly_wired_surfaces() {
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "cursor".to_owned(),
                    capability_id: "cap-cursor".to_owned(),
                    account_label: Some("cursor@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "google".to_owned(),
                    capability_id: "cap-google".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "openrouter".to_owned(),
                    capability_id: "cap-openrouter".to_owned(),
                    account_label: None,
                },
                // No collector, registry, or discovery entry exists.
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "copilot".to_owned(),
                    capability_id: "skipped".to_owned(),
                    account_label: None,
                },
            ],
        },
        &resolver,
    )
    .unwrap();

    let mut ids: Vec<_> = catalog
        .candidates
        .iter()
        .map(|candidate| candidate.surface_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["cursor", "google", "openrouter"]);
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_same_provider_sources_with_same_labels_keep_source_capabilities_distinct() {
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    canonical_identity: Some(
                        CanonicalAccountIdentity {
                            surface: HostSurfaceId::Codex,
                            subject: CanonicalAccountSubject::SourceCapability(
                                "authenticated-source-a".to_owned(),
                            ),
                        }
                        .protocol_identity(),
                    ),
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: Some("same@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    canonical_identity: Some(
                        CanonicalAccountIdentity {
                            surface: HostSurfaceId::Codex,
                            subject: CanonicalAccountSubject::SourceCapability(
                                "authenticated-source-b".to_owned(),
                            ),
                        }
                        .protocol_identity(),
                    ),
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-b".to_owned(),
                    account_label: Some("same@example.test".to_owned()),
                },
            ],
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources(catalog, &NoEnvResolver);

    assert_eq!(validated.accounts.len(), 2);
    assert_eq!(validated.bindings.len(), 2);
    assert_eq!(
        validated
            .accounts
            .iter()
            .map(|account| account.account_key.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert!(validated.accounts.iter().all(|account| {
        matches!(
            account.identity.subject,
            CanonicalAccountSubject::SourceCapability(_)
        )
    }));
    assert!(
        validated
            .accounts
            .iter()
            .all(|account| account.source_ids.len() == 1)
    );
}

#[test]
fn disc_unresolved_authoritative_labels_do_not_mint_authenticated_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    canonical_identity: None,
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-b".to_owned(),
                    account_label: None,
                },
            ],
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    let bindings = validated.bindings.clone();

    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(validated);

    for (index, binding) in bindings.iter().enumerate() {
        let mut view = FocusedUsageView::unavailable("fixture", index as i64);
        view.focused_agent = Some("codex".to_owned());
        view.focused_provider = Some("OpenAI".to_owned());
        view.account.provider_label = "OpenAI / Codex".to_owned();
        view.account.account_label = "same@example.test".to_owned();
        view.confidence = UsageConfidence::Authoritative;
        view.status_bar_label = format!("source-{index}");
        runtime.record_discovered_snapshot(binding, view);
    }

    assert!(runtime.discovered_views.is_empty());
    assert!(runtime.discovery.as_ref().unwrap().accounts.is_empty());
    assert_eq!(runtime.discovered_provider_views.len(), 1);
}

fn write_codex_only_global(config_root: &Path, codex_root: &Path) {
    write_registry(config_root, &[("codex", Agent::Codex, codex_root)]);
}

fn write_codex_workspace(path: &Path, root: &Path) {
    let id = path.file_stem().unwrap().to_str().unwrap();
    let config_root = path.parent().unwrap().parent().unwrap();
    let global = config_root.join("config.toml");
    let mut config: AppConfig = toml::from_str(&std::fs::read_to_string(&global).unwrap()).unwrap();
    config.accounts.insert(
        id.to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: id.to_owned(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: root.to_path_buf(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    std::fs::write(global, toml::to_string(&config).unwrap()).unwrap();
    std::fs::write(
        path,
        format!(
            r#"version = "{}"
workdir = "/workspace/project"
accounts = ["{id}"]
[[mounts]]
src = "/host/project"
dst = "/workspace/project"
"#,
            jackin_config::CURRENT_WORKSPACE_VERSION
        ),
    )
    .unwrap();
}

fn write_codex_auth(root: &Path, account_id: &str, email_payload: &str, token: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        format!(
            r#"{{"tokens":{{"access_token":"{token}","account_id":"{account_id}","id_token":"e30.{email_payload}.x"}}}}"#
        ),
    )
    .unwrap();
}

#[test]
fn disc_source_valid_profiles_resolve_without_network_or_fake_presence() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret",
    );
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].surface_id, "codex");
    assert_eq!(validated.accounts[0].account_label, "alice@example.test");
    let debug = format!("{validated:?}");
    assert!(!debug.contains("fixture-secret"));
    assert!(!debug.contains(profile.to_string_lossy().as_ref()));
}

#[test]
fn profile_material_rotation_at_same_path_changes_catalog_entry_revision() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret-a",
    );
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.clone(),
        operator_home: temp.path().join("home"),
    };
    let first_catalog = discover_usage_sources(&scope, &NoEnvResolver).unwrap();
    let first = validate_usage_sources(first_catalog, &NoEnvResolver);
    let first_entries = crate::host::broker::usage_catalog_entries(&first);

    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret-b",
    );
    let second_catalog = discover_usage_sources(&scope, &NoEnvResolver).unwrap();
    let second = validate_usage_sources(second_catalog, &NoEnvResolver);
    let second_entries = crate::host::broker::usage_catalog_entries(&second);

    assert_eq!(
        first.bindings[0].capability_id,
        second.bindings[0].capability_id
    );
    assert_ne!(
        first.bindings[0].credential_revision,
        second.bindings[0].credential_revision
    );
    assert_ne!(first_entries, second_entries);
}

#[test]
fn disc_config_generation_rotates_capability_for_same_credential_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.clone(),
        operator_home: temp.path().join("home"),
    };
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret",
    );
    let reader = RecordingProfileReader::default();
    let first = validate_usage_sources_with_reader(
        discover_usage_sources(&scope, &NoEnvResolver).unwrap(),
        &NoEnvResolver,
        &reader,
    );
    let first_capability = crate::host::usage_broker_capabilities(&first)
        .into_iter()
        .next()
        .unwrap();

    write_registry(&config_root, &[("codex-renamed", Agent::Codex, &profile)]);
    let second = validate_usage_sources_with_reader(
        discover_usage_sources(&scope, &NoEnvResolver).unwrap(),
        &NoEnvResolver,
        &reader,
    );
    let second_capability = crate::host::usage_broker_capabilities(&second)
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(
        first.accounts[0].account_key,
        second.accounts[0].account_key
    );
    assert_ne!(first.config_generation, second.config_generation);
    assert_ne!(first_capability, second_capability);
}

#[test]
fn disc_source_missing_and_malformed_profiles_are_isolated_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let workspaces = config_root.join("workspaces");
    let valid = temp.path().join("valid");
    let missing = temp.path().join("missing");
    let malformed = temp.path().join("malformed");
    write_codex_only_global(&config_root, &valid);
    std::fs::create_dir_all(&workspaces).unwrap();
    write_codex_workspace(&workspaces.join("missing.toml"), &missing);
    write_codex_workspace(&workspaces.join("malformed.toml"), &malformed);
    write_codex_auth(
        &valid,
        "account-valid",
        "eyJlbWFpbCI6InZhbGlkQGV4YW1wbGUudGVzdCJ9",
        "valid-secret",
    );
    std::fs::create_dir_all(&malformed).unwrap();
    std::fs::write(malformed.join("auth.json"), "{broken-secret").unwrap();
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert_eq!(validated.accounts.len(), 1);
    assert!(validated.diagnostics.iter().any(|diagnostic| {
        diagnostic.surface_id.as_deref() == Some("codex")
            && diagnostic.issue == UsageDiscoveryIssue::CredentialMissing
    }));
    assert!(validated.diagnostics.iter().any(|diagnostic| {
        diagnostic.surface_id.as_deref() == Some("codex")
            && diagnostic.issue == UsageDiscoveryIssue::CredentialMalformed
    }));
    let debug = format!("{:?}", validated.diagnostics);
    assert!(!debug.contains("broken-secret"));
    assert!(!debug.contains(temp.path().to_string_lossy().as_ref()));
}

#[test]
fn disc_source_kimi_profile_requires_credentials_in_selected_root() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let kimi_root = temp.path().join("kimi-profile");
    std::fs::create_dir_all(&config_root).unwrap();
    write_registry(&config_root, &[("kimi", Agent::Kimi, &kimi_root)]);
    let selected_credential = write_kimi_auth_route(
        &kimi_root,
        "https://api.kimi.com/coding/tenant/v1",
        "https://auth.kimi.com/tenant",
    );
    std::fs::write(
        selected_credential,
        r#"{"access_token":"selected-kimi-token"}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources(catalog, &NoEnvResolver);

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.bindings.len(), 1);
    assert!(validated.bindings[0].identity.is_none());
    let proof = validated.bindings[0].profile_material.as_ref().unwrap();
    let config = std::fs::read(kimi_root.join("config.toml")).unwrap();
    let slot = jackin_config::kimi_runtime_auth_slot(
        &config,
        jackin_config::KIMI_CODE_AUTH_SLOT_CONTRACT_VERSION,
        &BTreeMap::new(),
    )
    .unwrap();
    let slot_descriptor = serde_json::json!({"kimi_runtime_auth_slot": slot});
    assert_eq!(
        proof.source,
        jackin_core::profile_credential_source_identity(
            Agent::Kimi,
            "moonshot",
            &kimi_root,
            Some(&slot_descriptor),
        )
    );
    assert_eq!(
        proof.material_revision,
        jackin_core::profile_credential_material_revision(
            Agent::Kimi,
            br#"{"access_token":"selected-kimi-token"}"#
        )
        .unwrap()
    );
}

#[test]
fn disc_kimi_missing_selected_credentials_never_uses_other_home_profile() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("selected");
    let home = temp.path().join("home");
    let ambient = home.join(".kimi-code/credentials");
    let config_root = temp.path().join("config");
    let selected_credential = write_kimi_auth_route(
        &selected,
        "https://api.kimi.com/coding/tenant/v1",
        "https://auth.kimi.com/tenant",
    );
    std::fs::create_dir_all(&ambient).unwrap();
    std::fs::create_dir_all(selected.join("credentials")).unwrap();
    // This valid default-route token must not satisfy the selected custom route.
    std::fs::write(
        selected.join("credentials/kimi-code.json"),
        r#"{"access_token":"wrong-route-secret"}"#,
    )
    .unwrap();
    std::fs::write(
        ambient.join("kimi-code.json"),
        r#"{"access_token":"ambient-secret"}"#,
    )
    .unwrap();
    write_registry(&config_root, &[("kimi", Agent::Kimi, &selected)]);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(!selected_credential.exists());
    assert!(validated.bindings.is_empty());
    assert!(
        validated
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.issue == UsageDiscoveryIssue::CredentialMissing)
    );
}

#[test]
fn disc_amp_composite_profile_reads_selected_data_root() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("amp-work");
    let data = selected.join("data/amp");
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"selected-secret"}"#,
    )
    .unwrap();
    write_registry(&config_root, &[("amp-work", Agent::Amp, &selected)]);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.bindings.len(), 1);
    assert!(validated.bindings[0].identity.is_none());
    let proof = validated.bindings[0].profile_material.as_ref().unwrap();
    assert_eq!(
        proof.source,
        jackin_core::profile_credential_source_identity(Agent::Amp, "amp", &data, None)
    );
    assert_eq!(
        proof.material_revision,
        jackin_core::profile_credential_material_revision(
            Agent::Amp,
            br#"{"apiKey@https://ampcode.com/":"selected-secret"}"#
        )
        .unwrap()
    );
    assert!(!format!("{validated:?}").contains("selected-secret"));
}

#[test]
fn disc_dedup_repeated_roots_read_once_and_same_identity_merges() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let workspaces = config_root.join("workspaces");
    let shared = temp.path().join("shared-profile");
    let second = temp.path().join("second-profile");
    write_codex_only_global(&config_root, &shared);
    std::fs::create_dir_all(&workspaces).unwrap();
    write_codex_workspace(&workspaces.join("first.toml"), &shared);
    write_codex_workspace(&workspaces.join("second.toml"), &second);
    for (root, token) in [(&shared, "secret-one"), (&second, "secret-two")] {
        write_codex_auth(
            root,
            "same-provider-account",
            "eyJlbWFpbCI6InNhbWVAZXhhbXBsZS50ZXN0In0",
            token,
        );
    }
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    assert_eq!(
        catalog
            .candidates
            .iter()
            .filter(|candidate| candidate.surface_id == "codex")
            .count(),
        2
    );
    let capability_ids = catalog
        .candidates
        .iter()
        .map(|candidate| candidate.capability_id.clone())
        .collect::<Vec<_>>();
    assert!(
        capability_ids
            .iter()
            .all(|capability_id| capability_id.len() == 64),
        "source capability ids must be stable opaque hashes: {capability_ids:?}"
    );
    let rediscovered = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    assert_eq!(
        capability_ids,
        rediscovered
            .candidates
            .iter()
            .map(|candidate| candidate.capability_id.clone())
            .collect::<Vec<_>>()
    );

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].source_ids.len(), 2);
    assert_eq!(
        reader.reads.lock().unwrap().get(&shared.join("auth.json")),
        Some(&1)
    );
    assert_eq!(
        reader.reads.lock().unwrap().get(&second.join("auth.json")),
        Some(&1)
    );
    assert!(
        validated.accounts[0]
            .provenance
            .iter()
            .any(|scope| scope == "account codex")
    );
    assert!(
        validated.accounts[0]
            .provenance
            .iter()
            .any(|scope| scope == "workspace first")
    );
}

#[test]
fn disc_dedup_legacy_shared_snapshot_never_creates_active_row() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let mut historical = FocusedUsageView::unavailable("stale", 1);
    historical.focused_agent = Some("codex".to_owned());
    historical.focused_provider = Some("Codex".to_owned());
    historical.account.provider_label = "OpenAI / Codex".to_owned();
    historical.account.account_label = "removed@example.test".to_owned();
    std::fs::write(
        shared.join("usage-old.snapshot.json"),
        serde_json::to_vec(&historical).unwrap(),
    )
    .unwrap();
    let store = temp.path().join("missing.db");
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    };

    let catalog = crate::host::accounts::materialize_account_catalog(
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &store,
        Some(&discovery),
    )
    .unwrap();

    assert!(catalog.entries_for_surface(HostSurfaceId::Codex).is_empty());
}

#[test]
fn disc_cursor_token_profile_binds_refreshable_material() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let cursor_root = temp.path().join("cursor-work");
    std::fs::create_dir_all(&cursor_root).unwrap();
    write_registry(
        &config_root,
        &[("cursor-work", Agent::Cursor, &cursor_root)],
    );
    std::fs::write(
        cursor_root.join("auth.json"),
        r#"{"accessToken":"fixture-token","refreshToken":"fixture-refresh"}"#,
    )
    .unwrap();
    std::fs::write(
        cursor_root.join("cli-config.json"),
        r#"{"authInfo":{"email":"work@example.test"}}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "work@example.test");
    assert_eq!(validated.bindings.len(), 1);
    match &validated.bindings[0].source {
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor {
            auth,
            identity,
        }) => {
            assert!(!auth.access_token.is_empty());
            assert_eq!(identity.as_deref(), Some("work@example.test"));
        }
        _ => panic!("cursor token profile must bind refreshable material"),
    }
}

#[test]
fn disc_cursor_tokenless_profile_is_malformed_without_binding() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let cursor_root = temp.path().join("cursor-bare");
    std::fs::create_dir_all(&cursor_root).unwrap();
    write_registry(
        &config_root,
        &[("cursor-bare", Agent::Cursor, &cursor_root)],
    );
    std::fs::write(
        cursor_root.join("auth.json"),
        r#"{"refreshToken":"only-refresh"}"#,
    )
    .unwrap();
    std::fs::write(
        cursor_root.join("cli-config.json"),
        r#"{"authInfo":{"email":"bare@example.test"}}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    assert!(validated.bindings.is_empty());
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert!(matches!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::CredentialMalformed
    ));
}

#[test]
fn disc_cursor_profile_mints_material_with_cli_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cursor");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        r#"{"accessToken":"fixture-opaque-token"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("cli-config.json"),
        r#"{"authInfo":{"email":"cursor@example.test"}}"#,
    )
    .unwrap();
    let reader = RecordingProfileReader::default();

    let ProfileValidation::Authenticated {
        account_label,
        material: Some(material),
        ..
    } = profile_identity(&reader, Agent::Cursor, &root, temp.path())
    else {
        panic!("cursor profile must authenticate with material");
    };
    assert_eq!(account_label.as_deref(), Some("cursor@example.test"));
    assert!(matches!(
        *material,
        ProfileCredentialMaterial::Cursor { .. }
    ));
}

#[test]
fn disc_cursor_profile_tokenless_is_malformed_and_missing_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cursor");
    std::fs::create_dir_all(&root).unwrap();
    let reader = RecordingProfileReader::default();

    assert!(matches!(
        profile_identity(&reader, Agent::Cursor, &root, temp.path()),
        ProfileValidation::Missing
    ));

    std::fs::write(root.join("auth.json"), r#"{"noToken":true}"#).unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Cursor, &root, temp.path()),
        ProfileValidation::Malformed
    ));
}

#[test]
fn disc_gemini_profile_mints_material_with_or_without_label() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("gemini");
    std::fs::create_dir_all(&root).unwrap();
    let reader = RecordingProfileReader::default();

    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Missing
    ));

    std::fs::write(
        root.join("oauth_creds.json"),
        r#"{"user_email":"g@example.test"}"#,
    )
    .unwrap();
    let ProfileValidation::Authenticated {
        account_label,
        material: Some(material),
        ..
    } = profile_identity(&reader, Agent::Gemini, &root, temp.path())
    else {
        panic!("gemini profile must authenticate with material");
    };
    assert_eq!(account_label.as_deref(), Some("g@example.test"));
    assert!(matches!(
        *material,
        ProfileCredentialMaterial::Gemini { .. }
    ));

    std::fs::write(root.join("oauth_creds.json"), "{}").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Anonymous(Some(_))
    ));

    std::fs::write(root.join("oauth_creds.json"), b"{invalid").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Gemini, &root, temp.path()),
        ProfileValidation::Malformed
    ));
}

#[test]
fn disc_blocked_providers_mint_no_refresh_material() {
    let temp = tempfile::tempdir().unwrap();
    let reader = RecordingProfileReader::default();
    // omp/hermes: attribution-only adapters; presence never mints material.
    let omp = temp.path().join("omp");
    std::fs::create_dir_all(omp.join("agent")).unwrap();
    std::fs::write(omp.join("agent/agent.db"), b"sqlite fixture").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Omp, &omp, temp.path()),
        ProfileValidation::Anonymous(None)
    ));
    let hermes = temp.path().join("hermes");
    std::fs::create_dir_all(&hermes).unwrap();
    std::fs::write(hermes.join("auth.json"), "{}").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Hermes, &hermes, temp.path()),
        ProfileValidation::Anonymous(None)
    ));
    // Muse: local identity only; the secret stays in the platform store and
    // no pollable fetch exists, so no material.
    let muse = temp.path().join("muse");
    std::fs::create_dir_all(&muse).unwrap();
    std::fs::write(
        muse.join("auth.json"),
        r#"{"providers":{"meta":{"user_email":"m@example.test"}}}"#,
    )
    .unwrap();
    let ProfileValidation::Authenticated { material: None, .. } =
        profile_identity(&reader, Agent::Muse, &muse, temp.path())
    else {
        panic!("muse profile must carry identity without material");
    };
}

/// File-blind reader with a configurable Antigravity Keychain grant outcome.
struct AntigravityGrantReader {
    grant: ProfileReadOutcome,
}

impl ProfileCredentialReader for AntigravityGrantReader {
    fn read(&self, _path: &Path) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }

    fn exists(&self, _path: &Path) -> bool {
        false
    }

    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        panic!("Claude is ignored in Antigravity fixtures")
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        self.grant.clone()
    }
}

#[test]
fn disc_antigravity_grant_mints_cli_refresh_material() {
    let temp = tempfile::tempdir().unwrap();
    // Grant present (payload always empty; presence is the whole answer) →
    // anonymous binding with CLI refresh material.
    for grant in [
        ProfileReadOutcome::Bytes(Vec::new()),
        ProfileReadOutcome::Bytes(vec![1, 2, 3]),
    ] {
        let reader = AntigravityGrantReader { grant };
        let ProfileValidation::Anonymous(Some(material)) =
            profile_identity(&reader, Agent::Antigravity, temp.path(), temp.path())
        else {
            panic!("antigravity grant must mint anonymous CLI material");
        };
        assert!(matches!(*material, ProfileCredentialMaterial::Antigravity));
    }
    // Grant absent/denied propagates truthfully, never a phantom binding.
    for (grant, expected) in [
        (ProfileReadOutcome::Missing, "missing"),
        (ProfileReadOutcome::Denied, "denied"),
    ] {
        let reader = AntigravityGrantReader { grant };
        let outcome = profile_identity(&reader, Agent::Antigravity, temp.path(), temp.path());
        assert!(
            matches!(
                outcome,
                ProfileValidation::Missing | ProfileValidation::Denied
            ),
            "antigravity without grant must be {expected}"
        );
    }
}

#[test]
fn disc_material_less_profile_binding_is_unpollable() {
    let temp = tempfile::tempdir().unwrap();
    let reader = RecordingProfileReader::default();
    // Muse: local identity, no material → Unpollable, never Capability.
    let muse = temp.path().join("muse");
    std::fs::create_dir_all(&muse).unwrap();
    std::fs::write(
        muse.join("auth.json"),
        r#"{"providers":{"meta":{"user_email":"m@example.test"}}}"#,
    )
    .unwrap();
    let parts = validate_source(
        DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Meta,
            agent: Agent::Muse,
            provider: "meta".to_owned(),
            selector: None,
            root: muse,
            operator_home: temp.path().to_path_buf(),
            account_label: None,
            source_id: "source-muse".to_owned(),
            capability_id: "cap-muse".to_owned(),
            provenance: BTreeSet::new(),
            configured_account_ids: BTreeSet::new(),
        },
        &NoEnvResolver,
        &reader,
    );
    assert!(matches!(parts.6, ValidatedCredentialSource::Unpollable));
    // omp: attribution-only presence, no material → Unpollable as well.
    let omp = temp.path().join("omp");
    std::fs::create_dir_all(omp.join("agent")).unwrap();
    std::fs::write(omp.join("agent/agent.db"), b"sqlite fixture").unwrap();
    let parts = validate_source(
        DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::OpenRouter,
            agent: Agent::Omp,
            provider: "openrouter".to_owned(),
            selector: None,
            root: omp,
            operator_home: temp.path().to_path_buf(),
            account_label: None,
            source_id: "source-omp".to_owned(),
            capability_id: "cap-omp".to_owned(),
            provenance: BTreeSet::new(),
            configured_account_ids: BTreeSet::new(),
        },
        &NoEnvResolver,
        &reader,
    );
    assert!(matches!(parts.6, ValidatedCredentialSource::Unpollable));
}

#[test]
fn refresh_unpollable_binding_returns_honest_unsupported() {
    let binding = test_binding(HostSurfaceId::Meta, ValidatedCredentialSource::Unpollable);
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
            assert_eq!(
                view.last_error.as_deref(),
                Some("usage polling not supported for this provider")
            );
            assert!(view.buckets.is_empty());
            assert!(view.account.account_label.is_empty());
        }
        other => panic!("unpollable refresh must return an honest snapshot: {other:?}"),
    }
}

fn test_binding(
    surface: HostSurfaceId,
    source: ValidatedCredentialSource,
) -> ValidatedCredentialBinding {
    ValidatedCredentialBinding {
        surface,
        identity: None,
        source_id: "source-test".to_owned(),
        capability_id: "cap-test".to_owned(),
        credential_revision: "credential-revision-test".to_owned(),
        profile_material: None,
        provenance: BTreeSet::new(),
        configured_account_ids: BTreeSet::new(),
        source,
    }
}

#[test]
fn refresh_cursor_binding_dispatches_to_collector() {
    // Live provider RPC with a fixture token: the dashboard rejects it, so
    // the arm must return the collector's honest Stale view — never
    // Malformed/Unsupported, which would mean dispatch never happened. The
    // material carries the profile root; refresh re-reads it.
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth.json");
    std::fs::write(&auth_path, r#"{"accessToken":"fixture-opaque-token"}"#).unwrap();
    let binding = test_binding(
        HostSurfaceId::Cursor,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor {
            auth: crate::usage::cursor_auth_from_value(
                &serde_json::json!({"accessToken": "fixture-opaque-token"}),
            )
            .unwrap(),
            identity: None,
        }),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Stale);
            assert_eq!(view.account.provider_label, "Cursor");
            assert_eq!(view.focused_agent.as_deref(), Some("cursor"));
            assert!(view.last_error.is_some());
        }
        other => panic!("cursor refresh must dispatch to the collector: {other:?}"),
    }
}

#[test]
fn refresh_antigravity_binding_dispatches_to_cli() {
    // Live `agy` shell-out (cursor precedent above): any collector view —
    // Fresh quota, Stale, or a version-gate NeedsSecret — proves dispatch.
    // Only Malformed/Unsupported-by-discovery would mean the arm never ran.
    let binding = test_binding(
        HostSurfaceId::Google,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.account.provider_label, "Antigravity");
            assert_eq!(view.focused_agent.as_deref(), Some("gemini"));
            assert!(!view.is_refreshing_placeholder());
        }
        other => panic!("antigravity refresh must dispatch to the CLI collector: {other:?}"),
    }
}

#[test]
fn disc_account_aliases_avoid_governed_names_and_round_trip() {
    let mut seen = BTreeSet::new();
    for entry in jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY {
        let alias = usage_account_alias_entry(*entry, entry.owner);
        assert_eq!(alias.owner, entry.owner);
        assert_ne!(alias.name, entry.name);
        assert!(
            !jackin_core::is_account_env(alias.name),
            "alias must survive operator-env attribution: {}",
            alias.name
        );
        assert_eq!(governed_name_for_account_alias(alias.name), entry.name);
        assert!(seen.insert(alias.name), "duplicate alias: {}", alias.name);
    }
    assert_eq!(
        governed_name_for_account_alias("ZAI_API_KEY"),
        "ZAI_API_KEY"
    );
}

#[test]
fn disc_zai_aliases_keep_one_canonical_owner_and_dispatch_route() {
    for name in ["ZAI_API_KEY", "ZHIPU_API_KEY", "Z_AI_API_KEY"] {
        let entry = UsageCredentialEnvName {
            name,
            owner: UsageCredentialOwner::Zai,
        };
        let alias = usage_account_alias_entry(entry, UsageCredentialOwner::Zai);
        assert_eq!(alias.owner, UsageCredentialOwner::Zai);
        assert_eq!(
            super::super::credential_resolver::dispatch_key_for_route(
                alias.owner,
                governed_name_for_account_alias(alias.name),
            ),
            "ZAI_API_KEY"
        );
    }
}

/// Mimics the CLI/broker secret-source split: resolves only the exact
/// requested declaration from the isolated config, deduplicating identical
/// secrets per owner behind one opaque handle.
#[derive(Default)]
struct SecretDedupFakeResolver {
    calls: Mutex<Vec<Vec<String>>>,
    handles: Mutex<BTreeMap<String, OpaqueCredentialHandle>>,
}

impl ProviderCredentialEnvResolver for SecretDedupFakeResolver {
    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        self.calls
            .lock()
            .unwrap()
            .push(keys.iter().map(|key| key.name.to_owned()).collect());
        keys.iter()
            .filter_map(|entry| {
                let declaration = config.env.get(entry.name)?;
                let fingerprint = format!("{:?}:{declaration:?}", entry.owner);
                let mut handles = self.handles.lock().unwrap();
                let next = handles.len() + 1;
                let handle = handles
                    .entry(fingerprint)
                    .or_insert_with(|| {
                        OpaqueCredentialHandle::new(format!("fixture-credential-{next}"))
                    })
                    .clone();
                Some(ProviderCredentialEnvResolution {
                    key: entry.name.to_owned(),
                    outcome: ProviderCredentialEnvOutcome::Resolved(handle),
                })
            })
            .collect()
    }
}

fn write_accounts_config(
    config_root: &Path,
    profiles: &[(&str, Agent, &Path)],
    keys: &[(&str, AiProvider, &str)],
) {
    let mut config = AppConfig::default();
    for (id, agent, directory) in profiles {
        config.accounts.insert(
            (*id).to_owned(),
            jackin_config::AccountConfig {
                enabled: true,
                name: (*id).to_owned(),
                provider: AiProvider::for_agent(*agent).expect("native-provider agent"),
                credential: AccountCredential::Profile {
                    agent: *agent,
                    directory: directory.to_path_buf(),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    for (id, provider, value) in keys {
        config.accounts.insert(
            (*id).to_owned(),
            jackin_config::AccountConfig {
                enabled: true,
                name: (*id).to_owned(),
                provider: *provider,
                credential: AccountCredential::ApiKey {
                    value: jackin_config::EnvValue::Plain((*value).to_owned()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    std::fs::create_dir_all(config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
}

fn discover_with(
    config_root: &Path,
    home: &Path,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> UsageDiscoveryCatalog {
    discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.to_path_buf(),
            operator_home: home.to_path_buf(),
        },
        resolver,
    )
    .unwrap()
}

#[test]
fn disc_shared_profile_failed_diagnostic_keeps_exact_configured_ids() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-shared");
    let mut config = AppConfig::default();
    for (id, name) in [
        ("grant-one", "Display | one"),
        ("grant-two", "Display, account two"),
    ] {
        config.accounts.insert(
            id.to_owned(),
            jackin_config::AccountConfig {
                enabled: true,
                name: name.to_owned(),
                provider: AiProvider::OpenAi,
                credential: AccountCredential::Profile {
                    agent: Agent::Codex,
                    directory: profile.clone(),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = FakeEnvResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    let expected = BTreeSet::from(["grant-one".to_owned(), "grant-two".to_owned()]);
    assert!(resolver.calls.lock().unwrap().is_empty());
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].configured_account_ids, expected);
    let source_ids = catalog
        .sources
        .iter()
        .map(|source| match source {
            DiscoveredCredentialSource::Profile {
                configured_account_ids,
                ..
            }
            | DiscoveredCredentialSource::Env {
                configured_account_ids,
                ..
            }
            | DiscoveredCredentialSource::Capability {
                configured_account_ids,
                ..
            } => configured_account_ids,
        })
        .collect::<Vec<_>>();
    assert_eq!(source_ids, vec![&expected]);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());
    assert!(validated.accounts.is_empty());
    assert!(validated.bindings.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert_eq!(validated.diagnostics[0].configured_account_ids, expected);

    // Display names and punctuation only affect the human scope label. The
    // diagnostic association remains the exact registry IDs above.
    assert!(
        validated.diagnostics[0]
            .scope_label
            .contains("account grant-one")
    );
    assert!(
        validated.diagnostics[0]
            .scope_label
            .contains("account grant-two")
    );
    assert!(
        !validated.diagnostics[0]
            .scope_label
            .contains("Display | one")
    );
    assert!(
        !validated.diagnostics[0]
            .scope_label
            .contains("Display, account two")
    );

    config.accounts.get_mut("grant-one").unwrap().name = "renamed; delimiter".to_owned();
    config.accounts.get_mut("grant-two").unwrap().name = "renamed | second".to_owned();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let renamed_catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(
        renamed_catalog.candidates[0].configured_account_ids,
        expected
    );
    let renamed = validate_usage_sources_with_reader(
        renamed_catalog,
        &resolver,
        &RecordingProfileReader::default(),
    );
    assert_eq!(renamed.diagnostics.len(), 1);
    assert_eq!(renamed.diagnostics[0].configured_account_ids, expected);
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_env_key_account_resolves_through_isolated_alias() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "codex");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(
        calls
            .iter()
            .all(|call| { call.len() == 1 && call[0] == "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY" })
    );

    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.bindings.len(), 1);
    assert!(validated.bindings[0].identity.is_none());
    // Canonical ownership remains separate from exact provider dispatch.
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env {
            ref key,
            ref dispatch_key,
            ..
        } if key == "OPENAI_API_KEY" && dispatch_key == "OPENAI_API_KEY"
    ));
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
    assert_eq!(validated.unresolved_capabilities().count(), 1);
}

#[test]
fn disc_oauth_token_account_resolves_through_isolated_alias() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "oa-claude".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "oa-claude".to_owned(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: jackin_config::EnvValue::Plain("fixture-oauth-token".to_owned()),
            },
        },
    );
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::OAuthToken
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(
        calls[0],
        vec!["JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN"]
    );
    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env { ref key, .. } if key == "ANTHROPIC_API_KEY"
    ));
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env { ref dispatch_key, .. }
            if dispatch_key == "CLAUDE_CODE_OAUTH_TOKEN"
    ));
}

#[test]
fn disc_anonymous_env_key_cannot_borrow_same_provider_profile_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-shared");
    write_accounts_config(
        &config_root,
        &[("codex-profile", Agent::Codex, &profile)],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    write_codex_auth(
        &profile,
        "same-provider-account",
        "eyJlbWFpbCI6InNhbWVAZXhhbXBsZS50ZXN0In0",
        "fixture-secret",
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 2);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "same@example.test");
    assert_eq!(
        validated.accounts[0].provenance,
        vec!["account codex-profile"]
    );
    assert_eq!(validated.accounts[0].source_ids.len(), 1);
    assert!(matches!(
        validated.accounts[0].identity.subject,
        CanonicalAccountSubject::ProviderId(_)
    ));
    assert_eq!(validated.bindings.len(), 2);
    assert_eq!(
        validated
            .bindings
            .iter()
            .filter(|binding| binding.identity.is_some())
            .count(),
        1
    );
    assert_eq!(
        validated
            .bindings
            .iter()
            .filter(|binding| binding.identity.is_none())
            .count(),
        1
    );
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 2);
    assert_eq!(validated.unresolved_capabilities().count(), 1);
}

#[test]
fn disc_distinct_anonymous_env_keys_keep_distinct_unresolved_capabilities() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[
            ("key-one", AiProvider::OpenAi, "fixture-key-one"),
            ("key-two", AiProvider::OpenAi, "fixture-key-two"),
        ],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 2);

    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.unresolved_capabilities().count(), 2);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 2);
}

#[test]
fn disc_same_env_key_through_two_accounts_dedupes_to_one_source() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[
            ("or-alpha", AiProvider::OpenRouter, "fixture-or-shared-key"),
            ("or-beta", AiProvider::OpenRouter, "fixture-or-shared-key"),
        ],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].provenance.len(), 2);

    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    assert_eq!(validated.bindings[0].provenance.len(), 2);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
}

#[test]
fn disc_anonymous_env_key_without_profile_retains_capability_without_account_row() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[("xai-key", AiProvider::Xai, "xai-fixture-secret")],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
    assert_eq!(validated.unresolved_capabilities().count(), 1);
}

#[test]
fn disc_env_key_with_two_provider_identities_stays_separate() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let first = temp.path().join("first-profile");
    let second = temp.path().join("second-profile");
    write_accounts_config(
        &config_root,
        &[
            ("codex-one", Agent::Codex, &first),
            ("codex-two", Agent::Codex, &second),
        ],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    write_codex_auth(
        &first,
        "account-one",
        "eyJlbWFpbCI6Im9uZUBleGFtcGxlLnRlc3QifQ",
        "secret-one",
    );
    write_codex_auth(
        &second,
        "account-two",
        "eyJlbWFpbCI6InR3b0BleGFtcGxlLnRlc3QifQ",
        "secret-two",
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    // Neither authenticated sibling supplies proof for the anonymous key.
    assert_eq!(validated.accounts.len(), 2);
    assert_eq!(validated.unresolved_capabilities().count(), 1);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 3);
}

#[test]
fn disc_env_key_does_not_attach_to_label_only_profile() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_accounts_config(
        &config_root,
        &[("codex-profile", Agent::Codex, &profile)],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    // OAuth material without provider identity and a configured display name
    // remains anonymous; the key cannot borrow an identity from that name.
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("auth.json"),
        r#"{"tokens":{"access_token":"fixture-secret"}}"#,
    )
    .unwrap();
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    assert!(validated.accounts.is_empty());
    assert_eq!(validated.unresolved_capabilities().count(), 2);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 2);
}

#[test]
fn refresh_gemini_binding_dispatches_to_collector() {
    let temp = tempfile::tempdir().unwrap();
    let creds = temp.path().join("oauth_creds.json");
    std::fs::write(&creds, "{}").unwrap();
    let binding = test_binding(
        HostSurfaceId::Google,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Gemini {
            creds_path: creds.clone(),
        }),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
            assert_eq!(view.account.provider_label, "Google");
            assert_eq!(view.focused_agent.as_deref(), Some("gemini"));
        }
        other => panic!("gemini refresh must dispatch to the collector: {other:?}"),
    }
    // A credential file deleted after discovery re-proves as NeedsSecret.
    std::fs::remove_file(&creds).unwrap();
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
        }
        other => panic!("deleted gemini creds must need secret: {other:?}"),
    }
}

mod disabled_open;

#[test]
fn profile_descriptor_preserves_provider_and_full_selector() {
    let root = PathBuf::from("/synthetic/profiles/store");
    let key = |provider: &str, entry: &str, profile: Option<&str>| CredentialSourceKey::Profile {
        agent: Agent::Hermes,
        provider: provider.to_owned(),
        selector: Some((entry.to_owned(), profile.map(str::to_owned))),
        root: root.clone(),
    };
    let base = source_capability_id(
        HostSurfaceId::OpenRouter,
        &key("openrouter", "account", Some("first")),
    );
    for changed in [
        key("openrouter ", "account", Some("first")),
        key("openrouter", "account ", Some("first")),
        key("openrouter", "account", Some("second")),
        key("openrouter", "account", None),
    ] {
        assert_ne!(
            base,
            source_capability_id(HostSurfaceId::OpenRouter, &changed)
        );
    }
}

struct RotatingCodexProfileReader {
    reads: std::cell::Cell<usize>,
}

impl ProfileCredentialReader for RotatingCodexProfileReader {
    fn read(&self, _path: &Path) -> ProfileReadOutcome {
        let read = self.reads.get();
        self.reads.set(read + 1);
        ProfileReadOutcome::Bytes(if read == 0 {
            br#"{"tokens":{"access_token":"captured-fixture","account_id":" exact-id "}}"#.to_vec()
        } else {
            br#"{"tokens":{"access_token":"rotated-fixture","account_id":"replacement"}}"#.to_vec()
        })
    }
    fn exists(&self, _path: &Path) -> bool {
        false
    }
    fn read_claude_keychain(
        &self,
        _scope: &jackin_core::ClaudeKeychainScope,
    ) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }
    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        ProfileReadOutcome::Missing
    }
}

#[test]
fn profile_proof_and_dispatch_material_share_one_captured_read() {
    let root = PathBuf::from("/synthetic/codex/profile");
    let reader = RotatingCodexProfileReader {
        reads: std::cell::Cell::new(0),
    };
    let catalog = UsageDiscoveryCatalog {
        config_generation: None,
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        sources: vec![DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Codex,
            agent: Agent::Codex,
            provider: "openai".to_owned(),
            selector: None,
            root: root.clone(),
            operator_home: PathBuf::from("/synthetic/home"),
            account_label: Some("presentation-only".to_owned()),
            source_id: "source-ordinal".to_owned(),
            capability_id: "exact-source".to_owned(),
            provenance: BTreeSet::new(),
            configured_account_ids: BTreeSet::from(["registered".to_owned()]),
        }],
    };
    let discovery = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);
    assert_eq!(reader.reads.get(), 1);
    assert_eq!(discovery.bindings.len(), 1);
    let binding = &discovery.bindings[0];
    assert_eq!(
        binding.identity.as_ref().unwrap().subject,
        CanonicalAccountSubject::ProviderId(" exact-id ".to_owned())
    );
    let proof = binding.profile_material.as_ref().unwrap();
    assert_eq!(
        proof.source,
        jackin_core::profile_credential_source_identity(Agent::Codex, "openai", &root, None)
    );
    assert_eq!(
        proof.material_revision,
        jackin_core::profile_credential_material_revision(
            Agent::Codex,
            br#"{"tokens":{"access_token":"captured-fixture","account_id":" exact-id "}}"#
        )
        .unwrap()
    );
    match &binding.source {
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex { credentials }) => {
            assert_eq!(credentials.access_token, "captured-fixture");
        }
        _ => panic!("captured profile must carry captured material"),
    }
}

#[test]
fn claude_metadata_cannot_substitute_for_primary_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("claude");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(".claude.json"),
        br#"{"claudeAiOauth":{"accessToken":"metadata-fixture"}}"#,
    )
    .unwrap();
    struct MissingKeychainReader(RecordingProfileReader);
    impl ProfileCredentialReader for MissingKeychainReader {
        fn read(&self, path: &Path) -> ProfileReadOutcome {
            self.0.read(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.0.exists(path)
        }
        fn read_claude_keychain(
            &self,
            _scope: &jackin_core::ClaudeKeychainScope,
        ) -> ProfileReadOutcome {
            ProfileReadOutcome::Missing
        }
        fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
            ProfileReadOutcome::Missing
        }
    }
    let reader = MissingKeychainReader(RecordingProfileReader::default());
    assert!(matches!(
        claude_profile_identity(&reader, &root, temp.path()),
        ProfileValidation::Missing
    ));
    assert!(
        captured_profile_material(
            &reader,
            Agent::Claude,
            "anthropic",
            None,
            &root,
            temp.path()
        )
        .is_none()
    );
}

#[test]
fn profile_discovery_uses_explicit_xdg_data_root_for_selected_instance() {
    let mut config = AppConfig::default();
    for (id, agent, provider) in [
        ("amp", Agent::Amp, AiProvider::Amp),
        ("opencode", Agent::Opencode, AiProvider::Opencode),
    ] {
        config.accounts.insert(
            id.to_owned(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "presentation label".to_owned(),
                provider,
                credential: AccountCredential::Profile {
                    agent,
                    directory: PathBuf::from("/synthetic/nominal"),
                    xdg_roots: Some(jackin_config::XdgRoots {
                        data: PathBuf::from("/synthetic/selected/data"),
                        config: PathBuf::from("/synthetic/selected/config"),
                        cache: PathBuf::from("/synthetic/selected/cache"),
                    }),
                    source_selector: None,
                },
            },
        );
    }
    let mut candidates = BTreeMap::new();
    let mut diagnostics = Vec::new();
    enumerate_registered_accounts(
        &config,
        Path::new("/synthetic/home"),
        &NoEnvResolver,
        &mut candidates,
        &mut diagnostics,
    );
    let catalog = materialize_catalog(None, candidates, diagnostics);
    assert!(catalog.diagnostics.is_empty());
    assert_eq!(catalog.sources.len(), 2);
    for source in catalog.sources {
        let DiscoveredCredentialSource::Profile {
            agent,
            root,
            configured_account_ids,
            ..
        } = source
        else {
            panic!("profile expected");
        };
        assert_eq!(
            root,
            PathBuf::from("/synthetic/selected/data").join(agent.slug())
        );
        assert_eq!(
            configured_account_ids,
            BTreeSet::from([agent.slug().to_owned()])
        );
    }
}

struct RevisionEvidenceResolver(ProviderCredentialSourceMaterial);

impl ProviderCredentialEnvResolver for RevisionEvidenceResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }

    fn source_material(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> Option<ProviderCredentialSourceMaterial> {
        Some(self.0.clone())
    }
}

#[test]
fn env_catalog_revision_binds_exact_source_and_captured_material() {
    let revision = |source, material_fingerprint: &str| {
        let resolver = RevisionEvidenceResolver(ProviderCredentialSourceMaterial {
            source,
            material_fingerprint: material_fingerprint.to_owned(),
        });
        let discovery = validate_usage_sources_with_reader(
            UsageDiscoveryCatalog {
                config_generation: None,
                candidates: Vec::new(),
                diagnostics: Vec::new(),
                sources: vec![DiscoveredCredentialSource::Env {
                    surface: HostSurfaceId::Codex,
                    handle: OpaqueCredentialHandle::new("same-handle"),
                    key: "OPENAI_API_KEY".to_owned(),
                    dispatch_key: "OPENAI_API_KEY".to_owned(),
                    launch_keys: BTreeSet::from(["OPENAI_API_KEY".to_owned()]),
                    kind: UsageCredentialKind::ApiKey,
                    account_label: None,
                    source_id: "same-source-ordinal".to_owned(),
                    capability_id: "same-capability".to_owned(),
                    provenance: BTreeSet::new(),
                    configured_account_ids: BTreeSet::from(["registered".to_owned()]),
                }],
            },
            &resolver,
            &RecordingProfileReader::default(),
        );
        assert!(discovery.diagnostics.is_empty());
        assert_eq!(discovery.bindings.len(), 1);
        discovery.bindings[0].credential_revision.clone()
    };
    let source = UsageCredentialSourceIdentity::OnePassword {
        reference: "op://synthetic-vault/synthetic-item/token".to_owned(),
        account: Some("selected-account".to_owned()),
    };
    let original = revision(source.clone(), "synthetic-token-fingerprint");
    assert_eq!(
        original,
        revision(source.clone(), "synthetic-token-fingerprint")
    );
    assert_ne!(
        original,
        revision(source.clone(), "rotated-token-fingerprint")
    );
    for changed_source in [
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://synthetic-vault/replacement-item/token".to_owned(),
            account: Some("selected-account".to_owned()),
        },
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://synthetic-vault/synthetic-item/token".to_owned(),
            account: Some("different-account".to_owned()),
        },
        UsageCredentialSourceIdentity::HostEnv {
            name: "SYNTHETIC_TOKEN".to_owned(),
        },
        UsageCredentialSourceIdentity::Literal,
    ] {
        assert_ne!(
            original,
            revision(changed_source, "synthetic-token-fingerprint")
        );
    }
}

#[test]
fn amp_profile_accepts_only_one_canonical_server_credential() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("secrets.json");
    let reader = RecordingProfileReader::default();
    for malformed in [
        serde_json::json!({"apiKey@https://foreign.example/": "foreign-fixture"}),
        serde_json::json!({"mcpAuth@https://mcp.example/": "mcp-fixture"}),
        serde_json::json!({"apiKey@https://ampcode.com": "first-fixture", "apiKey@https://ampcode.com/": "second-fixture"}),
    ] {
        std::fs::write(&path, malformed.to_string()).unwrap();
        assert!(matches!(
            amp_profile_identity(&reader, &path),
            ProfileValidation::Malformed
        ));
    }
    std::fs::write(
        &path,
        serde_json::json!({
            "apiKey@https://foreign.example/": "foreign-fixture",
            "mcpAuth@https://mcp.example/": "mcp-fixture",
            "apiKey@https://ampcode.com": " exact-canonical-fixture ",
        })
        .to_string(),
    )
    .unwrap();
    let ProfileValidation::Anonymous(Some(material)) = amp_profile_identity(&reader, &path) else {
        panic!(
            "canonical server credential must remain anonymous until provider identity evidence"
        );
    };
    let ProfileCredentialMaterial::Amp { key } = *material else {
        panic!("Amp material expected");
    };
    assert_eq!(key, " exact-canonical-fixture ");
}

#[test]
fn opencode_native_auth_cannot_supply_a_different_declared_provider() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("opencode-profile");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("auth.json"),
        br#"{"opencode-go":{"type":"api","key":"native-opencode-fixture"}}"#,
    )
    .unwrap();
    let account = jackin_config::AccountConfig {
        enabled: true,
        name: "presentation-only".to_owned(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::Profile {
            agent: Agent::Opencode,
            directory: profile,
            xdg_roots: None,
            source_selector: None,
        },
    };
    assert!(account.supports_agent(Agent::Opencode));
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("selected-openai".to_owned(), account);
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let resolver = FakeEnvResolver::default();
    let catalog = discover_with(&config_root, temp.path(), &resolver);
    assert!(catalog.diagnostics.is_empty());
    assert_eq!(catalog.candidates.len(), 1);
    let reader = RecordingProfileReader::default();
    let validated = validate_usage_sources_with_reader(catalog, &resolver, &reader);
    assert!(validated.bindings.is_empty());
    assert!(validated.accounts.is_empty());
    assert!(crate::host::usage_broker_capabilities(&validated).is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert_eq!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::CredentialMalformed
    );
    assert_eq!(
        validated.diagnostics[0].configured_account_ids,
        BTreeSet::from(["selected-openai".to_owned()])
    );
    assert!(reader.reads.lock().unwrap().is_empty());
    assert!(resolver.calls.lock().unwrap().is_empty());
}
