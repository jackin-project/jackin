// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sibling image prewarm and recipe git SHA.

#[cfg(not(test))]
use jackin_core::Agent;
use jackin_core::CommandRunner;
#[cfg(not(test))]
use jackin_core::JackinPaths;
#[cfg(not(test))]
use jackin_core::RoleSelector;

#[cfg(not(test))]
use jackin_docker::{ShellRunner, docker_client::BollardDockerClient};

use jackin_manifest::repo::CachedRepo;

#[cfg(not(test))]
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

#[cfg(not(test))]
use super::ImagePrewarmStatus;
use super::git_head_sha;
#[cfg(not(test))]
use super::{SiblingImagePrewarmOutcome, prewarm_agent_image_from_validated_repo};

#[cfg(not(test))]
pub(crate) async fn prewarm_sibling_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    agent: Agent,
) -> anyhow::Result<SiblingImagePrewarmOutcome> {
    let mut runner = ShellRunner { debug: false };
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

    match prewarm_agent_image_from_validated_repo(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        branch_override,
        agent,
        &docker,
        &mut runner,
        repo_lock,
        false,
    )
    .await?
    .status
    {
        ImagePrewarmStatus::Reused => Ok(SiblingImagePrewarmOutcome::Reused),
        ImagePrewarmStatus::Built => Ok(SiblingImagePrewarmOutcome::Built),
    }
}

pub(crate) async fn role_git_sha_for_recipe(
    cached_repo: &CachedRepo,
    known_head_sha: Option<&str>,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "role_git_sha",
        None,
    );
    let (head_sha, detail) = if let Some(sha) = known_head_sha {
        (Some(sha.to_owned()), "known")
    } else {
        let resolved = git_head_sha(&cached_repo.repo_dir, runner).await;
        let detail = if resolved.is_some() {
            "resolved"
        } else {
            "unavailable"
        };
        (resolved, detail)
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "role_git_sha",
        Some(detail),
    );
    head_sha
}
