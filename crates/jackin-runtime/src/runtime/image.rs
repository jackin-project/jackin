// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker image build pipeline: prepare binaries, build derived image, tag and cache.

#[path = "image/version.rs"]
mod version;

#[path = "image/published.rs"]
mod published;

#[path = "image/prewarm.rs"]
mod prewarm;

#[path = "image/build.rs"]
mod build;
pub(crate) use build::{build_agent_image, git_head_sha};
#[cfg(test)]
pub(crate) use build::{
    cache_bust_value_for_build, ensure_local_role_base, should_mint_fresh_cache_bust,
};

#[cfg(not(test))]
pub use prewarm::prewarm_role_images;
pub use prewarm::{ImagePrewarmStatus, RoleImagePrewarmRow};
pub use version::*;

use published::{PublishedImageFreshness, published_image_freshness, published_image_is_stale};

// Stages: collect agent binaries → build derived context → `docker build` →
// tag. Not responsible for container start, session attach, or identity
// capture — those live in sibling modules.
//
// Key invariant: the derived image label set written here is the authority
// consumed by `discovery` and `naming` for cache-hit detection.

use anyhow::Context as _;
use futures_util::future::try_join_all;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use jackin_core::CommandRunner;
use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::DockerApi;
#[cfg(not(test))]
use jackin_docker::{ShellRunner, docker_client::BollardDockerClient};
use jackin_image::capsule_binary;
use jackin_image::derived_image::AgentInstall;
use jackin_image::image_recipe::expected_image_recipes;
use jackin_image::version_check;
use jackin_manifest::repo::CachedRepo;

#[cfg(test)]
pub(crate) use jackin_image::image_recipe::{
    build_image_recipe_with_construct_image, expected_image_recipe_for_test,
    image_recipe_label_map_for_install_test, image_recipe_label_map_for_test,
};

use super::naming::{
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, image_name,
    image_name_for_branch, role_base_image_name,
};
use super::progress::{LaunchProgress, LaunchStage};
#[cfg(not(test))]
use super::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

pub(super) use jackin_image::image_decision::{
    ImageDecision, ImageInvalidationReason, build_decision, classify_image_labels,
    decision_base_image_override, emit_image_decision, emit_image_reuse,
};

pub(crate) struct PreparedRuntimeBinaries {
    agent_installs: BTreeMap<Agent, AgentInstall<PathBuf>>,
    prefetched_agent_versions: BTreeMap<Agent, String>,
    jackin_capsule_src: String,
}

pub(super) fn local_image_buildx_args() -> Vec<&'static str> {
    // Runtime image builds consume local-only base tags such as
    // `jk_<role>__base:<sha>` and PR-local construct images. A docker-container
    // buildx builder cannot see the host Docker image store, so use the
    // Docker-driver default builder. The global context flag keeps buildx from
    // rejecting `default` when DOCKER_HOST or another active context is set.
    vec![
        "--context",
        "default",
        "buildx",
        "build",
        "--builder",
        "default",
    ]
}

