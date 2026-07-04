// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Role manifest re-exports and filesystem helpers.
//!
//! Serde types (`RoleManifest`, etc.) live in `jackin-core`. This module
//! re-exports them and provides I/O helpers (`load_role_manifest`, migration
//! validation) that depend on `toml_edit`, `jackin-config` migrations, and
//! `jackin-core` `env_model`.
//!
//! Not responsible for: filesystem validation (`repo.rs`), or env-var
//! resolution (`operator_env.rs`).

use crate::repo_contract::MANIFEST_FILENAME;
use anyhow::Context;
pub use jackin_core::env_model::{
    JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_ENV_NAME, JACKIN_ENV_VALUE,
};
use std::path::Path;

pub use jackin_core::manifest::{
    AmpConfig, ClaudeConfig, ClaudeMarketplaceConfig, CodexConfig, EnvVarDecl, HookEntry,
    HooksConfig, IdentityConfig, KimiConfig, ManifestDockerConfig, ManifestWarning, OpencodeConfig,
    RoleManifest,
};

/// Load and validate a `jackin.role.toml` from `repo_dir`.
///
/// # Errors
/// Returns an error if the file cannot be read, parsed, or fails version/
/// agent-consistency validation.
pub fn load_role_manifest(repo_dir: &Path) -> anyhow::Result<RoleManifest> {
    let manifest_path = repo_dir.join(MANIFEST_FILENAME);
    let contents = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let doc: toml_edit::DocumentMut = contents
        .parse()
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
    let role_name = display_role_name(repo_dir);
    let manifest_version = crate::migrations::validate_manifest_version(&doc)
        .with_context(|| format!("validating version of {}", manifest_path.display()))?;
    let manifest: RoleManifest = toml::from_str(&contents)
        .with_context(|| format!("parsing {} as RoleManifest", manifest_path.display()))?;
    validate_feature_versions(&manifest, &manifest_version, &role_name)
        .with_context(|| format!("validating version of {}", manifest_path.display()))?;
    let _warnings = crate::validate::validate_agent_consistency(&manifest)?;
    Ok(manifest)
}

fn display_role_name(repo_dir: &Path) -> String {
    let leaf = repo_dir.file_name().and_then(|name| name.to_str());
    let parent = repo_dir
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str());
    match (parent, leaf) {
        (Some(parent), Some("default" | "branches")) => parent.to_owned(),
        (_, Some(name)) => name.to_owned(),
        _ => repo_dir.display().to_string(),
    }
}

fn validate_feature_versions(
    manifest: &RoleManifest,
    manifest_version: &jackin_config::SchemaVersion,
    role_name: &str,
) -> anyhow::Result<()> {
    let v1alpha3 = jackin_config::parse_version("v1alpha3")?;
    let v1alpha4 = jackin_config::parse_version("v1alpha4")?;
    let v1alpha5 = jackin_config::parse_version("v1alpha5")?;
    let v1alpha6 = jackin_config::parse_version("v1alpha6")?;
    if manifest_version < &v1alpha3
        && (manifest
            .agents
            .as_ref()
            .is_some_and(|agents| agents.contains(&jackin_core::Agent::Opencode))
            || manifest.opencode.is_some())
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha3 agent fields, which requires v1alpha3; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    if manifest_version < &v1alpha4
        && (manifest
            .agents
            .as_ref()
            .is_some_and(|agents| agents.contains(&jackin_core::Agent::Kimi))
            || manifest.kimi.is_some())
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha4 agent fields, which requires v1alpha4; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    if manifest_version < &v1alpha5
        && (manifest
            .claude
            .as_ref()
            .is_some_and(|c| !c.providers.is_empty())
            || manifest
                .codex
                .as_ref()
                .is_some_and(|c| !c.providers.is_empty())
            || manifest
                .opencode
                .as_ref()
                .is_some_and(|c| !c.providers.is_empty()))
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha5 per-provider model overrides ([<agent>.providers]), which requires v1alpha5; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    if manifest_version < &v1alpha6 && manifest.docker.is_some() {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha6 docker fields, which requires v1alpha6; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
