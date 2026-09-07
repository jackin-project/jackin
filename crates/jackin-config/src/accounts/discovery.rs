// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential discovery reports locations, never credential values.

use std::path::{Path, PathBuf};

use jackin_core::Agent;
use serde_json::Value;

use super::AiProvider;

/// Find provider API-key references in an explicit environment snapshot.
/// Returns variable names only; values never leave this boundary.
pub fn discover_environment_accounts(
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<(AiProvider, String)> {
    [
        (AiProvider::Anthropic, &["ANTHROPIC_API_KEY"][..]),
        (AiProvider::OpenAi, &["OPENAI_API_KEY"][..]),
        (AiProvider::Amp, &["AMP_API_KEY"][..]),
        (AiProvider::Xai, &["XAI_API_KEY"][..]),
        (AiProvider::Opencode, &["OPENCODE_API_KEY"][..]),
        (
            AiProvider::Moonshot,
            &["KIMI_API_KEY", "KIMI_CODE_API_KEY", "MOONSHOT_API_KEY"][..],
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
    ]
    .into_iter()
    .filter_map(|(provider, names)| {
        // One account per provider: prefer the canonical name, then aliases.
        // Bootstrap keys accounts by provider, so returning aliases separately
        // would silently replace the preferred credential reference.
        names.iter().find_map(|name| {
            environment
                .get(*name)
                .filter(|value| !value.trim().is_empty())
                .map(|_| (provider, (*name).to_owned()))
        })
    })
    .collect()
}

/// Discover supported subscription-token references without copying their values.
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

/// Evidence backing a discovered account. Discovery does not verify expiry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialEvidence {
    /// A recognized credential field in this file is nonempty.
    File(PathBuf),
    /// An exact Claude Keychain service exists; its secret was not read.
    Keychain(String),
}

/// A usable source location to import into the account registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAccount {
    /// Agent that owns the credential format.
    pub agent: Agent,
    /// Selected source directory to store in the account registry.
    pub directory: PathBuf,
    /// Credential location which established this discovery.
    pub evidence: CredentialEvidence,
}

/// Stable, secret-free discovery failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    /// Source cannot be read as a regular file.
    #[error("credential source cannot be read")]
    Unreadable,
    /// Source contains invalid JSON.
    #[error("credential file is not valid JSON")]
    Malformed,
    /// Source exceeds the bounded credential read size.
    #[error("credential file exceeds the discovery size limit")]
    TooLarge,
}

/// One failed source; other agents are still scanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryIssue {
    /// Agent whose source could not be inspected.
    pub agent: Agent,
    /// Source directory whose scan failed.
    pub directory: PathBuf,
    /// Sanitized failure category.
    pub error: DiscoveryError,
}

/// Results from scanning every supported agent's default directory.
#[derive(Debug, Default)]
pub struct DiscoveryReport {
    /// Sources with credential evidence.
    pub accounts: Vec<DiscoveredAccount>,
    /// Failures encountered while continuing other scans.
    pub issues: Vec<DiscoveryIssue>,
}

/// Scan catalog defaults first, independent of shell config-directory overrides.
/// This performs blocking filesystem/Keychain work; UI callers must use a worker.
pub fn discover_default_accounts(home: &Path) -> DiscoveryReport {
    let mut report = DiscoveryReport::default();
    for &agent in Agent::ALL {
        let primary = home.join(agent.runtime().state_paths().credential_dir);
        let fallback = (agent == Agent::Kimi).then(|| home.join(".kimi"));
        for directory in std::iter::once(primary).chain(fallback) {
            match discover_account_directory(agent, &directory, home) {
                Ok(Some(account)) => {
                    report.accounts.push(account);
                    break;
                }
                Ok(None) => {}
                Err(error) => report.issues.push(DiscoveryIssue {
                    agent,
                    directory,
                    error,
                }),
            }
        }
    }
    report
}

/// Inspect a selected config/credential directory, without reading shell files.
/// Empty folders and metadata-only files do not count as accounts.
/// Performs blocking I/O and must run off render/runtime threads.
///
/// # Errors
/// Returns a sanitized category for unreadable, oversized, or malformed files.
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
    let mut file = directory.join(match agent {
        Agent::Claude => ".credentials.json",
        Agent::Codex | Agent::Opencode | Agent::Grok => "auth.json",
        Agent::Amp => "secrets.json",
        Agent::Kimi => "credentials/kimi-code.json",
    });
    // Alias-style Amp accounts set both XDG roots beneath one selected folder.
    if agent == Agent::Amp && !file.exists() {
        let nested = directory.join("data/amp/secrets.json");
        if nested.exists() {
            file = nested;
        }
    }
    let file_result = read_credentials(&file);
    if let Ok(Some(value)) = &file_result
        && has_credentials(agent, value)
    {
        return Ok(Some(DiscoveredAccount {
            agent,
            directory: directory.to_path_buf(),
            evidence: CredentialEvidence::File(file),
        }));
    }
    // A stale file must not hide a valid login in the exact Keychain scope.
    if agent == Agent::Claude
        && let Some(scope) = jackin_core::claude_keychain_scope(directory, home, home)
        && keychain_exists(&scope.service)
    {
        return Ok(Some(DiscoveredAccount {
            agent,
            directory: scope.normalized_config_dir,
            evidence: CredentialEvidence::Keychain(scope.service),
        }));
    }
    file_result.map(|_| None)
}

fn read_credentials(path: &Path) -> Result<Option<Value>, DiscoveryError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(DiscoveryError::Unreadable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DiscoveryError::Unreadable),
    }
    // The synchronous persistence boundary owns bounded file reads.
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
        Agent::Opencode => value.as_object().is_some_and(|entries| {
            entries
                .values()
                .any(|entry| match entry.get("type").and_then(Value::as_str) {
                    Some("api") => nonempty(entry.get("key")),
                    Some("oauth") => {
                        nonempty(entry.get("access")) || nonempty(entry.get("refresh"))
                    }
                    _ => false,
                })
        }),
        Agent::Grok => value.as_object().is_some_and(|entries| {
            entries.iter().any(|(scope, entry)| {
                (scope.starts_with("https://auth.x.ai::") || scope.contains("/sign-in"))
                    && nonempty(entry.get("key"))
            })
        }),
    }
}

#[cfg(target_os = "macos")]
fn keychain_service_exists(service: &str) -> bool {
    // No -w/-g: query metadata only and discard it, avoiding secret extraction.
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