#[expect(
    clippy::too_many_lines,
    reason = "Decide-role-image: per-cache-state + per-invalidation-reason + \
              per-build-strategy branches nested with telemetry. Inline shape \
              preserves the per-decision-arm state machine."
)]
#[expect(
    clippy::too_many_arguments,
    reason = "Decide-role-image call site propagates paths, selector, cached + \
              validated repos, rebuild + branch override + pinned sha, docker, \
              runner. Named-arg reads match the per-input propagation idiom; \
              bundling into a config struct is the deferred-parallel-pass."
)]
pub(super) async fn decide_role_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    rebuild: bool,
    branch_override: Option<&str>,
    pinned_sha: Option<&str>, // D7: skips git rev-parse HEAD when Some
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<ImageDecision> {
    // Resolve the role-repo HEAD SHA up front: it is both the image *tag* (so
    // each role commit gets its own immutable image instead of overwriting a
    // mutable `:latest`) and an input to the published-image staleness checks
    // below. The recipe-hash / construct labels still decide reuse-vs-rebuild
    // within a tag — only the name carries the SHA.
    let head_sha = role_git_sha_for_recipe(cached_repo, pinned_sha, runner).await;
    let image = branch_override.map_or_else(
        || image_name(selector, head_sha.as_deref()),
        |branch| image_name_for_branch(selector, branch, head_sha.as_deref()),
    );
    let mut base_image_override = decision_base_image_override(validated_repo, branch_override);
    if rebuild {
        emit_image_decision(&image, ImageInvalidationReason::ExplicitRebuild);
        return Ok(ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::ExplicitRebuild,
            role_git_sha: head_sha,
        });
    }

    jackin_diagnostics::active_timing_started(
        "derived image",
        "image_tag_lookup",
        Some(image.as_str()),
    );
    let tag_result = docker.list_image_tags(&image).await;
    jackin_diagnostics::active_timing_done(
        "derived image",
        "image_tag_lookup",
        if tag_result.is_ok() {
            Some(image.as_str())
        } else {
            Some("error")
        },
    );
    let tags = match tag_result {
        Ok(tags) => tags,
        Err(error) => {
            // Always-on, not just `debug_log!`: a failing tag lookup forces a
            // full rebuild, and a persistently degraded Docker daemon turns
            // that into a silent rebuild storm whose only symptom (without
            // JACKIN_DEBUG=1) is every launch being slow. Surface the cause.
            tracing::warn!(
                %image,
                error = format!("{error:#}"),
                "could not list local image tags; falling back to a full rebuild (Docker daemon may be unhealthy)"
            );
            jackin_diagnostics::debug_log!(
                "image",
                "could not list local image tags for {image}; rebuilding: {error:#}"
            );
            emit_image_decision(&image, ImageInvalidationReason::ImageListFailed);
            return Ok(build_decision(
                ImageInvalidationReason::ImageListFailed,
                None,
                base_image_override,
            ));
        }
    };
    if tags.is_empty() {
        let mut reason = ImageInvalidationReason::LocalImageMissing;
        if let Some(published) = base_image_override {
            let freshness = published_image_freshness(
                published,
                &validated_repo.dockerfile.construct_version,
                head_sha.as_deref(),
                docker,
            )
            .await;
            let stale = match freshness {
                PublishedImageFreshness::Fresh => false,
                PublishedImageFreshness::Stale => true,
                PublishedImageFreshness::NeedsRoleSha(stored_sha) => {
                    head_sha.as_deref() != Some(stored_sha.as_str())
                }
            };
            if stale {
                jackin_diagnostics::debug_log!(
                    "image",
                    "published image {published} is out of date; building from workspace Dockerfile"
                );
                base_image_override = None;
                reason = ImageInvalidationReason::PublishedImageStale;
            }
        }
        emit_image_decision(&image, reason);
        return Ok(build_decision(reason, head_sha, base_image_override));
    }

    jackin_diagnostics::active_timing_started("derived image", "image_recipe", None);
    let local_base_image = role_base_image_name(selector, branch_override, head_sha.as_deref());
    let expected_recipes = expected_image_recipes(
        cached_repo,
        validated_repo,
        head_sha.as_deref(),
        branch_override,
        Some(local_base_image.as_str()),
        paths,
        &image,
    )?;
    jackin_diagnostics::active_timing_done(
        "derived image",
        "image_recipe",
        Some(&format!("{} expected recipes", expected_recipes.len())),
    );
    jackin_diagnostics::active_timing_started(
        "derived image",
        "image_label_inspect",
        Some(image.as_str()),
    );
    let label_result = docker.inspect_image_labels(&image).await;
    jackin_diagnostics::active_timing_done(
        "derived image",
        "image_label_inspect",
        if label_result.is_ok() {
            Some(image.as_str())
        } else {
            Some("error")
        },
    );
    let labels = match label_result {
        Ok(labels) => labels,
        Err(error) => {
            // Always-on (see the tag-lookup fallback above): label inspection
            // failing forces a rebuild despite the image existing, so a flaky
            // daemon silently rebuilds on every launch. Make the cause visible.
            tracing::warn!(
                %image,
                error = format!("{error:#}"),
                "local image exists but label inspection failed; falling back to a full rebuild (Docker daemon may be unhealthy)"
            );
            jackin_diagnostics::debug_log!(
                "image",
                "local image {image} exists but label inspection failed; rebuilding: {error:#}"
            );
            emit_image_decision(&image, ImageInvalidationReason::InspectFailed);
            return Ok(build_decision(
                ImageInvalidationReason::InspectFailed,
                head_sha,
                base_image_override,
            ));
        }
    };

    match classify_image_labels(&labels, &expected_recipes) {
        None => {
            jackin_diagnostics::debug_log!(
                "image",
                "reusing derived image {image}; recipe hash matches one current recipe"
            );
            emit_image_reuse(&image);
            Ok(ImageDecision::Reuse { image })
        }
        Some(reason) => {
            if let Some(published) = base_image_override
                && published_image_is_stale(
                    published,
                    &validated_repo.dockerfile.construct_version,
                    head_sha.as_deref(),
                    docker,
                )
                .await
            {
                jackin_diagnostics::debug_log!(
                    "image",
                    "published image {published} is out of date; building from workspace Dockerfile"
                );
                base_image_override = None;
            }
            jackin_diagnostics::debug_log!(
                "image",
                "derived image {image} invalidated ({}); expected one of current recipe hashes",
                reason.as_str()
            );
            emit_image_decision(&image, reason);
            Ok(build_decision(reason, head_sha, base_image_override))
        }
    }
}

