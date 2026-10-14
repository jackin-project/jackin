// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AppConfig, DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS};
use jackin_core::{CommandRunner, JackinPaths, RoleSelector};
use jackin_docker::docker_client::DockerApi;

use jackin_runtime_image::image::ImageDecision;
use jackin_runtime_naming::naming::{image_name, image_name_for_branch};
use jackin_runtime_repo_cache::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchImagePlan {
    pub decision: &'static str,
    pub reason: Option<&'static str>,
    pub image: String,
    pub base_image: Option<String>,
    pub role_git_sha: Option<String>,
    pub published_image: Option<String>,
    pub role_models: std::collections::BTreeMap<jackin_core::Agent, String>,
}

impl LaunchImagePlan {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "decision": self.decision,
            "reason": self.reason,
            "image": self.image,
            "base_image": self.base_image,
            "role_git_sha": self.role_git_sha,
        })
    }
}

pub async fn resolve_launch_image_plan(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    rebuild: bool,
    role_branch: Option<&str>,
) -> anyhow::Result<LaunchImagePlan> {
    let (source, _is_new) = config.resolve_role_source(selector)?;
    let ttl = if rebuild {
        std::time::Duration::ZERO
    } else {
        std::time::Duration::from_secs(
            config
                .role_repo_refresh_ttl_seconds
                .unwrap_or(DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS),
        )
    };
    let (cached_repo, validated_repo, _repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::interactive(false)
            .with_branch(role_branch)
            .with_refresh_ttl(ttl),
        || Ok(false),
    )
    .await?;
    let published_image = validated_repo.manifest.published_image.clone();
    let decision = jackin_runtime_image::image::decide_role_image(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        rebuild,
        role_branch,
        None,
        docker,
        runner,
    )
    .await?;
    let mut plan = plan_from_decision(selector, role_branch, &decision, published_image);
    plan.role_models = jackin_runtime_launch_capsule_setup::capsule_setup::role_model_defaults(
        &validated_repo.manifest,
    );
    Ok(plan)
}

pub(crate) fn plan_from_decision(
    selector: &RoleSelector,
    role_branch: Option<&str>,
    decision: &ImageDecision,
    published_image: Option<String>,
) -> LaunchImagePlan {
    let role_git_sha = decision.role_git_sha();
    let image = decision.resolved_image().map_or_else(
        || {
            role_branch.map_or_else(
                || image_name(selector, role_git_sha.as_deref()),
                |branch| image_name_for_branch(selector, branch, role_git_sha.as_deref()),
            )
        },
        ToOwned::to_owned,
    );
    LaunchImagePlan {
        decision: decision.kind(),
        reason: decision
            .reason()
            .map(jackin_runtime_image::image::ImageInvalidationReason::as_str),
        image,
        base_image: decision.base_image_ref().map(ToOwned::to_owned),
        role_git_sha,
        published_image,
        role_models: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests;
