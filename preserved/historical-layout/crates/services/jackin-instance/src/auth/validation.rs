// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential-source shape validation for locked and unlocked sources.

use crate::SyncSourceValidationError;

use jackin_config::{AiProvider, ProfileSelector};
use jackin_core::Agent;
use std::path::Path;

use crate::auth::{auth_directory, locked_claude_credentials, validate_omp_source_selection};

#[cfg(unix)]
pub(crate) fn validate_locked_sync_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
    source: &auth_directory::LockedSource,
) -> Result<(), SyncSourceValidationError> {
    match agent {
        Agent::Claude => {
            if locked_claude_credentials(source, source_dir, host_home)
                .map_err(|error| {
                    SyncSourceValidationError::new(format!("Claude source rejected: {error:#}"))
                })?
                .is_some()
            {
                Ok(())
            } else {
                Err(SyncSourceValidationError::new(format!(
                    "Not a Claude config folder: {} has no .credentials.json and no matching \
                     macOS Keychain login. Select the folder you set as CLAUDE_CONFIG_DIR when \
                     you logged in to Claude.",
                    source_dir.display()
                )))
            }
        }
        Agent::Codex => validate_locked_credential_file(source, "auth.json", "Codex"),
        Agent::Grok => validate_locked_credential_file(source, "auth.json", "Grok"),
        Agent::Opencode => validate_locked_opencode(source, provider),
        Agent::Antigravity => {
            validate_locked_credential_file(source, "settings.json", "Antigravity")
        }
        Agent::Gemini => validate_locked_credential_file(source, "oauth_creds.json", "Gemini"),
        Agent::Cursor => validate_locked_credential_file(source, "auth.json", "Cursor"),
        Agent::Muse => validate_locked_credential_file(source, "auth.json", "Muse"),
        Agent::Omp | Agent::Hermes => validate_locked_store_source_dir(
            agent, provider, selector, source_dir, source, host_home,
        ),
        Agent::Amp => validate_locked_credential_file(source, "secrets.json", "Amp"),
        Agent::Kimi => {
            let config = auth_directory::read_locked_source_file(
                &source.root,
                &["config.toml"],
                "Kimi config.toml",
            )
            .map_err(|error| {
                SyncSourceValidationError::new(format!("Kimi source rejected: {error:#}"))
            })?;
            let credentials = auth_directory::validate_locked_source_directory(
                &source.root,
                &["credentials"],
                "Kimi credentials",
            )
            .map_err(|error| {
                SyncSourceValidationError::new(format!("Kimi source rejected: {error:#}"))
            })?;
            if config.is_some() && credentials {
                Ok(())
            } else {
                Err(SyncSourceValidationError::new(format!(
                    "Not a Kimi config folder: {} must contain config.toml and a credentials/ \
                     directory.",
                    source_dir.display()
                )))
            }
        }
    }
}

#[cfg(unix)]
pub(crate) fn validate_locked_credential_file(
    source: &auth_directory::LockedSource,
    name: &str,
    agent: &str,
) -> Result<(), SyncSourceValidationError> {
    let content =
        auth_directory::read_locked_source_file(&source.root, &[name], &format!("{agent} {name}"))
            .map_err(|error| {
                SyncSourceValidationError::new(format!("{agent} source rejected: {error:#}"))
            })?;
    match content {
        Some(content) => {
            let text = std::str::from_utf8(&content).map_err(|error| {
                SyncSourceValidationError::new(format!(
                    "{agent} credential {name} is not valid UTF-8: {error}"
                ))
            })?;
            if text.trim().is_empty() {
                Err(SyncSourceValidationError::new(format!(
                    "{agent} credential {name} is empty."
                )))
            } else {
                Ok(())
            }
        }
        None => Err(SyncSourceValidationError::new(format!(
            "Not a {agent} config folder: expected {name} directly inside the source directory."
        ))),
    }
}

#[cfg(unix)]
pub(crate) fn validate_locked_opencode(
    source: &auth_directory::LockedSource,
    provider: Option<AiProvider>,
) -> Result<(), SyncSourceValidationError> {
    let content =
        auth_directory::read_locked_source_file(&source.root, &["auth.json"], "OpenCode auth.json")
            .map_err(|error| {
                SyncSourceValidationError::new(format!("OpenCode source rejected: {error:#}"))
            })?;
    let Some(content) = content else {
        return Err(SyncSourceValidationError::new(
            "Not an OpenCode config folder: expected auth.json directly inside the source directory.",
        ));
    };
    if content.iter().all(u8::is_ascii_whitespace) {
        return Err(SyncSourceValidationError::new(
            "OpenCode credential auth.json is empty.",
        ));
    }
    let value = serde_json::from_slice::<serde_json::Value>(&content).map_err(|_| {
        SyncSourceValidationError::new(
            "OpenCode auth.json is malformed; no credentials were selected.",
        )
    })?;
    select_opencode_auth_entry(&value, provider)
        .map(|_| ())
        .map_err(|reason| {
            SyncSourceValidationError::new(format!(
                "OpenCode auth.json cannot be selected safely: {reason}."
            ))
        })
}

#[cfg(unix)]
pub(crate) fn validate_locked_store_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    source: &auth_directory::LockedSource,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    if agent == Agent::Omp {
        return validate_omp_source_selection(&source.root, provider, selector).map_err(|error| {
            SyncSourceValidationError::new(format!("OMP source snapshot failed: {error:#}"))
        });
    }
    // The source lock remains held while discovery reads the descriptor. The
    // launch admission path performs the stronger protected-root snapshot and
    // revalidates its bytes before any worker starts.
    validate_store_source_dir(agent, provider, selector, source_dir, host_home)
}