pub(super) async fn prepare_runtime_binaries_for_agents(
    paths: &JackinPaths,
    _validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agents: &[Agent],
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<PreparedRuntimeBinaries> {
    if let Some(progress) = &mut progress {
        progress.stage_progress(LaunchStage::AgentBinaries, "preparing agent binaries");
    }

    let agents = agents.to_vec();

    // Resolve + download the selected agent binary and jackin-capsule concurrently.
    // Each ensure_available call is network-bound (HTTP resolve + optional download),
    // so running them in parallel cuts wall-clock time to the slowest single binary
    // rather than the sum of all.
    //
    // Derived image ENTRYPOINT is `/jackin/runtime/jackin-capsule`, so a missing
    // capsule binary would produce an opaque "exec: file not found" at `docker run`.
    // Failing fast here gives an actionable error message.
    let capsule_future = async {
        jackin_diagnostics::active_timing_started("agent binaries", "ensure_capsule_binary", None);
        let result = capsule_binary::ensure_available(paths)
            .await
            .context("preparing jackin-capsule binary");
        jackin_diagnostics::active_timing_done(
            "agent binaries",
            "ensure_capsule_binary",
            if result.is_ok() {
                Some("prefetched")
            } else {
                Some("error")
            },
        );
        result
    };

    let (agent_install_pairs, jackin_capsule_binary) = tokio::try_join!(
        prepare_agent_binaries(paths, &agents, "agent binaries", true),
        capsule_future
    )?;
    // Each agent appears once (one pass over supported_agents()); the map keys
    // that uniqueness so it cannot drift downstream.
    let mut prefetched_agent_versions = BTreeMap::new();
    let agent_installs: BTreeMap<_, _> = agent_install_pairs
        .into_iter()
        .map(|(agent, install, version)| {
            if let Some(version) = version {
                prefetched_agent_versions.insert(agent, version);
            }
            (agent, install)
        })
        .collect();

    let jackin_capsule_src = jackin_capsule_binary.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "cached jackin-capsule path {} contains non-UTF-8 bytes; cannot reference it from Dockerfile",
            jackin_capsule_binary.display()
        )
    })?;

    Ok(PreparedRuntimeBinaries {
        agent_installs,
        prefetched_agent_versions,
        jackin_capsule_src: jackin_capsule_src.to_owned(),
    })
}

