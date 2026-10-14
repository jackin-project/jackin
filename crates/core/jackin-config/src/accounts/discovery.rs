// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use jackin_core::{Agent, MOONSHOT_API_KEY_ENV_NAME};
use serde_json::Value;

use super::{AiProvider, ProfileSelector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvironmentAccountCandidate {
    pub provider: AiProvider,
    pub variable: String,
    pub base_url: Option<String>,
}

fn endpoint_variables(provider: AiProvider) -> &'static [&'static str] {
    match provider {
        AiProvider::Anthropic => &["ANTHROPIC_BASE_URL"],
        AiProvider::OpenAi => &["OPENAI_BASE_URL", "OPENAI_API_BASE", "OPENAI_API_URL"],
        AiProvider::Amp => &["AMP_URL", "AMP_BASE_URL", "AMP_API_URL"],
        AiProvider::Xai => &["XAI_BASE_URL", "XAI_API_BASE", "XAI_API_URL"],
        AiProvider::Opencode => &["OPENCODE_BASE_URL", "OPENCODE_API_BASE", "OPENCODE_API_URL"],
        AiProvider::Moonshot => &[
            "KIMI_BASE_URL",
            "KIMI_CODE_BASE_URL",
            "MOONSHOT_BASE_URL",
            "MOONSHOT_API_BASE",
            "MOONSHOT_API_URL",
        ],
        AiProvider::Zai => &[
            "ZAI_BASE_URL",
            "Z_AI_BASE_URL",
            "ZHIPU_BASE_URL",
            "ZAI_API_BASE",
            "ZAI_API_URL",
        ],
        AiProvider::Minimax => &["MINIMAX_BASE_URL", "MINIMAX_API_BASE", "MINIMAX_API_URL"],
        AiProvider::Google => &[
            "GEMINI_BASE_URL",
            "GOOGLE_BASE_URL",
            "GEMINI_API_BASE",
            "GEMINI_API_URL",
        ],
        AiProvider::Cursor => &["CURSOR_BASE_URL", "CURSOR_API_BASE", "CURSOR_API_URL"],
        AiProvider::Meta => &["META_BASE_URL", "META_API_BASE", "META_API_URL"],
        AiProvider::OpenRouter => &["OPENROUTER_BASE_URL", "OPENROUTER_API_URL"],
    }
}

fn environment_base_url(
    provider: AiProvider,
    environment: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    endpoint_variables(provider).iter().find_map(|name| {
        environment
            .get(*name)
            .filter(|value| !value.trim().is_empty())
            .cloned()
    })
}

