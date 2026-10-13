// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Supported-agent lookup for the console role picker.
//!
//! Lookup-only: the actual launch path uses
//! `AppConfig::resolve_role_source`, which synthesizes + inserts a
//! `RoleSource` for unregistered namespaced selectors. That mutation is
//! for the launch (which persists trust), not for a transient agent-list
//! query that discards the config.
//!
//! Split out of `jackin-runtime` (S7 split 96); the old
//! `jackin_runtime::runtime::launch::resolve_supported_agents_for_console`
//! path keeps working through the hub re-export.

use jackin_config::AppConfig;
use jackin_core::{CommandRunner, JackinPaths, RoleSelector};
use jackin_runtime_repo_cache::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

/// Resolve the agents a role supports, for console display.
///
/// Reads the cached role manifest when one is on disk (a cache hit saves
/// a git round trip per role-row Enter); otherwise resolves the agent
/// repo non-interactively and reports the validated manifest's agents.
/// Emits `RoleRepository` cache Hit/Stale/Miss telemetry either way.
pub async fn resolve_supported_agents_for_console(
    paths: &JackinPaths,
    config: &AppConfig,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<jackin_core::Agent>> {
    // Lookup-only: the actual launch path uses
    // `AppConfig::resolve_role_source` which synthesizes + inserts a
    // RoleSource for unregistered namespaced selectors. That mutation
    // is for the launch (which persists trust), not for a transient
    // agent-list query that discards the config.
    let source = config
        .roles
        .get(&selector.key())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown role selector {}", selector.key()))?;
    // Cached manifest is sufficient because the supported-agent set
    // rarely changes between fetches; the real launch re-fetches and
    // re-validates. Saves a git round trip per role-row Enter.
    let cached = jackin_manifest::repo::CachedRepo::new(paths, selector);
    if cached.repo_dir.join(".git").is_dir() {
        match jackin_manifest::load_role_manifest(&cached.repo_dir) {
            Ok(manifest) => {
                jackin_telemetry::cache::decision(
                    jackin_telemetry::schema::enums::CacheName::RoleRepository,
                    jackin_telemetry::schema::enums::CacheResult::Hit,
                );
                return Ok(manifest.supported_agents());
            }
            Err(_error) => {
                jackin_telemetry::cache::decision(
                    jackin_telemetry::schema::enums::CacheName::RoleRepository,
                    jackin_telemetry::schema::enums::CacheResult::Stale,
                );
                let _warning = jackin_telemetry::record_recovered_degradation();
            }
        }
    } else {
        jackin_telemetry::cache::decision(
            jackin_telemetry::schema::enums::CacheName::RoleRepository,
            jackin_telemetry::schema::enums::CacheResult::Miss,
        );
    }
    let (_, validated_repo, _repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::non_interactive(),
        || Ok(false),
    )
    .await?;
    Ok(validated_repo.manifest.supported_agents())
}
