// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::{CommandRunner, JackinPaths, RoleSelector};
use jackin_runtime_repo_cache::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

pub async fn resolve_supported_agents_for_console(
    paths: &JackinPaths,
    config: &AppConfig,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<jackin_core::Agent>> {
    let source = config
        .roles
        .get(&selector.key())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown role selector {}", selector.key()))?;
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
