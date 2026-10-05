// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential discovery reports locations, never credential values.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use jackin_core::{Agent, MOONSHOT_API_KEY_ENV_NAME};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
    environment: &BTreeMap<String, String>,
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
    environment: &BTreeMap<String, String>,
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
    environment: &BTreeMap<String, String>,
) -> Vec<(AiProvider, String)> {
    discover_environment_account_candidates(environment)
        .into_iter()
        .map(|candidate| (candidate.provider, candidate.variable))
        .collect()
}

/// Discover supported subscription-token references without copying their values.
pub fn discover_environment_oauth_accounts(
    environment: &BTreeMap<String, String>,
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
    // Keep the root credential authoritative when both locations exist; launch
    // and usage proof use this same selected path.
    if agent == Agent::Amp {
        file = amp_credentials_path(directory);
    }
    if agent == Agent::Kimi {
        let config_path = directory.join("config.toml");
        let config_bytes = read_kimi_config(&config_path)?;
        let env = BTreeMap::new();
        let relative = kimi_runtime_credential_relative_path(
            &config_bytes,
            KIMI_CODE_AUTH_SLOT_CONTRACT_VERSION,
            &env,
        )
        .map_err(|_| DiscoveryError::Unsupported("Kimi runtime auth route is not verified"))?;
        file = directory.join(relative);
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

/// Only Kimi CLI 2.1.1 has a verified route-to-file contract in this build.
/// Image labels carry the exact version; other versions must fail closed.
pub const KIMI_CODE_AUTH_SLOT_CONTRACT_VERSION: &str = "2.1.1";

const DEFAULT_KIMI_CODE_BASE_URL: &str = "https://api.kimi.com/coding/v1";
const DEFAULT_KIMI_CODE_OAUTH_HOST: &str = "https://auth.kimi.com";

/// The route and credential file selected by one verified Kimi CLI contract.
/// Persist this value with credential proofs so host admission and capsule
/// materialization use the same runtime selector.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KimiRuntimeAuthSlot {
    /// Exact CLI release whose selection behavior was verified.
    pub cli_version: String,
    /// Effective OAuth host after CLI defaults and environment overrides.
    pub oauth_host: String,
    /// Effective API base URL after CLI defaults and environment overrides.
    pub base_url: String,
    /// Exact OAuth storage key selected by Kimi 2.1.1.
    pub oauth_key: String,
    /// Credential file selected beneath the profile root.
    pub credential_relative_path: PathBuf,
    /// Digest of the canonical, projected config materialized for this slot.
    pub runtime_config_sha256: String,
}

/// Resolve the exact managed Kimi Code credential path for the verified CLI
/// contract. The returned path is relative to the profile root and includes
/// `credentials/`.
///
/// Kimi CLI 2.1.1 resolves the managed OAuth slot from the effective
/// `(oauthHost, baseUrl)` pair. It uses `credentials/kimi-code.json` for the
/// default pair and a deterministic SHA-256-derived sibling otherwise.
///
/// # Errors
/// Rejects unknown CLI versions, malformed config, unsupported providers, and
/// non-file OAuth storage. No filesystem recency heuristic is used.
pub fn kimi_runtime_auth_slot(
    config_toml: &[u8],
    cli_version: &str,
    environment: &BTreeMap<String, String>,
) -> anyhow::Result<KimiRuntimeAuthSlot> {
    Ok(kimi_runtime_auth_config(config_toml, cli_version, environment)?.0)
}

/// Resolve a Kimi slot and project the profile into the exact safe config
/// materialized into its runtime home. The projection contains one managed
/// OAuth provider and only model aliases bound to that provider.
pub fn kimi_runtime_auth_config(
    config_toml: &[u8],
    cli_version: &str,
    environment: &BTreeMap<String, String>,
) -> anyhow::Result<(KimiRuntimeAuthSlot, Vec<u8>)> {
    anyhow::ensure!(
        cli_version == KIMI_CODE_AUTH_SLOT_CONTRACT_VERSION,
        "Kimi CLI version {cli_version} has no verified OAuth slot contract"
    );
    reject_unproven_kimi_environment(environment)?;

    let config_text = std::str::from_utf8(config_toml).context("Kimi config is not UTF-8")?;
    let config: toml::Value = toml::from_str(config_text).context("Kimi config is malformed")?;
    let root = config
        .as_table()
        .context("Kimi config root must be a table")?;
    let provider = config
        .get("providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get("managed:kimi-code"))
        .and_then(toml::Value::as_table)
        .context("Kimi managed provider config is missing")?;
    anyhow::ensure!(
        provider.get("type").and_then(toml::Value::as_str) == Some("kimi"),
        "Kimi managed provider type is unsupported"
    );
    let oauth = provider
        .get("oauth")
        .and_then(toml::Value::as_table)
        .context("Kimi managed OAuth ref is missing")?;
    anyhow::ensure!(
        oauth.get("storage").and_then(toml::Value::as_str) == Some("file"),
        "Kimi managed OAuth storage must explicitly be file-backed"
    );
    let configured_key = oauth
        .get("key")
        .and_then(toml::Value::as_str)
        .filter(|key| !key.trim().is_empty())
        .context("Kimi managed OAuth key is invalid")?;

    let configured_base_url = optional_normalized_string(provider, "base_url")?;
    let configured_oauth_host = optional_normalized_string(oauth, "oauth_host")?;
    let _configured_default_provider = optional_normalized_string(root, "default_provider")?;
    for name in environment.keys().filter(|name| {
        name.starts_with("KIMI") && (name.ends_with("BASE_URL") || name.ends_with("OAUTH_HOST"))
    }) {
        anyhow::ensure!(
            [
                "KIMI_CODE_BASE_URL",
                "KIMI_CODE_OAUTH_HOST",
                "KIMI_OAUTH_HOST"
            ]
            .contains(&name.as_str()),
            "unsupported Kimi route environment variable {name}"
        );
    }
    let env_base_url = nonempty_route_env(environment, "KIMI_CODE_BASE_URL")?;
    if let Some(base_url) = &env_base_url {
        anyhow::ensure!(
            base_url.trim() == base_url,
            "Kimi base URL override has leading or trailing whitespace"
        );
    }
    let code_oauth_host = nonempty_route_env(environment, "KIMI_CODE_OAUTH_HOST")?;
    let legacy_oauth_host = nonempty_route_env(environment, "KIMI_OAUTH_HOST")?;
    anyhow::ensure!(
        code_oauth_host.is_none()
            || legacy_oauth_host.is_none()
            || code_oauth_host == legacy_oauth_host,
        "conflicting Kimi OAuth host environment variables"
    );
    let env_oauth_host = code_oauth_host.or(legacy_oauth_host);
    let has_environment_override = env_base_url.is_some() || env_oauth_host.is_some();

    let base_url = env_base_url
        .as_deref()
        .or(configured_base_url.as_deref())
        .unwrap_or(DEFAULT_KIMI_CODE_BASE_URL)
        .trim_end_matches('/');
    // 2.1.1 drops a configured OAuth host whenever either recognized runtime
    // endpoint override is present; an OAuth-host-only override still leaves
    // the configured base URL intact.
    let oauth_host = if has_environment_override {
        env_oauth_host
            .as_deref()
            .unwrap_or(DEFAULT_KIMI_CODE_OAUTH_HOST)
    } else {
        configured_oauth_host
            .as_deref()
            .unwrap_or(DEFAULT_KIMI_CODE_OAUTH_HOST)
    };
    let oauth_host = oauth_host.trim().trim_end_matches('/');
    anyhow::ensure!(!base_url.is_empty(), "Kimi base URL is empty");
    anyhow::ensure!(!oauth_host.is_empty(), "Kimi OAuth host is empty");

    let (oauth_key, credential_relative_path) = kimi_oauth_slot_identity(oauth_host, base_url)?;
    anyhow::ensure!(
        configured_key == oauth_key,
        "Kimi managed OAuth key does not match its effective route"
    );
    let (models, default_model) = project_kimi_models(root)?;
    let projected = project_kimi_root(
        root,
        models,
        default_model,
        base_url,
        oauth_host,
        &oauth_key,
    );
    let mut runtime_config =
        toml::to_string(&projected).context("serializing canonical Kimi runtime config")?;
    runtime_config.push('\n');
    let runtime_config_sha256 = hex::encode(Sha256::digest(runtime_config.as_bytes()));

    let slot = KimiRuntimeAuthSlot {
        cli_version: cli_version.to_owned(),
        oauth_host: oauth_host.to_owned(),
        base_url: base_url.to_owned(),
        oauth_key,
        credential_relative_path,
        runtime_config_sha256,
    };
    Ok((slot, runtime_config.into_bytes()))
}

fn reject_unproven_kimi_environment(environment: &BTreeMap<String, String>) -> anyhow::Result<()> {
    const AUTH_ENV_NAMES: &[&str] = &[
        "KIMI_API_KEY",
        "KIMI_BASE_URL",
        "KIMI_CODE_CUSTOM_HEADERS",
        "KIMI_WEB_SEARCH_API_KEY",
        "KIMI_WEB_SEARCH_BASE_URL",
        "KIMI_WEB_FETCH_API_KEY",
        "KIMI_WEB_FETCH_BASE_URL",
        "KIMI_REGISTRY_API_KEY",
        "KIMI_DISABLE_OAUTH_LOCK",
        "KIMI_SECONDARY_MODEL",
        "KIMI_CODE_PLUGIN_MARKETPLACE_URL",
        "KIMI_CODE_PLUGIN_MARKETPLACE_FROM_DEV_SERVER",
        "KIMI_CODE_PASSWORD",
        "KIMI_CODE_REMOTE_CONTROL_RELAY_URL",
    ];
    for name in environment.keys() {
        anyhow::ensure!(
            !name.starts_with("KIMI_MODEL_") && !AUTH_ENV_NAMES.contains(&name.as_str()),
            "Kimi auth or model-routing environment variable {name} is not admitted for profile sync"
        );
    }
    Ok(())
}

fn kimi_oauth_slot_identity(oauth_host: &str, base_url: &str) -> anyhow::Result<(String, PathBuf)> {
    if oauth_host == DEFAULT_KIMI_CODE_OAUTH_HOST && base_url == DEFAULT_KIMI_CODE_BASE_URL {
        return Ok((
            "oauth/kimi-code".to_owned(),
            PathBuf::from("credentials/kimi-code.json"),
        ));
    }
    // Property order is part of Kimi 2.1.1's JSON.stringify hash input.
    let oauth_host_json = serde_json::to_string(oauth_host)?;
    let base_url_json = serde_json::to_string(base_url)?;
    let hash_input = format!("{{\"oauthHost\":{oauth_host_json},\"baseUrl\":{base_url_json}}}");
    let hash = hex::encode(Sha256::digest(hash_input.as_bytes()));
    let hash_prefix = hash
        .get(..16)
        .context("Kimi auth slot hash has an invalid length")?;
    let key = format!("oauth/kimi-code-env-{hash_prefix}");
    Ok((
        key.clone(),
        PathBuf::from(format!("credentials/kimi-code-env-{hash_prefix}.json")),
    ))
}

fn project_kimi_models(
    root: &toml::map::Map<String, toml::Value>,
) -> anyhow::Result<(toml::Value, String)> {
    let source_models = root
        .get("models")
        .and_then(toml::Value::as_table)
        .context("Kimi profile has no model aliases bound to the managed provider")?;
    let mut models = toml::map::Map::new();
    for (alias, value) in source_models {
        let Some(model) = value.as_table() else {
            continue;
        };
        let Some(projected) = project_kimi_model(model)? else {
            continue;
        };
        models.insert(alias.clone(), toml::Value::Table(projected));
    }
    anyhow::ensure!(
        !models.is_empty(),
        "Kimi profile has no safe model aliases bound to the managed provider"
    );

    let configured_default = optional_normalized_string(root, "default_model")?;
    let default_model = configured_default
        .filter(|alias| models.contains_key(alias))
        .or_else(|| models.keys().next().cloned())
        .context("Kimi profile has no admitted default model alias")?;
    Ok((toml::Value::Table(models), default_model))
}

fn project_kimi_model(
    model: &toml::map::Map<String, toml::Value>,
) -> anyhow::Result<Option<toml::map::Map<String, toml::Value>>> {
    // A model-level credential or endpoint takes precedence over provider
    // OAuth in Kimi 2.1.1. Exclude that alias from the runtime projection.
    for name in ["api_key", "oauth", "base_url"] {
        if normalized_field(model, name)?.is_some() {
            return Ok(None);
        }
    }
    for name in ["provider", "provider_id"] {
        if optional_normalized_string(model, name)?
            .is_some_and(|provider| provider != "managed:kimi-code")
        {
            return Ok(None);
        }
    }
    let Some(model_name) = optional_normalized_string(model, "model")? else {
        return Ok(None);
    };
    let Some(max_context_size) = normalized_field(model, "max_context_size")? else {
        return Ok(None);
    };
    if max_context_size.as_integer().is_none_or(|size| size <= 0) {
        return Ok(None);
    }

    let mut projected = toml::map::Map::new();
    projected.insert("provider".to_owned(), "managed:kimi-code".into());
    projected.insert("provider_id".to_owned(), "managed:kimi-code".into());
    projected.insert("model".to_owned(), model_name.into());
    projected.insert("max_context_size".to_owned(), max_context_size.clone());
    for name in [
        "name",
        "aliases",
        "max_input_size",
        "max_output_size",
        "capabilities",
        "display_name",
        "reasoning_key",
        "protocol",
        "adaptive_thinking",
        "beta_api",
        "support_efforts",
        "default_effort",
        "off_effort",
    ] {
        if let Some(value) = normalized_field(model, name)? {
            projected.insert(name.to_owned(), value.clone());
        }
    }
    if let Some(overrides) = normalized_field(model, "overrides")? {
        let Some(overrides) = overrides.as_table() else {
            return Ok(None);
        };
        let mut safe_overrides = toml::map::Map::new();
        for name in [
            "max_context_size",
            "max_input_size",
            "max_output_size",
            "capabilities",
            "display_name",
            "reasoning_key",
            "adaptive_thinking",
            "support_efforts",
            "default_effort",
            "off_effort",
        ] {
            if let Some(value) = normalized_field(overrides, name)? {
                safe_overrides.insert(name.to_owned(), value.clone());
            }
        }
        if !safe_overrides.is_empty() {
            projected.insert("overrides".to_owned(), toml::Value::Table(safe_overrides));
        }
    }
    Ok(Some(projected))
}

fn project_kimi_root(
    source: &toml::map::Map<String, toml::Value>,
    models: toml::Value,
    default_model: String,
    base_url: &str,
    oauth_host: &str,
    oauth_key: &str,
) -> toml::Value {
    let mut root = source.clone();
    for key in [
        "providers",
        "models",
        "default_model",
        "defaultModel",
        "default_provider",
        "defaultProvider",
        "services",
        "secondary_model",
        "secondaryModel",
        "api_key",
        "apiKey",
        "api_key_env",
        "apiKeyEnv",
        "oauth",
        "base_url",
        "baseUrl",
        "custom_headers",
        "customHeaders",
        "env",
        "source",
    ] {
        root.remove(key);
    }

    let mut oauth = toml::map::Map::new();
    oauth.insert("storage".to_owned(), "file".into());
    oauth.insert("key".to_owned(), oauth_key.into());
    oauth.insert("oauth_host".to_owned(), oauth_host.into());
    let mut provider = toml::map::Map::new();
    provider.insert("type".to_owned(), "kimi".into());
    provider.insert("base_url".to_owned(), base_url.into());
    provider.insert("oauth".to_owned(), toml::Value::Table(oauth));
    let mut providers = toml::map::Map::new();
    providers.insert("managed:kimi-code".to_owned(), toml::Value::Table(provider));
    root.insert("providers".to_owned(), toml::Value::Table(providers));
    root.insert("models".to_owned(), models);
    root.insert("default_provider".to_owned(), "managed:kimi-code".into());
    root.insert("default_model".to_owned(), default_model.into());
    toml::Value::Table(root)
}

fn normalized_field<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    snake_name: &str,
) -> anyhow::Result<Option<&'a toml::Value>> {
    let camel_name = snake_to_camel(snake_name);
    if camel_name != snake_name && table.contains_key(snake_name) && table.contains_key(&camel_name)
    {
        anyhow::bail!("Kimi config duplicates {snake_name} as {camel_name}");
    }
    Ok(table.get(snake_name).or_else(|| table.get(&camel_name)))
}

