// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule runtime configuration: load and validate `CapsuleConfig` from the
//! TOML file written by the host at container launch.
//!
//! Not responsible for: config schema definition (see `jackin-protocol`) or
//! host-side config serialization.

use anyhow::{Context, Result};
use jackin_protocol::CapsuleConfig;

pub fn load() -> Result<CapsuleConfig> {
    let contents = std::fs::read_to_string(jackin_protocol::CAPSULE_CONFIG_PATH)
        .with_context(|| format!("reading {}", jackin_protocol::CAPSULE_CONFIG_PATH))?;
    let config: CapsuleConfig = toml::from_str(&contents)
        .with_context(|| format!("parsing {}", jackin_protocol::CAPSULE_CONFIG_PATH))?;
    validate(&config)?;
    Ok(config)
}

pub fn load_optional() -> Option<CapsuleConfig> {
    let contents = match std::fs::read_to_string(jackin_protocol::CAPSULE_CONFIG_PATH) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::ConfigError,
            );
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] ignoring unreadable {}: {error:#}",
                jackin_protocol::CAPSULE_CONFIG_PATH
            ));
            return None;
        }
    };
    let config = match toml::from_str::<CapsuleConfig>(&contents) {
        Ok(config) => config,
        Err(error) => {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::ConfigError,
            );
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] ignoring invalid {}: {error:#}",
                jackin_protocol::CAPSULE_CONFIG_PATH
            ));
            return None;
        }
    };
    if let Err(error) = validate(&config) {
        let _error =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
        crate::output::stderr_line(format_args!(
            "[jackin-capsule] ignoring invalid {}: {error:#}",
            jackin_protocol::CAPSULE_CONFIG_PATH
        ));
        return None;
    }
    Some(config)
}

fn validate(config: &CapsuleConfig) -> Result<()> {
    if config.workdir.trim().is_empty() {
        anyhow::bail!("{} workdir is empty", jackin_protocol::CAPSULE_CONFIG_PATH);
    }
    for agent in &config.agents {
        let mode = config.auth_mode_for_agent(agent).ok_or_else(|| {
            anyhow::anyhow!("missing bounded auth mode for configured agent {agent}")
        })?;
        if !matches!(mode, "sync" | "api_key" | "oauth_token" | "ignore") {
            anyhow::bail!("invalid bounded auth mode for configured agent {agent}");
        }
    }
    if config
        .auth_modes
        .keys()
        .any(|agent| !config.agents.contains(agent))
    {
        anyhow::bail!("auth mode names an agent outside the configured allowlist");
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Load protected account data without including file contents in diagnostics.
pub(crate) fn load_agent_credentials(
    config: &CapsuleConfig,
) -> std::io::Result<jackin_protocol::AgentCredentialEnv> {
    let raw = match std::fs::read(jackin_core::container_paths::ACCOUNT_CREDENTIALS) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let credentials = jackin_protocol::AgentCredentialEnv::default();
            validate_agent_credentials(config, &credentials)?;
            return Ok(credentials);
        }
        Err(error) => return Err(error),
    };
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_slice(&raw).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid protected account credentials",
            )
        })?;
    validate_agent_credentials(config, &credentials)?;
    Ok(credentials)
}

fn validate_agent_credentials(
    config: &CapsuleConfig,
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> std::io::Result<()> {
    for agent in &config.agents {
        if matches!(
            config.auth_mode_for_agent(agent),
            Some("api_key" | "oauth_token")
        ) && credentials
            .for_agent(agent)
            .is_none_or(std::collections::BTreeMap::is_empty)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing protected credentials for configured account",
            ));
        }
    }
    for (agent, env) in credentials.iter() {
        if !config.agents.contains(agent)
            || !matches!(
                config.auth_mode_for_agent(agent),
                Some("api_key" | "oauth_token")
            )
            || env
                .iter()
                .any(|(name, value)| !jackin_core::is_account_env(name) || value.trim().is_empty())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials violate agent admission",
            ));
        }
    }
    Ok(())
}
