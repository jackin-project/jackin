// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Staged account credentials loading and env allowlists.

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use std::collections::BTreeSet;

/// Load protected account data without including file contents in diagnostics.
pub(crate) fn load_agent_credentials(
    config: &CapsuleConfig,
) -> std::io::Result<jackin_protocol::AgentCredentialEnv> {
    let mut instances = std::collections::BTreeMap::new();
    for (instance, path) in &config.instance_credential_files {
        let expected = jackin_protocol::account_credentials_container_path(instance);
        if path != &expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credential path is outside the admitted mount",
            ));
        }
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let staged = parse_staged_credential(&raw)?;
        if staged.instance != *instance {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credential file does not match its admitted instance",
            ));
        }
        if instances
            .insert(instance.clone(), staged.credential)
            .is_some()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "duplicate protected account credential instance",
            ));
        }
    }
    let credentials = jackin_protocol::AgentCredentialEnv::new(instances);
    validate_agent_credentials(config, &credentials)?;
    Ok(credentials)
}

/// Decode one staged protected-credentials file. Anything else (a legacy
/// container-wide envelope, missing version, or malformed JSON) is an
/// explicit restart/upgrade error, never a silent misread. Diagnostics never
/// carry file contents.
pub(crate) fn parse_staged_credential(
    raw: &[u8],
) -> std::io::Result<jackin_protocol::StagedInstanceCredential> {
    let credential: jackin_protocol::StagedInstanceCredential = serde_json::from_slice(raw)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid protected account credentials: expected a single-instance \
                 staged credential file; restart the container \
                 from an upgraded host",
            )
        })?;
    if credential.schema_version != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported protected account credentials schema: expected the single-instance \
             staged credential schema; restart the container \
             from an upgraded host",
        ));
    }
    Ok(credential)
}

pub(crate) const ANTHROPIC_BASE_URL_ENV_NAME: &str = "ANTHROPIC_BASE_URL";
pub(crate) const ANTHROPIC_DEFAULT_OPUS_MODEL_ENV_NAME: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL";
pub(crate) const ANTHROPIC_DEFAULT_SONNET_MODEL_ENV_NAME: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL";
pub(crate) const ANTHROPIC_DEFAULT_HAIKU_MODEL_ENV_NAME: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL";
pub(crate) const OPENAI_BASE_URL_ENV_NAME: &str = "OPENAI_BASE_URL";
pub(crate) const KIMI_BASE_URL_ENV_NAME: &str = "KIMI_BASE_URL";

/// Return the exact account-owned environment names that one admitted agent
/// may receive. `provider_surface` is the selected account's credential
/// routing surface. The caller must prove the surface before asking for an
/// allowlist; unsupported provider variables cannot cross an agent boundary.
pub(crate) fn allowed_account_env_names(
    agent_slug: &str,
    auth_mode: &str,
    provider_surface: &str,
) -> Result<BTreeSet<&'static str>> {
    let agent = jackin_core::Agent::from_slug(agent_slug)
        .ok_or_else(|| anyhow::anyhow!("unknown agent runtime {agent_slug:?}"))?;
    if !matches!(
        provider_surface,
        "claude"
            | "codex"
            | "amp"
            | "grok"
            | "zai"
            | "kimi"
            | "minimax"
            | "opencode"
            | "google"
            | "cursor"
            | "meta"
            | "openrouter"
    ) {
        anyhow::bail!("unknown provider surface {provider_surface:?}");
    }

    let mut allowed = BTreeSet::new();
    match auth_mode {
        "sync" | "ignore" => return Ok(allowed),
        "oauth_token" => {
            if agent != jackin_core::Agent::Claude || provider_surface != "claude" {
                return Ok(allowed);
            }
            allowed.insert(jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME);
            // An explicit Claude endpoint override is part of the OAuth
            // account contract and is still scoped to the selected pane.
            allowed.insert(ANTHROPIC_BASE_URL_ENV_NAME);
            return Ok(allowed);
        }
        "api_key" => {}
        _ => anyhow::bail!("invalid auth mode {auth_mode:?}"),
    }

    match agent {
        jackin_core::Agent::Claude => {
            allowed.extend([
                jackin_core::CLAUDE_MODEL_ENV_NAME,
                ANTHROPIC_DEFAULT_OPUS_MODEL_ENV_NAME,
                ANTHROPIC_DEFAULT_SONNET_MODEL_ENV_NAME,
                ANTHROPIC_DEFAULT_HAIKU_MODEL_ENV_NAME,
            ]);
            match provider_surface {
                "claude" => {
                    allowed.insert(jackin_core::ANTHROPIC_API_KEY_ENV_NAME);
                    allowed.insert(ANTHROPIC_BASE_URL_ENV_NAME);
                }
                "kimi" | "zai" | "minimax" => {
                    allowed.insert(jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME);
                    allowed.insert(ANTHROPIC_BASE_URL_ENV_NAME);
                }
                _ => return Ok(BTreeSet::new()),
            }
        }
        jackin_core::Agent::Codex => match provider_surface {
            "codex" | "zai" => {
                allowed.insert(jackin_core::OPENAI_API_KEY_ENV_NAME);
                allowed.insert(OPENAI_BASE_URL_ENV_NAME);
            }
            "kimi" => {
                allowed.insert(jackin_core::KIMI_API_KEY_ENV_NAME);
                allowed.insert(OPENAI_BASE_URL_ENV_NAME);
            }
            "minimax" => {
                allowed.insert(jackin_core::MINIMAX_API_KEY_ENV_NAME);
                allowed.insert(OPENAI_BASE_URL_ENV_NAME);
            }
            _ => return Ok(BTreeSet::new()),
        },
        jackin_core::Agent::Opencode | jackin_core::Agent::Omp | jackin_core::Agent::Hermes => {
            if let Some(name) = multi_provider_key(provider_surface) {
                allowed.insert(name);
            }
        }
        jackin_core::Agent::Amp => insert_native_key(
            &mut allowed,
            provider_surface,
            "amp",
            jackin_core::AMP_API_KEY_ENV_NAME,
        ),
        jackin_core::Agent::Kimi => insert_native_key(
            &mut allowed,
            provider_surface,
            "kimi",
            jackin_core::KIMI_API_KEY_ENV_NAME,
        ),
        jackin_core::Agent::Grok => insert_native_key(
            &mut allowed,
            provider_surface,
            "grok",
            jackin_core::XAI_API_KEY_ENV_NAME,
        ),
        jackin_core::Agent::Antigravity | jackin_core::Agent::Gemini => insert_native_key(
            &mut allowed,
            provider_surface,
            "google",
            jackin_core::GEMINI_API_KEY_ENV_NAME,
        ),
        jackin_core::Agent::Cursor => insert_native_key(
            &mut allowed,
            provider_surface,
            "cursor",
            jackin_core::CURSOR_API_KEY_ENV_NAME,
        ),
        jackin_core::Agent::Muse => insert_native_key(
            &mut allowed,
            provider_surface,
            "meta",
            jackin_core::META_API_KEY_ENV_NAME,
        ),
    }
    if agent == jackin_core::Agent::Kimi {
        allowed.insert(KIMI_BASE_URL_ENV_NAME);
    }
    Ok(allowed)
}

