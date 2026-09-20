// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential discovery reports locations, never credential values.

use std::path::{Path, PathBuf};

use jackin_core::Agent;
use serde_json::Value;

use super::{AiProvider, ProfileSelector};

/// An environment API-key source plus its non-secret endpoint override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvironmentAccountCandidate {
    /// Provider selected by the API-key variable.
    pub provider: AiProvider,
    /// Variable holding the credential reference.
    pub variable: String,
    /// Optional provider endpoint from the environment.
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

/// Find provider API-key sources and their endpoint overrides in an explicit
/// environment snapshot. Secret values never leave this boundary.
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
        // GEMINI_API_KEY is the documented Google AI Studio variable;
        // GOOGLE_API_KEY is a widely used alias for the same key.
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
        // One account per provider: prefer the canonical name, then aliases.
        // Bootstrap keys accounts by provider, so returning aliases separately
        // would silently replace the preferred credential reference.
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

/// Find provider API-key references in an explicit environment snapshot.
/// Returns variable names only; values never leave this boundary.
pub fn discover_environment_accounts(
    environment: &std::collections::BTreeMap<String, String>,
) -> Vec<(AiProvider, String)> {
    discover_environment_account_candidates(environment)
        .into_iter()
        .map(|candidate| (candidate.provider, candidate.variable))
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
    /// Provider identity selected from the source store.  Multi-provider
    /// clients must carry this identity into the account registry; the
    /// directory alone is not an account selector.
    pub provider: Option<AiProvider>,
    /// Immutable entry/profile identity for a multi-provider store.
    pub source_selector: Option<ProfileSelector>,
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
    /// Source is present but not parseable in its documented layout.
    #[error("credential source is not parseable")]
    Malformed,
    /// Source exceeds the bounded credential read size.
    #[error("credential file exceeds the discovery size limit")]
    TooLarge,
    /// Source is readable but the selected agent cannot safely provision its
    /// layout without risking unrelated credentials.
    #[error("credential source uses an unsupported layout: {0}")]
    Unsupported(&'static str),
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
    // Antigravity credentials live ONLY in the macOS Keychain singleton
    // (service `gemini`, account `antigravity`); settings.json holds prefs
    // and can never be evidence, so it is not read at all.
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
    // Stores-backed agents enumerate content-verified candidates via the
    // `stores` enumerators (JSON + read-only SQLite, WAL-safe, no writes):
    // OpenCode checks auth.json AND the opencode.db `credential` table, omp
    // checks the agent.db `credentials` table, Hermes checks config.yaml +
    // profiles + auth.json. Candidates carry secrets for import; discovery
    // keeps only the source location and drops the values at this boundary.
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
    // Alias-style Amp accounts set both XDG roots beneath one selected folder.
    if agent == Agent::Amp && !file.exists() {
        let nested = directory.join("data/amp/secrets.json");
        if nested.exists() {
            file = nested;
        }
    }
    // Kimi rotates the live grant into per-environment siblings
    // (`credentials/kimi-code-env-<id>.json`) while the base file keeps a
    // drained placeholder; a stale base file must not hide the live grant.
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
    // A stale file must not hide a valid login in the exact Keychain scope.
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

/// Inspect a stores-backed agent directory via the `stores` enumerators.
///
/// A store is launchable only when exactly one candidate can be attributed to
/// it. Selecting the first candidate from a multi-entry store would register
/// an account whose later full-store mount exposes its siblings.
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

/// Inspect a store and retain every source-bound account candidate.
///
/// `OpenCode` candidates are keyed by the provider entry in `auth.json`.
/// Omp candidates use the provider/account entry plus optional profile label;
/// Hermes candidates use the provider entry plus required profile name. Those
/// exact dimensions are persisted as [`ProfileSelector`] values.
/// Database-only stores are not launchable by the current profile contract, so
/// they are rejected instead of registering candidates with no materializable
/// source. A sibling database is ignored when a single usable `auth.json`
/// entry supplies the source-bound profile.
fn inspect_store_accounts(
    agent: Agent,
    directory: &Path,
) -> Result<Vec<DiscoveredAccount>, DiscoveryError> {
    use super::stores::{hermes, omp, opencode};
    if agent == Agent::Opencode {
        opencode::validate_opencode_auth_layout(directory).map_err(map_store_error)?;
    }
    let candidates = match agent {
        // The database parser remains available for audit fixtures, but its
        // row identity cannot cross the profile boundary. A valid auth.json
        // entry is the only source currently materialized for launch/usage;
        // ignore a sibling database rather than mixing two identity systems.
        Agent::Opencode => opencode::enumerate_opencode_auth(&directory.join("auth.json")),
        Agent::Omp => omp::enumerate_omp_credentials(&directory.join("agent/agent.db")),
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
            if matches!(agent, Agent::Omp | Agent::Hermes) && accounts.len() == 1 {
                use super::stores::{hermes, omp};
                match agent {
                    Agent::Omp => {
                        omp::validate_single_credential_store(directory)
                            .map_err(map_store_error)?;
                    }
                    Agent::Hermes => {
                        hermes::validate_single_profile_store(directory)
                            .map_err(map_store_error)?;
                    }
                    _ => unreachable!("validated stores-backed agent"),
                }
            }
            Ok(accounts)
        }
        Err(error) => Err(map_store_error(error)),
    }
}

/// Map an `OpenCode` store key to the canonical provider catalog. `OpenCode`
/// names its native Go subscription `opencode-go`; every other accepted key
/// uses the catalog slug directly.
fn opencode_provider(name: &str) -> Option<AiProvider> {
    if name == "opencode-go" {
        return Some(AiProvider::Opencode);
    }
    name.parse().ok()
}

/// Map a secret-free [`StoreError`](super::stores::StoreError) to the
/// matching discovery category. Unsupported layouts retain their sanitized
/// reason so Settings can explain why a source was not registered.
fn map_store_error(error: super::stores::StoreError) -> DiscoveryError {
    match error {
        super::stores::StoreError::Unreadable => DiscoveryError::Unreadable,
        super::stores::StoreError::TooLarge => DiscoveryError::TooLarge,
        super::stores::StoreError::Malformed => DiscoveryError::Malformed,
        super::stores::StoreError::Unsupported(reason) => DiscoveryError::Unsupported(reason),
    }
}

// NOTE (S1/stores seam): per-value secret import from store candidates
// awaits a secret accessor on `StoreCandidate` (the field is currently
// private with no reader). Discovery consumes only the source location;
// when the stores lane exposes secrets for import, a
// `discover_store_credentials` API + `account scan` import can be layered
// here without touching the matchers above.

/// Newest Kimi per-environment credential file, if any.
///
/// Bounded directory scan: only `kimi-code-env-*.json` regular files are
/// considered, newest first by mtime (name order breaks ties and covers
/// mtime failures deterministically). Returns `None` when the directory
/// cannot be listed.
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
            // A newer directory with a `.json` suffix is not a credential
            // file.  Keep it out of the mtime ordering so it cannot hide a
            // valid live grant when the selected path is read below.
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
        Agent::Grok => value.as_object().is_some_and(|entries| {
            entries.iter().any(|(scope, entry)| {
                (scope.starts_with("https://auth.x.ai::") || scope.contains("/sign-in"))
                    && nonempty(entry.get("key"))
            })
        }),
        // Unreachable: Antigravity never reads a file; OpenCode, Omp,
        // and Hermes enumerate via the `stores` enumerators instead.
        Agent::Antigravity | Agent::Opencode | Agent::Omp | Agent::Hermes => false,
        // Docs-derived shape, unverified against a live install.
        Agent::Gemini => {
            nonempty(value.get("access_token"))
                || nonempty(value.get("refresh_token"))
                || nonempty(value.get("token"))
        }
        Agent::Cursor => nonempty(value.get("accessToken")) || nonempty(value.get("refreshToken")),
        // Verified shape: {schema_version: 2, providers: {meta: {...}}};
        // the secret itself lives in the Keychain, so the meta entry's
        // presence is the evidence.
        Agent::Muse => value.pointer("/providers/meta").is_some(),
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