pub(crate) fn discover_environment_account_candidates(
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<EnvironmentAccountCandidate> {
    [
        (AiProvider::Anthropic, &["ANTHROPIC_API_KEY"][..]),
        (AiProvider::OpenAi, &["OPENAI_API_KEY"][..]),
        (AiProvider::Amp, &["AMP_API_KEY"][..]),
        (AiProvider::Xai, &["XAI_API_KEY"][..]),
        (AiProvider::Opencode, &["OPENCODE_API_KEY"][..]),
        (
            AiProvider::Moonshot,
            &[
                "KIMI_API_KEY",
                "KIMI_CODE_API_KEY",
                MOONSHOT_API_KEY_ENV_NAME,
            ][..],
        ),
        (
            AiProvider::Zai,
            &["ZAI_API_KEY", "Z_AI_API_KEY", "ZHIPU_API_KEY"][..],
        ),
        (
            AiProvider::Minimax,
            &[
                "MINIMAX_API_KEY",
                "MINIMAX_CODING_API_KEY",
                "MINIMAX_API_TOKEN",
            ][..],
        ),
        (
            AiProvider::Google,
            &["GEMINI_API_KEY", "GOOGLE_API_KEY"][..],
        ),
        (AiProvider::Cursor, &["CURSOR_API_KEY"][..]),
        (AiProvider::Meta, &["META_API_KEY"][..]),
        (AiProvider::OpenRouter, &["OPENROUTER_API_KEY"][..]),
    ]
    .into_iter()
    .filter_map(|(provider, names)| {
        names.iter().find_map(|name| {
            environment
                .get(*name)
                .filter(|value| !value.trim().is_empty())
                .map(|_| EnvironmentAccountCandidate {
                    provider,
                    variable: (*name).to_owned(),
                    base_url: environment_base_url(provider, environment),
                })
        })
    })
    .collect()
}

pub fn discover_environment_accounts(
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<(AiProvider, String)> {
    discover_environment_account_candidates(environment)
        .into_iter()
        .map(|candidate| (candidate.provider, candidate.variable))
        .collect()
}

pub fn discover_environment_oauth_accounts(
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<(Agent, String)> {
    let name = jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME;
    environment
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map(|_| vec![(Agent::Claude, name.to_owned())])
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialEvidence {
    File(PathBuf),
    Keychain(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAccount {
    pub agent: Agent,
    pub provider: Option<AiProvider>,
    pub source_selector: Option<ProfileSelector>,
    pub directory: PathBuf,
    pub evidence: CredentialEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    #[error("credential source cannot be read")]
    Unreadable,
    #[error("credential source is not parseable")]
    Malformed,
    #[error("credential file exceeds the discovery size limit")]
    TooLarge,
    #[error("credential source uses an unsupported layout: {0}")]
    Unsupported(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryIssue {
    pub agent: Agent,
    pub directory: PathBuf,
    pub error: DiscoveryError,
}

#[derive(Debug, Default)]
pub struct DiscoveryReport {
    pub accounts: Vec<DiscoveredAccount>,
    pub issues: Vec<DiscoveryIssue>,
}

pub fn discover_default_accounts(home: &Path) -> DiscoveryReport {
    let codex_home = std::env::var_os("CODEX_HOME");
    discover_default_accounts_with_codex_home(home, codex_home.as_deref())
}

pub(crate) fn discover_default_accounts_with_codex_home(
    home: &Path,
    codex_home: Option<&OsStr>,
) -> DiscoveryReport {
    let mut report = DiscoveryReport::default();
    let has_explicit_codex_home = codex_home.is_some_and(|value| !value.is_empty());
    let codex_directory = match codex_home_directory(home, codex_home) {
        Ok(directory) => Some(directory),
        Err(error) => {
            report.issues.push(DiscoveryIssue {
                agent: Agent::Codex,
                directory: PathBuf::from("CODEX_HOME"),
                error,
            });
            None
        }
    };
    for &agent in Agent::ALL {
        let primary = if agent == Agent::Codex {
            let Some(directory) = codex_directory.as_ref() else {
                continue;
            };
            directory.clone()
        } else {
            home.join(agent.runtime().state_paths().credential_dir)
        };
        let fallback = (agent == Agent::Kimi).then(|| home.join(".kimi"));
        for directory in std::iter::once(primary).chain(fallback) {
            if agent == Agent::Opencode {
                match inspect_store_accounts(agent, &directory) {
                    Ok(accounts) if !accounts.is_empty() => {
                        report.accounts.extend(accounts);
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => report.issues.push(DiscoveryIssue {
                        agent,
                        directory,
                        error,
                    }),
                }
                continue;
            }
            match discover_account_directory(agent, &directory, home) {
                Ok(Some(account)) => {
                    report.accounts.push(account);
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    report.issues.push(DiscoveryIssue {
                        agent,
                        directory: if agent == Agent::Codex && has_explicit_codex_home {
                            PathBuf::from("CODEX_HOME")
                        } else {
                            directory
                        },
                        error,
                    });
                }
            }
        }
    }
    report
}

fn codex_home_directory(
    home: &Path,
    codex_home: Option<&OsStr>,
) -> Result<PathBuf, DiscoveryError> {
    let Some(value) = codex_home.filter(|value| !value.is_empty()) else {
        return Ok(home.join(Agent::Codex.runtime().state_paths().credential_dir));
    };
    let configured = PathBuf::from(value);
    let directory = if configured.is_absolute() {
        configured
    } else {
        std::env::current_dir()
            .map_err(|_| {
                DiscoveryError::Unsupported("CODEX_HOME cannot be resolved from current directory")
            })?
            .join(configured)
    };
    let metadata = std::fs::metadata(&directory).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            DiscoveryError::Unsupported("CODEX_HOME path does not exist")
        }
        _ => DiscoveryError::Unreadable,
    })?;
    if !metadata.is_dir() {
        return Err(DiscoveryError::Unsupported(
            "CODEX_HOME path is not a directory",
        ));
    }
    Ok(directory)
}

pub fn discover_account_directory(
    agent: Agent,
    directory: &Path,
    home: &Path,
) -> Result<Option<DiscoveredAccount>, DiscoveryError> {
    inspect_directory(agent, directory, home, keychain_service_exists)
}

fn inspect_directory(
    agent: Agent,
    directory: &Path,
    home: &Path,
    keychain_exists: impl FnOnce(&str) -> bool,
) -> Result<Option<DiscoveredAccount>, DiscoveryError> {
    if agent == Agent::Antigravity {
        if keychain_exists("gemini") {
            return Ok(Some(DiscoveredAccount {
                agent,
                provider: AiProvider::for_agent(agent),
                source_selector: None,
                directory: directory.to_path_buf(),
                evidence: CredentialEvidence::Keychain("gemini".to_owned()),
            }));
        }
        return Ok(None);
    }
    if matches!(agent, Agent::Opencode | Agent::Omp | Agent::Hermes) {
        return inspect_store(agent, directory);
    }
    let mut file = directory.join(match agent {
        Agent::Claude => ".credentials.json",
        Agent::Codex | Agent::Grok | Agent::Cursor | Agent::Muse => "auth.json",
        Agent::Amp => "secrets.json",
        Agent::Kimi => "credentials/kimi-code.json",
        Agent::Gemini => "oauth_creds.json",
        Agent::Antigravity | Agent::Opencode | Agent::Omp | Agent::Hermes => {
            unreachable!("handled by early returns above")
        }
    });
    if agent == Agent::Amp && !file.exists() {
        let nested = directory.join("data/amp/secrets.json");
        if nested.exists() {
            file = nested;
        }
    }
    if agent == Agent::Kimi
        && !matches!(&read_credentials(&file), Ok(Some(value)) if has_credentials(agent, value))
        && let Some(live) = newest_kimi_env_credentials(&directory.join("credentials"))
    {
        file = live;
    }
    let file_result = read_credentials(&file);
    if let Ok(Some(value)) = &file_result
        && has_credentials(agent, value)
    {
        return Ok(Some(DiscoveredAccount {
            agent,
            provider: AiProvider::for_agent(agent),
            source_selector: None,
            directory: directory.to_path_buf(),
            evidence: CredentialEvidence::File(file),
        }));
    }
    if agent == Agent::Claude
        && let Some(scope) = jackin_core::claude_keychain_scope(directory, home, home)
        && keychain_exists(&scope.service)
    {
        return Ok(Some(DiscoveredAccount {
            agent,
            provider: AiProvider::for_agent(agent),
            source_selector: None,
            directory: scope.normalized_config_dir,
            evidence: CredentialEvidence::Keychain(scope.service),
        }));
    }
    file_result.map(|_| None)
}

fn inspect_store(
    agent: Agent,
    directory: &Path,
) -> Result<Option<DiscoveredAccount>, DiscoveryError> {
    let mut accounts = inspect_store_accounts(agent, directory)?;
    match accounts.len() {
        0 => Ok(None),
        1 => Ok(accounts.pop()),
        _ => Err(DiscoveryError::Unsupported(match agent {
            Agent::Omp => "omp credential store contains multiple entries",
            Agent::Hermes => "Hermes credential store contains multiple profiles",
            Agent::Opencode => "OpenCode auth store contains multiple entries",
            _ => unreachable!("stores-backed agents only"),
        })),
    }
}

fn inspect_store_accounts(
    agent: Agent,
    directory: &Path,
) -> Result<Vec<DiscoveredAccount>, DiscoveryError> {
    use super::stores::{hermes, omp, opencode};
    if agent == Agent::Opencode {
        opencode::validate_opencode_auth_layout(directory).map_err(map_store_error)?;
    }
    if agent == Agent::Omp {
        let accounts = omp::enumerate_omp_credentials(directory).map_err(map_store_error)?;
        return accounts
            .into_iter()
            .map(|account| {
                let provider = account.entry().parse().map_err(|_| {
                    DiscoveryError::Unsupported(
                        "OMP credential provider is not in jackin's catalog",
                    )
                })?;
                Ok(DiscoveredAccount {
                    agent,
                    provider: Some(provider),
                    source_selector: Some(ProfileSelector {
                        entry: account.entry().to_owned(),
                        profile: Some(account.profile().to_owned()),
                    }),
                    directory: directory.to_path_buf(),
                    evidence: CredentialEvidence::File(directory.join("agent/agent.db")),
                })
            })
            .collect();
    }
    let candidates = match agent {
        Agent::Opencode => opencode::enumerate_opencode_auth(&directory.join("auth.json")),
        Agent::Omp => unreachable!("OMP discovery uses the exact snapshot path above"),
        Agent::Hermes => hermes::enumerate_hermes_store(directory),
        _ => unreachable!("stores-backed agents only"),
    };
    match candidates {
        Ok(candidates) => {
            if agent == Agent::Opencode {
                if candidates.is_empty() && directory.join("opencode.db").is_file() {
                    return Err(DiscoveryError::Unsupported(
                        "OpenCode database credentials require a source-bound auth.json profile",
                    ));
                }
                let mut accounts = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    let provider = opencode_provider(&candidate.provider).ok_or(
                        DiscoveryError::Unsupported(
                            "OpenCode credential provider is not in jackin's catalog",
                        ),
                    )?;
                    accounts.push(DiscoveredAccount {
                        agent,
                        provider: Some(provider),
                        source_selector: None,
                        directory: directory.to_path_buf(),
                        evidence: CredentialEvidence::File(candidate.source),
                    });
                }
                return Ok(accounts);
            }
            let mut accounts = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                let provider = candidate.provider.parse().map_err(|_| {
                    DiscoveryError::Unsupported(
                        "multi-provider store credential provider is not in jackin's catalog",
                    )
                })?;
                let source_selector = Some(ProfileSelector {
                    entry: candidate.provider,
                    profile: candidate.profile,
                });
                accounts.push(DiscoveredAccount {
                    agent,
                    provider: Some(provider),
                    source_selector,
                    directory: directory.to_path_buf(),
                    evidence: CredentialEvidence::File(candidate.source),
                });
            }
            if agent == Agent::Hermes && accounts.len() == 1 {
                hermes::validate_single_profile_store(directory).map_err(map_store_error)?;
            }
            Ok(accounts)
        }
        Err(error) => Err(map_store_error(error)),
    }
}

fn opencode_provider(name: &str) -> Option<AiProvider> {
    if name == "opencode-go" {
        return Some(AiProvider::Opencode);
    }
    name.parse().ok()
}

fn map_store_error(error: super::stores::StoreError) -> DiscoveryError {
    match error {
        super::stores::StoreError::Unreadable => DiscoveryError::Unreadable,
        super::stores::StoreError::TooLarge => DiscoveryError::TooLarge,
        super::stores::StoreError::Malformed => DiscoveryError::Malformed,
        super::stores::StoreError::Unsupported(reason) => DiscoveryError::Unsupported(reason),
    }
}

fn newest_kimi_env_credentials(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let is_env_grant = name.starts_with("kimi-code-env-")
                && entry.path().extension().is_some_and(|ext| ext == "json");
            if !is_env_grant {
                return None;
            }
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((mtime, entry.path()))
        })
        .collect();
    candidates.sort();
    candidates.pop().map(|(_, path)| path)
}

fn read_credentials(path: &Path) -> Result<Option<Value>, DiscoveryError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(DiscoveryError::Unreadable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DiscoveryError::Unreadable),
    }
    const LIMIT: u64 = 1024 * 1024;
    let bytes = match crate::persist::read_bounded_file(path, LIMIT + 1) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DiscoveryError::Unreadable),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(DiscoveryError::TooLarge);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| DiscoveryError::Malformed)
}