fn optional_normalized_string(
    table: &toml::map::Map<String, toml::Value>,
    snake_name: &str,
) -> anyhow::Result<Option<String>> {
    normalized_field(table, snake_name)?
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("Kimi config field {snake_name} is not a string"))
        })
        .transpose()
}

fn snake_to_camel(snake_name: &str) -> String {
    let mut camel = String::with_capacity(snake_name.len());
    let mut uppercase_next = false;
    for character in snake_name.chars() {
        if character == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            camel.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            camel.push(character);
        }
    }
    camel
}

/// Resolve the exact managed Kimi Code credential path for a verified CLI.
pub fn kimi_runtime_credential_relative_path(
    config_toml: &[u8],
    cli_version: &str,
    environment: &BTreeMap<String, String>,
) -> anyhow::Result<PathBuf> {
    Ok(kimi_runtime_auth_slot(config_toml, cli_version, environment)?.credential_relative_path)
}

fn nonempty_route_env(
    environment: &BTreeMap<String, String>,
    name: &str,
) -> anyhow::Result<Option<String>> {
    environment
        .get(name)
        .map(|value| {
            anyhow::ensure!(
                !value.trim().is_empty(),
                "Kimi route environment variable {name} is empty"
            );
            Ok(value.to_owned())
        })
        .transpose()
}