pub(super) fn spawn_sibling_runtime_prewarm(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
    selected_image_reused: bool,
) -> Option<tokio::task::JoinHandle<()>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    let siblings = validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect::<Vec<_>>();
    if siblings.is_empty() {
        if let Some(run) = &active_run {
            run.stage(
                "runtime_prewarm_skipped",
                "agent binaries",
                "no sibling runtime binaries to prewarm",
                Some(selected_agent.slug()),
            );
        }
        return None;
    }
    if !selected_image_reused {
        if let Some(run) = &active_run {
            run.stage(
                "runtime_prewarm_skipped",
                "agent binaries",
                "selected image was rebuilt; skipping sibling runtime binary prewarm to avoid competing with foreground launch",
                Some(selected_agent.slug()),
            );
        }
        return None;
    }

    let paths = paths.clone();
    let agents = siblings
        .iter()
        .map(|agent| agent.slug())
        .collect::<Vec<_>>()
        .join(",");
    if let Some(run) = &active_run {
        let reason = format!("sibling_runtime_prewarm:{agents}");
        let detail = serde_json::json!({
            "plan": "PrewarmOnly",
            "reason": reason,
            "container": null,
        })
        .to_string();
        run.stage(
            "launch_plan",
            "restore",
            "selected launch plan PrewarmOnly",
            Some(&detail),
        );
    }
    Some(tokio::spawn(async move {
        if let Some(run) = &active_run {
            run.stage(
                "runtime_prewarm_started",
                "agent binaries",
                "prewarming sibling runtime binaries",
                Some(&agents),
            );
        }
        if let Some(run) = &active_run {
            run.timing_started("agent binaries", "sibling_runtime_prewarm", Some(&agents));
        }
        let result = prepare_agent_binaries(&paths, &siblings, "runtime prewarm", false).await;
        let timing_detail = match &result {
            Ok(prepared) => agent_binary_prepare_summary(prepared),
            Err(error) => format!("failed: {error:#}"),
        };
        if let Some(run) = &active_run {
            run.timing_done(
                "agent binaries",
                "sibling_runtime_prewarm",
                Some(&timing_detail),
            );
        }
        if let Some(run) = active_run {
            match result {
                Ok(prepared) => run.stage(
                    "runtime_prewarm_done",
                    "agent binaries",
                    "prewarmed sibling runtime binaries",
                    Some(&agent_binary_prepare_summary(&prepared)),
                ),
                Err(error) => run.stage(
                    "runtime_prewarm_failed",
                    "agent binaries",
                    "sibling runtime binary prewarm failed",
                    Some(&format!("{error:#}")),
                ),
            }
        }
    }))
}

pub(super) fn spawn_sibling_image_prewarm(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
    selected_image_reused: bool,
) {
    let siblings = sibling_agents(validated_repo, selected_agent);
    if siblings.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                "derived image",
                "no sibling runtime images to prewarm",
                Some(selected_agent.slug()),
            );
        }
        return;
    }
    if !selected_image_reused {
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                "derived image",
                "selected image was rebuilt; skipping sibling image prewarm to avoid competing with foreground launch",
                Some(selected_agent.slug()),
            );
        }
        return;
    }

    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                "derived image",
                "sibling runtime image prewarm disabled in unit tests",
                Some(selected_agent.slug()),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        tokio::spawn(async move {
            let agents = siblings
                .iter()
                .map(|agent| agent.slug())
                .collect::<Vec<_>>()
                .join(",");
            if let Some(run) = jackin_diagnostics::active_run() {
                run.stage(
                    "sibling_image_prewarm_started",
                    "derived image",
                    "prewarming sibling runtime images",
                    Some(&agents),
                );
            }

            jackin_diagnostics::active_timing_started(
                "derived image",
                "sibling_image_prewarm",
                Some(&agents),
            );
            let (built, reused, failed) = prewarm_sibling_images_concurrently(
                paths,
                selector,
                role_git,
                branch_override,
                siblings,
            )
            .await;
            let timing_detail = if failed.is_empty() {
                format!("built={built}; reused={reused}")
            } else {
                format!("built={built}; reused={reused}; failed={}", failed.len())
            };
            jackin_diagnostics::active_timing_done(
                "derived image",
                "sibling_image_prewarm",
                Some(&timing_detail),
            );
            if let Some(run) = jackin_diagnostics::active_run() {
                if failed.is_empty() {
                    run.stage(
                        "sibling_image_prewarm_done",
                        "derived image",
                        "prewarmed sibling runtime images",
                        Some(&format!("built={built}; reused={reused}")),
                    );
                } else {
                    run.stage(
                        "sibling_image_prewarm_failed",
                        "derived image",
                        "sibling runtime image prewarm finished with failures",
                        Some(&failed.join("; ")),
                    );
                }
            }
        });
    }
}

