// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent image prewarm execution.

use super::published::published_image_is_stale;

use jackin_core::Agent;

use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;
#[cfg(not(test))]
use jackin_docker::{ShellRunner, docker_client::BollardDockerClient};

use jackin_image::version_check;

#[cfg(not(test))]
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

use super::{ImageInvalidationReason, RoleImagePrewarmRow, role_git_sha_for_recipe};
#[cfg(test)]
use super::{
    build_image_recipe_with_construct_image, cache_bust_value_for_build, ensure_local_role_base,
    expected_image_recipe_for_test, image_recipe_label_map_for_install_test,
    image_recipe_label_map_for_test, should_mint_fresh_cache_bust,
};
#[cfg(not(test))]
use super::{prewarm_agent_image_from_validated_repo, refresh_agent_image_from_validated_repo};

#[cfg(not(test))]
pub(crate) async fn prewarm_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    agent: Agent,
    debug: bool,
) -> anyhow::Result<RoleImagePrewarmRow> {
    let mut runner = ShellRunner { debug };
    let docker = BollardDockerClient::connect()?;
    let (cached_repo, validated_repo, repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        role_git,
        &mut runner,
        RepoResolveOptions::non_interactive()
            .with_branch(branch_override)
            .with_refresh_ttl(std::time::Duration::ZERO),
        || Ok(false),
    )
    .await?;
    prewarm_agent_image_from_validated_repo(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        branch_override,
        agent,
        &docker,
        &mut runner,
        repo_lock,
        debug,
    )
    .await
}

#[cfg(not(test))]
pub(crate) async fn reuse_staleness_sentinel(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    agent: Agent,
    image: &str,
    debug: bool,
) -> anyhow::Result<Option<RoleImagePrewarmRow>> {
    let mut runner = ShellRunner { debug };
    let docker = BollardDockerClient::connect()?;
    let (cached_repo, validated_repo, repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        role_git,
        &mut runner,
        RepoResolveOptions::non_interactive()
            .with_branch(branch_override)
            .with_refresh_ttl(std::time::Duration::ZERO),
        || Ok(false),
    )
    .await?;
    let role_git_sha = role_git_sha_for_recipe(&cached_repo, None, &mut runner).await;
    let reason = reuse_staleness_reason(
        paths,
        &validated_repo,
        image,
        role_git_sha.as_deref(),
        &docker,
    )
    .await;

    let Some(reason) = reason else {
        drop(repo_lock);
        return Ok(None);
    };

    let row = refresh_agent_image_from_validated_repo(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        branch_override,
        agent,
        &docker,
        &mut runner,
        repo_lock,
        debug,
        reason,
        role_git_sha.as_deref(),
    )
    .await?;
    Ok(Some(row))
}

#[cfg(not(test))]
pub(crate) async fn reuse_staleness_reason(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    image: &str,
    role_git_sha: Option<&str>,
    docker: &impl DockerApi,
) -> Option<ImageInvalidationReason> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "agent_version_check",
        Some(image),
    );
    let agents = validated_repo.manifest.supported_agents();
    let checks = agents.iter().map(|&agent| async move {
        (
            agent,
            version_check::needs_agent_update(paths, image, agent).await,
        )
    });
    let results = futures_util::future::join_all(checks).await;
    let timing_detail = if results
        .iter()
        .any(|(_, check)| *check == version_check::AgentVersionCheck::Stale)
    {
        "stale"
    } else if results
        .iter()
        .any(|(_, check)| *check == version_check::AgentVersionCheck::Unknown)
    {
        "unknown"
    } else {
        "fresh"
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "agent_version_check",
        Some(timing_detail),
    );

    if results
        .iter()
        .any(|(_, check)| *check == version_check::AgentVersionCheck::Unknown)
    {
        let _warning = jackin_telemetry::record_recovered_degradation();
    }
    if results
        .into_iter()
        .any(|(_, check)| check == version_check::AgentVersionCheck::Stale)
    {
        return Some(ImageInvalidationReason::AgentVersionChanged);
    }

    if let Some(published) = validated_repo.manifest.published_image.as_deref()
        && published_image_is_stale(
            published,
            &validated_repo.dockerfile.construct_version,
            role_git_sha,
            docker,
        )
        .await
    {
        return Some(ImageInvalidationReason::PublishedImageStale);
    }

    None
}