fn read_kimi_config(path: &Path) -> Result<Vec<u8>, DiscoveryError> {
    const LIMIT: u64 = 1024 * 1024;
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            DiscoveryError::Malformed
        } else {
            DiscoveryError::Unreadable
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DiscoveryError::Unreadable);
    }
    let bytes = crate::persist::read_bounded_file(path, LIMIT + 1)
        .map_err(|_| DiscoveryError::Unreadable)?;
    if bytes.len() as u64 > LIMIT {
        return Err(DiscoveryError::TooLarge);
    }
    Ok(bytes)
}

/// Select the Amp credential file with the same precedence used by discovery,
/// launch capture, and usage identity. A root `secrets.json` wins whenever it
/// exists; the XDG `data/amp` file is the fallback.
#[must_use]
pub fn amp_credentials_path(directory: &Path) -> PathBuf {
    amp_credentials_path_from_presence(
        directory,
        directory.join("secrets.json").exists(),
        directory.join("data/amp/secrets.json").exists(),
    )
}

/// Select the Amp credential path from already observed file-presence facts.
/// This lets protected readers share discovery's precedence without reopening
/// credential contents through an unprotected path.
#[must_use]
pub fn amp_credentials_path_from_presence(
    directory: &Path,
    root_file_exists: bool,
    nested_file_exists: bool,
) -> PathBuf {
    if root_file_exists {
        directory.join("secrets.json")
    } else if nested_file_exists {
        directory.join("data/amp/secrets.json")
    } else {
        directory.join("secrets.json")
    }
}

/// Whether a Kimi credential object contains the access-token field used by
/// discovery to identify a live grant.
#[must_use]
pub fn kimi_credentials_value_has_access_token(value: &Value) -> bool {
    nonempty(value.get("access_token"))
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
        Agent::Amp => jackin_core::amp_profile_credential_payload(value).is_ok(),
        Agent::Kimi => kimi_credentials_value_has_access_token(value),
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
    let child = {
        let _native_spawn = jackin_process_directory::native_spawn_guard();
        std::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", service])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    child
        .and_then(|mut child| child.wait())
        .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "macos"))]
fn keychain_service_exists(_service: &str) -> bool {
    false
}

#[cfg(test)]
mod tests;