#[cfg(not(test))]
async fn prewarm_sibling_images_concurrently(
    paths: JackinPaths,
    selector: RoleSelector,
    role_git: String,
    branch_override: Option<String>,
    siblings: Vec<Agent>,
) -> (usize, usize, Vec<String>) {
    let mut built = 0usize;
    let mut reused = 0usize;
    let mut failed = Vec::new();
    let mut tasks = tokio::task::JoinSet::new();
    for sibling in siblings {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.clone();
        let branch_override = branch_override.clone();
        tasks.spawn(async move {
            let result = prewarm_sibling_image(
                &paths,
                &selector,
                &role_git,
                branch_override.as_deref(),
                sibling,
            )
            .await;
            (sibling, result)
        });
    }

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((_, Ok(SiblingImagePrewarmOutcome::Reused))) => reused += 1,
            Ok((_, Ok(SiblingImagePrewarmOutcome::Built))) => built += 1,
            Ok((sibling, Err(error))) => {
                failed.push(format!("{}: {error:#}", sibling.slug()));
            }
            Err(error) => failed.push(format!("task: {error:#}")),
        }
    }
    failed.sort();
    (built, reused, failed)
}

pub(super) fn spawn_selected_image_refresh(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    selected_agent: Agent,
    reason: ImageInvalidationReason,
    debug: bool,
) {
    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "selected_image_refresh_skipped",
                "derived image",
                "selected image refresh disabled in unit tests",
                Some(&format!("{}:{}", selected_agent.slug(), reason.as_str())),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        tokio::spawn(async move {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.stage(
                    "selected_image_refresh_started",
                    "derived image",
                    "refreshing selected runtime image in background",
                    Some(&format!("{}:{}", selected_agent.slug(), reason.as_str())),
                );
            }

            let timing_detail = format!("{}:{}", selected_agent.slug(), reason.as_str());
            jackin_diagnostics::active_timing_started(
                "derived image",
                "selected_image_refresh",
                Some(&timing_detail),
            );
            let result = prewarm_agent_image(
                &paths,
                &selector,
                &role_git,
                branch_override.as_deref(),
                selected_agent,
                debug,
            )
            .await;
            let timing_done = match &result {
                Ok(row) => format!("{}:{:?}", row.agent.slug(), row.status),
                Err(error) => format!("{}: failed: {error:#}", selected_agent.slug()),
            };
            jackin_diagnostics::active_timing_done(
                "derived image",
                "selected_image_refresh",
                Some(&timing_done),
            );

            if let Some(run) = jackin_diagnostics::active_run() {
                match result {
                    Ok(row) => run.stage(
                        "selected_image_refresh_done",
                        "derived image",
                        "refreshed selected runtime image in background",
                        Some(&format!(
                            "{}:{:?}:{}",
                            row.agent.slug(),
                            row.status,
                            row.image
                        )),
                    ),
                    Err(error) => run.stage(
                        "selected_image_refresh_failed",
                        "derived image",
                        "selected runtime image refresh failed",
                        Some(&format!("{}: {error:#}", selected_agent.slug())),
                    ),
                }
            }
        });
    }
}

pub(super) fn reuse_needs_background_staleness_check(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    image: &str,
) -> bool {
    validated_repo.manifest.published_image.is_some()
        || validated_repo
            .manifest
            .supported_agents()
            .into_iter()
            .any(|agent| version_check::stored_version(paths, agent, image).is_some())
}

