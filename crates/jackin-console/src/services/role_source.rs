// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Role-source resolution helpers for console role loading.

use std::collections::BTreeMap;

use jackin_config::{AppConfig, RoleSource};
use jackin_core::RoleSelector;

/// Resolve a configured role source, or derive the default GitHub source for a
/// bare built-in role name.
///
/// Namespaced selectors are delegated to `AppConfig::resolve_role_source` so
/// config-owned selector rules stay authoritative.
pub fn candidate_role_source(
    config: &AppConfig,
    selector: &RoleSelector,
) -> anyhow::Result<RoleSource> {
    let mut candidate = config.clone();
    match candidate.resolve_role_source(selector) {
        Ok((source, _)) => Ok(source),
        Err(_) if selector.namespace.is_none() => Ok(RoleSource {
            git: format!(
                "https://github.com/jackin-project/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            env: BTreeMap::new(),
        }),
        Err(err) => Err(err),
    }
}

#[derive(Debug)]
pub struct ResolvedRoleInput {
    pub raw: String,
    pub key: String,
    pub selector: RoleSelector,
    pub source: RoleSource,
}

#[derive(Debug)]
pub struct RoleInputResolutionError {
    pub raw: String,
    pub source_url: Option<String>,
    pub error: anyhow::Error,
}

pub fn resolve_role_input_source(
    config: &AppConfig,
    value: &str,
) -> Result<ResolvedRoleInput, RoleInputResolutionError> {
    let raw = value.trim();
    jackin_diagnostics::debug_log!("role", "resolving role loader input: raw={raw:?}");
    let selector = RoleSelector::parse(raw).map_err(|e| {
        jackin_diagnostics::debug_log!("role", "role selector parse failed for {raw:?}: {e}");
        RoleInputResolutionError {
            raw: raw.to_owned(),
            source_url: None,
            error: anyhow::Error::new(e),
        }
    })?;
    jackin_diagnostics::debug_log!("role", "parsed role selector: {selector}");

    let key = selector.key();
    let source = candidate_role_source(config, &selector).map_err(|error| {
        jackin_diagnostics::debug_log!(
            "role",
            "role loader failed for key={key:?} raw={raw:?}: {error:?}"
        );
        let source_url = candidate_role_source(config, &selector)
            .ok()
            .map(|source| source.git);
        RoleInputResolutionError {
            raw: raw.to_owned(),
            source_url,
            error,
        }
    })?;
    jackin_diagnostics::debug_log!(
        "role",
        "resolved candidate role source: key={key:?} git={git:?} trusted={trusted}",
        git = source.git.as_str(),
        trusted = source.trusted
    );
    Ok(ResolvedRoleInput {
        raw: raw.to_owned(),
        key,
        selector,
        source,
    })
}

#[cfg(test)]
mod tests;
