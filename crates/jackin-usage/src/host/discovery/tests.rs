use std::sync::Mutex;

use jackin_protocol::control::UsageConfidence;

use super::*;

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

#[test]
fn env_capability_ids_isolate_distinct_opaque_credentials() {
    let first = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-1"),
        key: "ZAI_API_KEY".to_owned(),
    };
    let second = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-2"),
        key: "ZAI_API_KEY".to_owned(),
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
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].2, vec!["ZAI_API_KEY"]);
    assert!(!format!("{catalog:?}").contains("fixture-key"));
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
        vec!["OPENROUTER_API_KEY"]
    );
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
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "opencode".to_owned(),
                    capability_id: "excluded".to_owned(),
                    account_label: None,
                },
            ],
        },
        &resolver,
    )
    .unwrap();

    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "claude");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ForwardedCapability
    );
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_same_provider_sources_with_same_labels_keep_source_capabilities_distinct() {
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: Some("same@example.test".to_owned()),
                },
                ForwardedUsageAccount {
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
fn disc_unresolved_same_labels_do_not_overwrite_discovered_views() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
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

    assert_eq!(runtime.discovered_views.len(), 2);
    assert_eq!(
        runtime
            .discovered_views
            .values()
            .map(|view| view.status_bar_label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["source-0", "source-1"])
    );
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
    std::fs::create_dir_all(&kimi_root).unwrap();
    std::fs::create_dir_all(&config_root).unwrap();
    write_registry(&config_root, &[("kimi", Agent::Kimi, &kimi_root)]);
    std::fs::create_dir_all(kimi_root.join("credentials")).unwrap();
    std::fs::write(
        kimi_root.join("credentials/kimi-code.json"),
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
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "kimi");
    assert_eq!(validated.bindings.len(), 1);
}

#[test]
fn disc_kimi_missing_selected_credentials_never_uses_other_home_profile() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("selected");
    let home = temp.path().join("home");
    let ambient = home.join(".kimi/credentials");
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&selected).unwrap();
    std::fs::create_dir_all(&ambient).unwrap();
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
        r#"{"apiKey@work@example.test":"selected-secret"}"#,
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
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "work@example.test");
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

    let catalog = crate::host::accounts::materialize_account_catalog(
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &store,
        Some(&[]),
    )
    .unwrap();

    assert!(catalog.entries_for_surface(HostSurfaceId::Codex).is_empty());
}