pub(super) fn spawn_reuse_staleness_sentinel(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    selected_agent: Agent,
    image: &str,
    debug: bool,
) {
    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "reuse_staleness_sentinel_skipped",
                "derived image",
                "reuse staleness sentinel disabled in unit tests",
                Some(&format!("{}:{image}", selected_agent.slug())),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        let image = image.to_owned();
        tokio::spawn(async move {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.stage(
                    "reuse_staleness_sentinel_started",
                    "derived image",
                    "checking reused runtime image staleness in background",
                    Some(&format!("{}:{image}", selected_agent.slug())),
                );
            }

            let result = reuse_staleness_sentinel(
                &paths,
                &selector,
                &role_git,
                branch_override.as_deref(),
                selected_agent,
                &image,
                debug,
            )
            .await;

            if let Some(run) = jackin_diagnostics::active_run() {
                match result {
                    Ok(Some(row)) => run.stage(
                        "reuse_staleness_sentinel_done",
                        "derived image",
                        "refreshed reused runtime image in background",
                        Some(&format!(
                            "{}:{:?}:{}",
                            row.agent.slug(),
                            row.status,
                            row.image
                        )),
                    ),
                    Ok(None) => run.stage(
                        "reuse_staleness_sentinel_done",
                        "derived image",
                        "reused runtime image is still fresh",
                        Some(&format!("{}:{image}", selected_agent.slug())),
                    ),
                    Err(error) => run.stage(
                        "reuse_staleness_sentinel_failed",
                        "derived image",
                        "reuse staleness sentinel failed",
                        Some(&format!("{}: {error:#}", selected_agent.slug())),
                    ),
                }
            }
        });
    }
}

fn sibling_agents(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
) -> Vec<Agent> {
    validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect()
}

#[cfg(not(test))]
enum SiblingImagePrewarmOutcome {
    Reused,
    Built,
}

