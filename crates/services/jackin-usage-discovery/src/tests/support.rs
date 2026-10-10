// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) type ResolverCall = (Option<String>, Option<String>, Vec<String>);

#[derive(Default)]
pub(super) struct FakeEnvResolver {
    pub(super) calls: Mutex<Vec<ResolverCall>>,
}

pub(super) struct NoEnvResolver;

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

pub(super) struct ConsentKeychainReader;

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
pub(super) struct RecordingProfileReader {
    pub(super) reads: Mutex<BTreeMap<PathBuf, usize>>,
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
            Ok(bytes) => ProfileReadOutcome::Bytes(zeroize::Zeroizing::new(bytes)),
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

pub(super) struct SyntheticDatabaseOnlyReader;

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

pub(super) fn write_registry(config_root: &Path, entries: &[(&str, Agent, &Path)]) {
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

pub(super) fn write_codex_only_global(config_root: &Path, codex_root: &Path) {
    write_registry(config_root, &[("codex", Agent::Codex, codex_root)]);
}

pub(super) fn write_codex_workspace(path: &Path, root: &Path) {
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

pub(super) fn write_codex_auth(root: &Path, account_id: &str, email_payload: &str, token: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        format!(
            r#"{{"tokens":{{"access_token":"{token}","account_id":"{account_id}","id_token":"e30.{email_payload}.x"}}}}"#
        ),
    )
    .unwrap();
}

pub(super) struct AntigravityGrantReader {
    pub(super) grant: ProfileReadOutcome,
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

pub(super) fn test_binding(
    surface: HostSurfaceId,
    source: ValidatedCredentialSource,
) -> ValidatedCredentialBinding {
    ValidatedCredentialBinding {
        surface,
        identity: None,
        source_id: "source-test".to_owned(),
        capability_id: "cap-test".to_owned(),
        credential_revision: "credential-revision-test".to_owned(),
        provenance: BTreeSet::new(),
        source,
    }
}

#[derive(Default)]
pub(super) struct SecretDedupFakeResolver {
    pub(super) calls: Mutex<Vec<Vec<String>>>,
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

pub(super) fn write_accounts_config(
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

pub(super) fn discover_with(
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