pub(crate) fn multi_provider_key(surface: &str) -> Option<&'static str> {
    match surface {
        "claude" => Some(jackin_core::ANTHROPIC_API_KEY_ENV_NAME),
        "codex" => Some(jackin_core::OPENAI_API_KEY_ENV_NAME),
        "grok" => Some(jackin_core::XAI_API_KEY_ENV_NAME),
        "kimi" => Some(jackin_core::MOONSHOT_API_KEY_ENV_NAME),
        "zai" => Some(jackin_core::ZHIPU_API_KEY_ENV_NAME),
        "minimax" => Some(jackin_core::MINIMAX_API_KEY_ENV_NAME),
        "google" => Some(jackin_core::GEMINI_API_KEY_ENV_NAME),
        "cursor" => Some(jackin_core::CURSOR_API_KEY_ENV_NAME),
        "meta" => Some(jackin_core::META_API_KEY_ENV_NAME),
        "openrouter" => Some(jackin_core::OPENROUTER_API_KEY_ENV_NAME),
        "opencode" => Some(jackin_core::OPENCODE_API_KEY_ENV_NAME),
        "amp" => None,
        _ => None,
    }
}

pub(crate) fn insert_native_key(
    allowed: &mut BTreeSet<&'static str>,
    provider_surface: &str,
    expected_surface: &str,
    key: &'static str,
) {
    if provider_surface == expected_surface {
        allowed.insert(key);
    }
}

pub(crate) fn is_protected_credential_name(name: &str) -> bool {
    name == jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
        || jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
            .iter()
            .any(|entry| entry.name == name)
}

pub(crate) fn validate_agent_credentials(
    config: &CapsuleConfig,
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> std::io::Result<()> {
    for instance in &config.instances {
        if config.agent_for_instance(instance).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "launch config instance has no agent runtime",
            ));
        }
        if matches!(
            config.auth_mode_for_instance(instance),
            Some("api_key" | "oauth_token")
        ) {
            if config
                .credential_provider_surface_for_instance(instance)
                .is_none()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing credential provider surface for configured account",
                ));
            }
            if credentials
                .for_instance(instance)
                .is_none_or(std::collections::BTreeMap::is_empty)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing protected credentials for configured account",
                ));
            }
        }
    }
    for (instance, entry) in credentials.iter() {
        let Some(expected_agent) = config.agent_for_instance(instance) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials name an instance without an agent runtime",
            ));
        };
        let Some(expected_account) = config.account_for_instance(instance) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials name an instance without an account",
            ));
        };
        let Some(provider_surface) = config.credential_provider_surface_for_instance(instance)
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials have no provider surface",
            ));
        };
        let allowed = allowed_account_env_names(
            expected_agent,
            config.auth_mode_for_instance(instance).unwrap_or_default(),
            provider_surface,
        )
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials have an invalid agent/provider contract",
            )
        })?;
        let credential_key_count = entry
            .env
            .keys()
            .filter(|name| allowed.contains(name.as_str()) && is_protected_credential_name(name))
            .count();
        if !config.instances.contains(instance)
            || !matches!(
                config.auth_mode_for_instance(instance),
                Some("api_key" | "oauth_token")
            )
            || entry.agent != expected_agent
            || entry.account_id != expected_account
            || credential_key_count != 1
            || entry
                .env
                .iter()
                .any(|(name, value)| !allowed.contains(name.as_str()) || value.trim().is_empty())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials violate instance admission",
            ));
        }
    }
    Ok(())
}