#[cfg(not(test))]
async fn prewarm_agent_image(
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
async fn reuse_staleness_sentinel(
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
async fn reuse_staleness_reason(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    image: &str,
    role_git_sha: Option<&str>,
    docker: &impl DockerApi,
) -> Option<ImageInvalidationReason> {
    jackin_diagnostics::active_timing_started("derived image", "agent_version_check", Some(image));
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
        "derived image",
        "agent_version_check",
        Some(timing_detail),
    );

    for (agent, check) in &results {
        if *check == version_check::AgentVersionCheck::Unknown {
            tracing::warn!(
                "derived image {image}: could not verify {} version; \
                 staleness undetermined (latest release unresolvable)",
                agent.runtime().slug()
            );
        }
    }
    if let Some((agent, _)) = results
        .into_iter()
        .find(|(_, check)| *check == version_check::AgentVersionCheck::Stale)
    {
        jackin_diagnostics::debug_log!(
            "image",
            "derived image {image}: {} baked version is outdated",
            agent.runtime().slug()
        );
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
        jackin_diagnostics::debug_log!(
            "image",
            "published image {published} is out of date; refreshing reused workspace image"
        );
        return Some(ImageInvalidationReason::PublishedImageStale);
    }

    None
}

#[expect(
    clippy::too_many_arguments,
    reason = "Prewarming the agent image needs every caller-supplied input \
              (paths, selector, cached + validated repos, branch override, \
              agent, docker, runner, repo_lock, debug) to flow into the build \
              pipeline; bundling into a config struct would be a parallel pass \
              that requires restructuring the image-build path. Named-arg reads \
              match the per-input propagation idiom."
)]
async fn prewarm_agent_image_from_validated_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
    agent: Agent,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
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
    super::launch::emit_prewarm_launch_plan(&prewarm_launch_plan_reason(&decision));
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
            jackin_diagnostics::debug_log!(
                "image_prewarm",
                "building {} image from published base: {}",
                agent.slug(),
                reason.as_str()
            );
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
            jackin_diagnostics::debug_log!(
                "image_prewarm",
                "building {} image from workspace Dockerfile: {}",
                agent.slug(),
                reason.as_str()
            );
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
async fn refresh_agent_image_from_validated_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
    agent: Agent,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
    debug: bool,
    reason: ImageInvalidationReason,
    role_git_sha: Option<&str>,
) -> anyhow::Result<RoleImagePrewarmRow> {
    super::launch::emit_prewarm_launch_plan(&format!("image_refresh:{}", reason.as_str()));
    jackin_diagnostics::debug_log!(
        "image_prewarm",
        "refreshing {} image from workspace Dockerfile: {}",
        agent.slug(),
        reason.as_str()
    );
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

fn prewarm_launch_plan_reason(decision: &ImageDecision) -> String {
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

#[cfg(not(test))]
async fn prewarm_sibling_image(
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

async fn prepare_agent_binaries(
    paths: &JackinPaths,
    agents: &[Agent],
    timing_stage: &'static str,
    warn_on_fallback: bool,
) -> anyhow::Result<Vec<(Agent, AgentInstall<PathBuf>, Option<String>)>> {
    let agent_futures = agents.iter().copied().map(|agent| async move {
        let timing_name = format!("ensure_{}_binary", agent.slug());
        jackin_diagnostics::active_timing_started(timing_stage, &timing_name, None);
        match jackin_image::agent_binary::ensure_available(paths, agent).await {
            Ok(binary) => {
                jackin_diagnostics::active_timing_done(
                    timing_stage,
                    &timing_name,
                    Some("prefetched"),
                );
                Ok::<_, anyhow::Error>((
                    binary.agent,
                    AgentInstall::Prefetched(binary.path),
                    binary.version,
                ))
            }
            Err(error) => {
                jackin_diagnostics::active_timing_done(
                    timing_stage,
                    &timing_name,
                    Some("script fallback"),
                );
                if warn_on_fallback {
                    jackin_diagnostics::emit_compact_line(
                        "warning",
                        &format!(
                            "[jackin] could not resolve or download the hard-coded {} binary; the upstream release layout may have changed or the server may be unavailable, so the Docker build will run fallback installer `{}`: {error:#}",
                            agent.slug(),
                            agent.fallback_install_command()
                        ),
                    );
                } else {
                    jackin_diagnostics::debug_log!(
                        "runtime_prewarm",
                        "could not prewarm {} binary; fallback installer remains available: {error:#}",
                        agent.slug()
                    );
                }
                Ok((agent, AgentInstall::ScriptFallback, None))
            }
        }
    });
    try_join_all(agent_futures).await
}

fn agent_binary_prepare_summary(
    prepared: &[(Agent, AgentInstall<PathBuf>, Option<String>)],
) -> String {
    let prefetched = prepared
        .iter()
        .filter(|(_, install, _)| matches!(install, AgentInstall::Prefetched(_)))
        .count();
    let fallback = prepared
        .iter()
        .filter(|(_, install, _)| matches!(install, AgentInstall::ScriptFallback))
        .count();
    let versioned = prepared
        .iter()
        .filter(|(_, _, version)| version.is_some())
        .count();
    format!(
        "{} agents; prefetched={prefetched}; fallback={fallback}; versions={versioned}",
        prepared.len()
    )
}

pub(super) async fn role_git_sha_for_recipe(
    cached_repo: &CachedRepo,
    known_head_sha: Option<&str>,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    jackin_diagnostics::active_timing_started("derived image", "role_git_sha", None);
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
    jackin_diagnostics::active_timing_done("derived image", "role_git_sha", Some(detail));
    head_sha
}

#[expect(
    unused_imports,
    reason = "re-exported so runtime/image/tests.rs can reach them via super::*"
)]
pub(super) use jackin_image::image_build::{
    BuildContextStats, DockerBuildStep, build_context_stats, collect_build_context_stats,
    compact_image_warning_line, docker_build_env, docker_info_uses_containerd_store,
    dockerfile_body_requests_github_token_secret, dockerfile_body_requests_role_git_sha_arg,
    dockerfile_requests_github_token_secret, dockerfile_requests_role_git_sha_arg,
    emit_build_context_snapshot, emit_compact_image_warning, emit_docker_build_step_diagnostics,
    emit_image_build_source, emit_non_containerd_image_store_note, is_buildkit_step_description,
    local_image_output_arg, parse_buildkit_duration_ms, parse_buildkit_line,
    parse_completed_buildkit_step, parse_docker_build_steps, should_stream_build_output,
    split_buildkit_duration,
};

#[cfg(test)]
mod tests;
