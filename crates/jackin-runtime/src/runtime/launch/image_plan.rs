// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! The image half of a launch plan, resolved without launching anything.
//!
//! `jackin load --dry-run` promises the *resolved* plan, and which image a
//! launch would use is only knowable after the role repo is fetched and its
//! manifest read: `published_image` is a manifest field, and the reuse-vs-build
//! decision is derived from that manifest plus the local image labels. Both are
//! resolved here through the same `resolve_agent_repo_with` +
//! `decide_role_image` pair the launch pipeline runs, so a dry run cannot drift
//! from the launch it describes (D-078).

use jackin_config::{AppConfig, DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS};
use jackin_core::{CommandRunner, JackinPaths, RoleSelector};
use jackin_docker::docker_client::DockerApi;

use crate::runtime::image::ImageDecision;
use crate::runtime::naming::{image_name, image_name_for_branch};
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

/// Which image a launch of this role would use, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchImagePlan {
    /// Decision variant (`reuse`, `refresh_in_background`,
    /// `build_from_published`, `build_from_workspace`).
    pub decision: &'static str,
    /// Why the image is rebuilt or refreshed; `None` for a plain reuse.
    pub reason: Option<&'static str>,
    /// Image tag the launch would run.
    pub image: String,
    /// Base/construct image the build would start from, when it builds from a
    /// published base.
    pub base_image: Option<String>,
    /// Role-repo commit the image is pinned to.
    pub role_git_sha: Option<String>,
    /// The manifest's `published_image`, verbatim. `None` when the role
    /// declares none — the warm base is optional, the git source is not.
    pub published_image: Option<String>,
    /// Per-agent role model defaults read during image-plan resolution. The
    /// dry-run path combines these with admitted account models and launch
    /// overrides through the runtime's canonical model resolver.
    pub role_models: std::collections::BTreeMap<jackin_core::Agent, String>,
}

impl LaunchImagePlan {
    /// Machine-readable projection for `--dry-run --format json`.
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

/// Resolve the role repo, read its manifest, and decide the image — the exact
/// work a launch does before it starts a container, and nothing after it.
///
/// # Errors
///
/// Propagates a failure to resolve the role source, fetch or validate the role
/// repository, or inspect local images.
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
        // A dry run makes no destructive choice: a corrupt cached repo is
        // reported, never silently re-cloned behind the operator's back.
        || Ok(false),
    )
    .await?;
    let published_image = validated_repo.manifest.published_image.clone();
    let decision = crate::runtime::image::decide_role_image(
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
    plan.role_models = super::capsule_setup::role_model_defaults(&validated_repo.manifest);
    Ok(plan)
}

/// Project a decided image onto the plan shape. Split out from the async
/// resolution so the projection is testable without Docker or a role repo.
pub(crate) fn plan_from_decision(
    selector: &RoleSelector,
    role_branch: Option<&str>,
    decision: &ImageDecision,
    published_image: Option<String>,
) -> LaunchImagePlan {
    let role_git_sha = decision.role_git_sha();
    let image = decision.resolved_image().map_or_else(
        || {
            // The build variants carry no tag: it is derived from the role SHA
            // exactly as `decide_role_image` derives it.
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
            .map(super::super::image::ImageInvalidationReason::as_str),
        image,
        base_image: decision.base_image_ref().map(ToOwned::to_owned),
        role_git_sha,
        published_image,
        role_models: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests;
