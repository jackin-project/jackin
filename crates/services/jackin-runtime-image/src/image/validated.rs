// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(not(test))]
//! Validated-repo image prewarm and refresh.

use jackin_core::Agent;
use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use jackin_manifest::repo::CachedRepo;

use super::{
    ImageDecision, ImageInvalidationReason, ImagePrewarmStatus, RoleImagePrewarmRow,
    build_agent_image, decide_role_image, prepare_runtime_binaries_for_agents,
};

#[expect(
    clippy::too_many_arguments,
    reason = "Prewarming the agent image needs every caller-supplied input \
              (paths, selector, cached + validated repos, branch override, \
              agent, docker, runner, repo_lock, debug) to flow into the build \
              pipeline; bundling into a config struct would be a parallel pass \
              that requires restructuring the image-build path. Named-arg reads \
              match the per-input propagation idiom."
)]
pub async fn prewarm_agent_image_from_validated_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
    agent: Agent,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: jackin_runtime_repo_cache::repo_cache::RepoLock,
    debug: bool,
) -> anyhow::Result<RoleImagePrewarmRow> {
    let decision = decide_role_image(
        paths,
        selector,
        cached_repo,
        validated_repo,
        false,
        branch_override,
        None,
        docker,
        runner,
    )
    .await?;
    jackin_runtime_launch_plan::launch_plan::emit_prewarm_launch_plan(&prewarm_launch_plan_reason(
        &decision,
    ));
    match decision {
        ImageDecision::Reuse { image, .. } => {
            drop(repo_lock);
            Ok(RoleImagePrewarmRow {
                agent,
                image,
                status: ImagePrewarmStatus::Reused,
            })
        }
        ImageDecision::RefreshInBackground { reason, .. } => {
            refresh_agent_image_from_validated_repo(
                paths,
                selector,
                cached_repo,
                validated_repo,
                branch_override,
                agent,
                docker,
                runner,
                repo_lock,
                debug,
                reason,
                None,
            )
            .await
        }
        ImageDecision::BuildFromPublished {
            reason,
            role_git_sha,
            base_image,
        } => {
            let runtime_binaries =
                prepare_runtime_binaries_for_agents(paths, validated_repo, &[agent], None).await?;
            let image = build_agent_image(
                paths,
                selector,
                cached_repo,
                validated_repo,
                agent,
                runtime_binaries,
                false,
                reason,
                Some(base_image.as_str()),
                debug,
                branch_override,
                docker,
                runner,
                repo_lock,
                role_git_sha.as_deref(),
                None,
            )
            .await?;
            Ok(RoleImagePrewarmRow {
                agent,
                image,
                status: ImagePrewarmStatus::Built,
            })
        }
        ImageDecision::BuildFromWorkspace {
            reason,
            role_git_sha,
        } => {
            let runtime_binaries =
                prepare_runtime_binaries_for_agents(paths, validated_repo, &[agent], None).await?;
            let image = build_agent_image(
                paths,
                selector,
                cached_repo,
                validated_repo,
                agent,
                runtime_binaries,
                false,
                reason,
                None,
                debug,
                branch_override,
                docker,
                runner,
                repo_lock,
                role_git_sha.as_deref(),
                None,
            )
            .await?;
            Ok(RoleImagePrewarmRow {
                agent,
                image,
                status: ImagePrewarmStatus::Built,
            })
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Background refresh needs the full build-agent-image context plus \
              the confirmed staleness reason."
)]
pub async fn refresh_agent_image_from_validated_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
    agent: Agent,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: jackin_runtime_repo_cache::repo_cache::RepoLock,
    debug: bool,
    reason: ImageInvalidationReason,
    role_git_sha: Option<&str>,
) -> anyhow::Result<RoleImagePrewarmRow> {
    jackin_runtime_launch_plan::launch_plan::emit_prewarm_launch_plan(&format!(
        "image_refresh:{}",
        reason.as_str()
    ));
    let runtime_binaries =
        prepare_runtime_binaries_for_agents(paths, validated_repo, &[agent], None).await?;
    let image = build_agent_image(
        paths,
        selector,
        cached_repo,
        validated_repo,
        agent,
        runtime_binaries,
        false,
        reason,
        None,
        debug,
        branch_override,
        docker,
        runner,
        repo_lock,
        role_git_sha,
        None,
    )
    .await?;
    Ok(RoleImagePrewarmRow {
        agent,
        image,
        status: ImagePrewarmStatus::Built,
    })
}

pub(crate) fn prewarm_launch_plan_reason(decision: &ImageDecision) -> String {
    match decision {
        ImageDecision::Reuse { .. } => "image_reuse:recipe_hash_match".to_owned(),
        ImageDecision::RefreshInBackground { reason, .. } => {
            format!("image_refresh:{}", reason.as_str())
        }
        ImageDecision::BuildFromPublished { reason, .. } => {
            format!("image_build_from_published:{}", reason.as_str())
        }
        ImageDecision::BuildFromWorkspace { reason, .. } => {
            format!("image_build_from_workspace:{}", reason.as_str())
        }
    }
}
