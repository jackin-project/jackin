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
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] ignoring invalid {}: {error:#}",
                jackin_protocol::CAPSULE_CONFIG_PATH
            ));
            return None;
        }
    };
    if let Err(error) = validate(&config) {
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
    Ok(())
}