/// Validate a stores-backed source through the same single-entry discovery
/// boundary used for account registration. This keeps a source that changes
/// after scan from silently selecting a different account at launch.
pub(crate) fn validate_store_source_dir(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    source_dir: &Path,
    host_home: &Path,
) -> Result<(), SyncSourceValidationError> {
    if agent == Agent::Hermes {
        validate_hermes_source_shape(source_dir)?;
    }
    let found = jackin_config::discover_account_directory(agent, source_dir, host_home)
        .map_err(|error| {
            SyncSourceValidationError::new(format!("{agent} source rejected: {error}"))
        })?
        .ok_or_else(|| {
            SyncSourceValidationError::new(format!(
                "{agent} source has no usable single-account credential store"
            ))
        })?;
    if provider.is_some_and(|expected| found.provider != Some(expected)) {
        return Err(SyncSourceValidationError::new(format!(
            "{agent} source provider no longer matches the selected account"
        )));
    }
    if selector.is_some_and(|expected| found.source_selector.as_ref() != Some(expected)) {
        return Err(SyncSourceValidationError::new(format!(
            "{agent} source entry/profile no longer matches the selected account"
        )));
    }
    Ok(())
}

pub(crate) fn validate_hermes_source_shape(
    source_dir: &Path,
) -> Result<(), SyncSourceValidationError> {
    for name in ["config.yaml", ".env", "auth.json"] {
        let path = source_dir.join(name);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            return Err(SyncSourceValidationError::new(format!(
                "Hermes source file {} is a symlink; refusing to follow it.",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(SyncSourceValidationError::new(format!(
                "Hermes source file {} is a special or non-regular file.",
                path.display()
            )));
        }
    }
    let profiles = source_dir.join("profiles");
    if let Ok(metadata) = std::fs::symlink_metadata(&profiles)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err(SyncSourceValidationError::new(format!(
            "Hermes source profiles {} is not a real directory.",
            profiles.display()
        )));
    }
    Ok(())
}

/// Validate the exact `OpenCode` credential that will be staged. A single
/// usable `auth.json` entry is the source-bound materialization. A sibling
/// database may coexist in a normal `OpenCode` data directory, but database-only
/// profiles fail because there is no launchable source to stage.
#[cfg(not(unix))]
pub(crate) fn validate_opencode_source_dir(
    source_dir: &Path,
    provider: Option<AiProvider>,
) -> Result<(), SyncSourceValidationError> {
    let auth_path = source_dir.join("auth.json");
    let content = read_source_text(&auth_path, "OpenCode auth.json")
        .map_err(|_| {
            SyncSourceValidationError::new(format!(
                "Not an OpenCode config folder: expected auth.json directly inside {}.",
                source_dir.display()
            ))
        })?
        .ok_or_else(|| {
            SyncSourceValidationError::new(format!(
                "Not an OpenCode config folder: expected auth.json directly inside {}.",
                source_dir.display()
            ))
        })?;
    if content.trim().is_empty() {
        return Err(SyncSourceValidationError::new(format!(
            "OpenCode credential auth.json in {} is empty.",
            source_dir.display()
        )));
    }
    let value = serde_json::from_str::<serde_json::Value>(&content).map_err(|_| {
        SyncSourceValidationError::new(
            "OpenCode auth.json is malformed; no credentials were selected.",
        )
    })?;
    select_opencode_auth_entry(&value, provider)
        .map(|_| ())
        .map_err(|reason| {
            SyncSourceValidationError::new(format!(
                "OpenCode auth.json cannot be selected safely: {reason}."
            ))
        })
}

/// Return the one provider entry that may cross the role-state boundary.
/// Values remain borrowed so validation does not copy secrets. Multi-entry
/// files are rejected even when one entry could be filtered: the persisted
/// account model has no raw store-key field, so filtering would still permit
/// same-directory identities to collapse during later scans.
pub(crate) fn select_opencode_auth_entry(
    value: &serde_json::Value,
    provider: Option<AiProvider>,
) -> Result<(&str, &serde_json::Value), &'static str> {
    let entries = value
        .as_object()
        .ok_or("the top-level value is not an object")?;
    if entries.len() != 1 {
        return Err("multiple provider entries are unsupported");
    }
    let Some((entry_key, entry)) = entries.iter().next() else {
        return Err("no provider credential exists");
    };
    if let Some(provider) = provider {
        let key = opencode_provider_key(provider)?;
        if entry_key != key {
            return Err("the selected provider credential is missing");
        }
    } else if entry_key != "opencode-go" {
        return Err(
            "source-bound OpenCode profiles currently support only the opencode-go auth entry",
        );
    }
    if !usable_opencode_auth_entry(entry) {
        return Err("the selected provider credential is empty or unsupported");
    }
    Ok((entry_key.as_str(), entry))
}

pub(crate) fn opencode_provider_key(provider: AiProvider) -> Result<&'static str, &'static str> {
    if provider == AiProvider::Opencode {
        Ok("opencode-go")
    } else {
        Err("source-bound OpenCode profiles currently support only the opencode-go auth entry")
    }
}

pub(crate) fn usable_opencode_auth_entry(entry: &serde_json::Value) -> bool {
    let Some(kind) = entry.get("type").and_then(serde_json::Value::as_str) else {
        return false;
    };
    match kind {
        "api" => entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|key| !key.trim().is_empty()),
        "oauth" => ["access", "refresh"].into_iter().any(|field| {
            entry
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|token| !token.trim().is_empty())
        }),
        _ => false,
    }
}