fn nonempty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn has_credentials(agent: Agent, value: &Value) -> bool {
    match agent {
        Agent::Claude => value.get("claudeAiOauth").is_some_and(|oauth| {
            nonempty(oauth.get("accessToken")) || nonempty(oauth.get("access_token"))
        }),
        Agent::Codex => {
            nonempty(value.get("OPENAI_API_KEY")) || nonempty(value.pointer("/tokens/access_token"))
        }
        Agent::Amp => value.as_object().is_some_and(|entries| {
            entries
                .iter()
                .any(|(key, value)| key.starts_with("apiKey@") && nonempty(Some(value)))
        }),
        Agent::Kimi => nonempty(value.get("access_token")),
        Agent::Grok => value.as_object().is_some_and(|entries| {
            entries.iter().any(|(scope, entry)| {
                (scope.starts_with("https://auth.x.ai::") || scope.contains("/sign-in"))
                    && nonempty(entry.get("key"))
            })
        }),
        Agent::Antigravity | Agent::Opencode | Agent::Omp | Agent::Hermes => false,
        Agent::Gemini => {
            nonempty(value.get("access_token"))
                || nonempty(value.get("refresh_token"))
                || nonempty(value.get("token"))
        }
        Agent::Cursor => nonempty(value.get("accessToken")) || nonempty(value.get("refreshToken")),
        Agent::Muse => value.pointer("/providers/meta").is_some(),
    }
}

#[cfg(target_os = "macos")]
fn keychain_service_exists(service: &str) -> bool {
    std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "macos"))]
fn keychain_service_exists(_service: &str) -> bool {
    false
}

#[cfg(test)]
mod tests;
