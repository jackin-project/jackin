use std::sync::Mutex;

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
                _ => None,
            })
            .collect()
    }
}

fn write_workspace(path: &Path, claude: &str, codex: &str, amp: &str, ignore_grok: bool) {
    let grok = if ignore_grok {
        "\n[grok]\nauth_forward = \"ignore\"\n"
    } else {
        ""
    };
    std::fs::write(
        path,
        format!(
            r#"version = "{version}"
workdir = "/workspace/project"
allowed_roles = ["reviewer"]

[[mounts]]
src = "/host/project"
dst = "/workspace/project"

[claude]
auth_forward = "sync"
sync_source_dir = "{claude}"

[codex]
auth_forward = "sync"
sync_source_dir = "{codex}"

[amp]
auth_forward = "sync"
sync_source_dir = "{amp}"

[roles.reviewer]
{grok}"#,
            version = jackin_config::CURRENT_WORKSPACE_VERSION,
        ),
    )
    .unwrap();
}

#[test]
fn disc_scope_global_workspace_role_matrix_deduplicates_profile_roots() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config_root = temp.path().join("config");
    let workspaces = config_root.join("workspaces");
    std::fs::create_dir_all(&workspaces).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"

[claude]
auth_forward = "sync"

[codex]
auth_forward = "sync"

[amp]
auth_forward = "sync"

[roles.reviewer]
git = "https://example.invalid/reviewer.git"
trusted = true
"#,
            jackin_config::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();
    write_workspace(
        &workspaces.join("scentbird.toml"),
        "/profiles/claude-scentbird",
        "/profiles/codex-shared",
        "/profiles/amp-shared",
        false,
    );
    write_workspace(
        &workspaces.join("scentbird-ai.toml"),
        "/profiles/claude-scentbird-ai",
        "/profiles/codex-shared",
        "/profiles/amp-shared",
        true,
    );
    let resolver = FakeEnvResolver::default();

    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &resolver,
    )
    .unwrap();

    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    let count = |surface: &str, kind: UsageCredentialKind| {
        catalog
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.surface_id == surface && candidate.credential_kind == kind
            })
            .count()
    };
    assert_eq!(count("claude", UsageCredentialKind::Profile), 3);
    assert_eq!(count("codex", UsageCredentialKind::Profile), 2);
    assert_eq!(count("amp", UsageCredentialKind::Profile), 2);
    assert_eq!(count("kimi", UsageCredentialKind::Profile), 1);
    assert_eq!(count("grok", UsageCredentialKind::Profile), 1);
    assert_eq!(count("zai", UsageCredentialKind::ApiKey), 1);
    assert_eq!(count("minimax", UsageCredentialKind::ApiKey), 1);

    let codex_shared = catalog
        .candidates
        .iter()
        .find(|candidate| {
            candidate.surface_id == "codex"
                && candidate
                    .provenance
                    .iter()
                    .any(|value| value == "workspace scentbird")
        })
        .unwrap();
    assert!(
        codex_shared
            .provenance
            .iter()
            .any(|value| value == "workspace scentbird-ai")
    );
    let calls = resolver.calls.lock().unwrap();
    assert!(calls.iter().any(|(workspace, role, _)| {
        workspace.as_deref() == Some("scentbird") && role.as_deref() == Some("reviewer")
    }));
    assert!(calls.iter().all(|(_, _, keys)| {
        !keys.iter().any(|key| key == "CONTEXT7_API_KEY")
            && !keys.iter().any(|key| key == "OPENCODE_API_KEY")
    }));
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

fn write_codex_only_global(config_root: &Path, codex_root: &Path) {
    std::fs::create_dir_all(config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"

[claude]
auth_forward = "ignore"

[codex]
auth_forward = "sync"
sync_source_dir = "{}"

[amp]
auth_forward = "ignore"

[kimi]
auth_forward = "ignore"

[grok]
auth_forward = "ignore"
"#,
            jackin_config::CURRENT_CONFIG_VERSION,
            codex_root.display()
        ),
    )
    .unwrap();
}

fn write_codex_workspace(path: &Path, root: &Path) {
    std::fs::write(
        path,
        format!(
            r#"version = "{}"
workdir = "/workspace/project"

[[mounts]]
src = "/host/project"
dst = "/workspace/project"

[codex]
auth_forward = "sync"
sync_source_dir = "{}"
"#,
            jackin_config::CURRENT_WORKSPACE_VERSION,
            root.display()
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
fn disc_source_kimi_profile_requires_no_auth_file() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let kimi_root = temp.path().join("kimi-profile");
    std::fs::create_dir_all(&kimi_root).unwrap();
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"
[claude]
auth_forward = "ignore"
[codex]
auth_forward = "ignore"
[amp]
auth_forward = "ignore"
[kimi]
auth_forward = "sync"
sync_source_dir = "{}"
[grok]
auth_forward = "ignore"
"#,
            jackin_config::CURRENT_CONFIG_VERSION,
            kimi_root.display()
        ),
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
            .any(|scope| scope == "default host profile")
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
