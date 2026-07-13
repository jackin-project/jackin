//! Docker image build pipeline: prepare binaries, build derived image, tag and cache.

#![allow(clippy::empty_line_after_doc_comments, reason = "residual lint budget")]

#[path = "image/version.rs"]
mod version;

#[path = "image/published.rs"]
mod published;

#[path = "image/prewarm.rs"]
mod prewarm;

#[cfg(not(test))]
pub use prewarm::prewarm_role_images;
pub use prewarm::{ImagePrewarmStatus, RoleImagePrewarmRow};
#[allow(unused_imports, unreachable_pub, reason = "residual lint budget")]
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
    sync::Arc,
};

use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;
#[cfg(not(test))]
use jackin_docker::{ShellRunner, docker_client::BollardDockerClient};
use jackin_image::capsule_binary;
use jackin_image::derived_image::{
    AgentInstall, create_derived_build_context_for_agents, create_role_base_build_context,
};
use jackin_image::image_recipe::{
    expected_image_recipes, recipe_labels, supported_set_uses_cache_bust,
};
use jackin_image::version_check;
use jackin_launch_tui::build_log::DiagnosticsBuildLogSink;
use jackin_manifest::repo::CachedRepo;

#[cfg(test)]
pub(crate) use jackin_image::image_recipe::{
    build_image_recipe_with_construct_image, expected_image_recipe_for_test,
    image_recipe_label_map_for_install_test, image_recipe_label_map_for_test,
};

use super::naming::{
    LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION,
    LABEL_IMAGE_ROLE_GIT_SHA, image_name, image_name_for_branch, role_base_image_name,
    short_git_sha,
};
use super::progress::{LaunchProgress, LaunchStage};
#[cfg(not(test))]
use super::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

pub(super) use jackin_image::image_decision::{
    ImageDecision, ImageInvalidationReason, build_decision, classify_image_labels,
    decision_base_image_override, emit_image_decision, emit_image_reuse,
};

pub(super) struct PreparedRuntimeBinaries {
    agent_installs: BTreeMap<Agent, AgentInstall<PathBuf>>,
    prefetched_agent_versions: BTreeMap<Agent, String>,
    jackin_capsule_src: String,
}

fn local_image_buildx_args() -> Vec<&'static str> {
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

#[allow(
    clippy::too_many_lines,
    reason = "Decide-role-image: per-cache-state + per-invalidation-reason + \
              per-build-strategy branches nested with telemetry. Inline shape \
              preserves the per-decision-arm state machine."
)]
#[allow(
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

#[allow(
    clippy::too_many_arguments,
    reason = "The background sentinel mirrors selected-image refresh inputs \
              so it can resolve the same role and rebuild the same selected image."
)]
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
#[allow(
    clippy::too_many_arguments,
    reason = "Sentinel rebuild uses the same role/image inputs as explicit \
              prewarm plus the reused image tag it is checking."
)]
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

#[allow(
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

#[allow(
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

fn should_mint_fresh_cache_bust(rebuild: bool, build_reason: ImageInvalidationReason) -> bool {
    rebuild || build_reason == ImageInvalidationReason::AgentVersionChanged
}

fn cache_bust_value_for_build(
    paths: &JackinPaths,
    image: &str,
    manifest: &jackin_core::manifest::RoleManifest,
    mint_fresh_cache_bust: bool,
) -> anyhow::Result<String> {
    if !supported_set_uses_cache_bust(manifest) {
        return Ok("unused".to_owned());
    }

    if mint_fresh_cache_bust {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system clock is before UNIX epoch: {e}"))?
            .as_secs()
            .to_string();
        version_check::store_cache_bust(paths, image, &ts);
        return Ok(ts);
    }

    Ok(version_check::stored_cache_bust(paths, image).unwrap_or_else(|| "0".to_owned()))
}

/// Resolve the role's **base** image into a local `jk_<role>__base:<sha>` image
/// that the derived overlay is built `FROM`.
///
/// - `published_base = Some(img)` (the decision found a fresh published image):
///   tag the already pulled and label-verified image as the local base.
/// - `published_base = None`: build the role Dockerfile locally (construct `FROM`
///   overridden by `JACKIN_CONSTRUCT_IMAGE` when set), no overlay.
///
/// Reused when `jk_<role>__base:<sha>` already exists and its labels match the
/// current role SHA plus either the local-build construct image label or the
/// published-image construct version label — so the heavy role layers are built
/// or tagged once per (role commit, construct) and overlay rebuilds don't touch
/// them.
#[allow(
    clippy::too_many_arguments,
    reason = "Resolving the local role base needs every caller-supplied input \
              (selector, branch + head sha, cached + validated repos, published \
              base, rebuild, debug, docker, runner) to flow through to the pull- \
              or-build branch. Named-arg reads match the per-input propagation \
              idiom the role-base resolver walks."
)]
async fn ensure_local_role_base(
    selector: &RoleSelector,
    branch_override: Option<&str>,
    head_sha: Option<&str>,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    published_base: Option<&str>,
    rebuild: bool,
    debug: bool,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<String> {
    // The base is always materialized locally as `jk_<role>__base:<sha>` so the
    // derived overlay never depends on the mutable published `:latest` tag:
    //   - published fresh -> tag the verified image under the local base name;
    //   - otherwise        -> build the role Dockerfile locally.
    // Reused when the local base tag already exists and its labels still match.
    let construct = jackin_manifest::repo_contract::construct_image();
    let base_name = role_base_image_name(selector, branch_override, head_sha);

    if !rebuild
        && docker
            .list_image_tags(&base_name)
            .await
            .is_ok_and(|tags| !tags.is_empty())
        && docker
            .inspect_image_labels(&base_name)
            .await
            .is_ok_and(|labels| {
                local_role_base_labels_match(
                    &labels,
                    &construct,
                    &validated_repo.dockerfile.construct_version,
                    head_sha,
                )
            })
    {
        jackin_diagnostics::debug_log!("image", "reusing local role base {base_name}");
        return Ok(base_name);
    }

    if let Some(published) = published_base {
        jackin_diagnostics::active_timing_started(
            "derived image",
            "tag_role_base",
            Some(&base_name),
        );
        if let Some(p) = progress.as_deref_mut() {
            p.stage_progress(
                LaunchStage::DerivedImage,
                "Tagging published role base image",
            );
        }
        let args = ["tag", published, base_name.as_str()];
        let options = RunOptions {
            capture_stderr: true,
            capture_stdout: true,
            null_stdin: true,
            ..RunOptions::default()
        };
        let result = runner.run("docker", &args, None, &options).await;
        jackin_diagnostics::active_timing_done(
            "derived image",
            "tag_role_base",
            if result.is_ok() {
                Some("tagged")
            } else {
                Some("error")
            },
        );
        result?;
        return Ok(base_name);
    }

    jackin_diagnostics::active_timing_started("derived image", "build_role_base", Some(&base_name));
    let build = create_role_base_build_context(&cached_repo.repo_dir, validated_repo, None)?;
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();
    let role_sha_label = format!(
        "{LABEL_IMAGE_ROLE_GIT_SHA}={}",
        head_sha.map_or("unknown", short_git_sha)
    );
    let construct_label = format!("{LABEL_IMAGE_CONSTRUCT}={construct}");
    let build_arg_role_git_sha = format!("ROLE_GIT_SHA={}", head_sha.unwrap_or("unknown"));

    let mut args = local_image_buildx_args();
    // A workspace rebuild refreshes the construct base. A plain workspace base
    // build rides the local layer cache.
    //
    // Only `--pull` the default published construct: an operator override of
    // `JACKIN_CONSTRUCT_IMAGE` (e.g. the local `jackin-local/construct:trixie`
    // built for PR verification) exists only in the local image store, so
    // `--pull` would force a registry resolve and fail with "pull access denied".
    // We treat "not the default published image" as "locally built / not
    // pullable" — overriding to a *different published* image would also skip the
    // pull, which is an accepted limitation of the override.
    let construct_is_locally_built = construct != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
    if rebuild && !construct_is_locally_built {
        args.push("--pull");
    }
    args.extend(["--label", &role_sha_label, "--label", &construct_label]);
    if dockerfile_requests_role_git_sha_arg(&build.dockerfile_path) {
        args.extend(["--build-arg", &build_arg_role_git_sha]);
    }

    let needs_token = dockerfile_requests_github_token_secret(&build.dockerfile_path);
    let github_token = if needs_token {
        resolve_github_token(runner).await
    } else {
        None
    };
    let secret_file: Option<tempfile::NamedTempFile> = github_token.as_ref().and_then(|token| {
        let mut f = tempfile::NamedTempFile::new().ok()?;
        std::io::Write::write_all(&mut f, token.as_bytes()).ok()?;
        Some(f)
    });
    let secret_arg = secret_file
        .as_ref()
        .map(|f| format!("id=github_token,src={}", f.path().display()));
    if let Some(ref s) = secret_arg {
        args.extend(["--secret", s.as_str()]);
    }
    let output_arg = local_image_output_arg(&base_name);
    args.extend([
        "--output",
        &output_arg,
        "-f",
        &dockerfile_path,
        &context_dir,
    ]);

    // Surface the role-base build on the live build screen exactly like the
    // derived build: a stage header plus the captured output teed into the
    // build-log panel the cockpit shows on demand.
    if let Some(p) = progress.as_deref_mut() {
        p.stage_progress(LaunchStage::DerivedImage, "Building role base image");
    }
    emit_non_containerd_image_store_note(runner).await;
    jackin_diagnostics::build_log::begin();
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &args, None, &build_options);
    let build_result = match progress.as_deref() {
        Some(p) => p.while_waiting(build_future).await,
        None => build_future.await,
    };
    jackin_diagnostics::build_log::end();
    jackin_diagnostics::active_timing_done(
        "derived image",
        "build_role_base",
        if build_result.is_ok() {
            Some("built")
        } else {
            Some("error")
        },
    );
    emit_docker_build_step_diagnostics();
    build_result?;
    Ok(base_name)
}

/// Build the Docker image for the role. Returns the image name.
#[allow(
    clippy::too_many_arguments,
    reason = "Building the agent image needs every caller-supplied input \
              (paths, selector, cached + validated repos, agent, runtime \
              binaries, rebuild + reason + base override, debug + branch override, \
              docker, runner, repo_lock, known head sha, progress) to flow into \
              the build pipeline. Named-arg reads match the per-input propagation \
              idiom the image builder walks."
)]
#[allow(
    clippy::too_many_lines,
    reason = "Same justification as the too_many_arguments allow: build-agent- \
              image carries every caller-supplied input through the build \
              pipeline. Inline shape preserves captured-locals across phases."
)]
pub(super) async fn build_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    runtime_binaries: PreparedRuntimeBinaries,
    rebuild: bool,
    build_reason: ImageInvalidationReason,
    build_base_image_override: Option<&str>,
    debug: bool,
    branch_override: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
    known_head_sha: Option<&str>,
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<String> {
    let use_prebuilt = build_base_image_override.is_some();
    let build_source_reason = if use_prebuilt {
        "published_image_fresh"
    } else if branch_override.is_some() {
        "branch_override"
    } else if rebuild {
        "rebuild_requested"
    } else {
        build_reason.as_str()
    };

    // Resolve the role repo HEAD SHA once — used for the published-image
    // staleness check, the local-image freshness check, and as a build-arg
    // so local builds carry the same label.
    let head_sha = role_git_sha_for_recipe(cached_repo, known_head_sha, runner).await;

    let local_image_name = branch_override.map_or_else(
        || image_name(selector, head_sha.as_deref()),
        |b| image_name_for_branch(selector, b, head_sha.as_deref()),
    );

    let force_base_rebuild = rebuild;
    let mint_fresh_cache_bust = should_mint_fresh_cache_bust(rebuild, build_reason);

    // Resolve the role base into a local `jk_<role>__base:<sha>` image — tagged
    // from the pulled, label-verified published image when the decision found it
    // fresh (`build_base_image_override`), or built from the role Dockerfile
    // otherwise — then derive the overlay `FROM` it. The derived build below
    // always uses a local base, so it never inlines the role Dockerfile and
    // never `--pull`s.
    let local_base = ensure_local_role_base(
        selector,
        branch_override,
        head_sha.as_deref(),
        cached_repo,
        validated_repo,
        build_base_image_override,
        force_base_rebuild,
        debug,
        docker,
        runner,
        progress.as_deref_mut(),
    )
    .await?;
    let base_image_override = Some(local_base.as_str());

    // create_derived_build_context copies the repo into a temp directory,
    // creating an immutable snapshot.  After this point the shared cached
    // repo can be safely modified by a parallel load.
    jackin_diagnostics::active_timing_started("derived image", "create_build_context", None);
    // Install every supported agent into the image, not just the selected one.
    // The container runs a multiplexer; a new tab can launch any supported
    // agent, exec'ing its CLI inside this same container. A selected-agent-only
    // image makes those sibling tabs crash with a missing binary. The selected
    // agent still drives the recipe's selected-install/version label.
    let agents_to_install = validated_repo.manifest.supported_agents();
    let build_result = create_derived_build_context_for_agents(
        &cached_repo.repo_dir,
        validated_repo,
        base_image_override,
        Some(&runtime_binaries.jackin_capsule_src),
        &agents_to_install,
        &runtime_binaries.agent_installs,
    );
    let build = match build_result {
        Ok(build) => {
            jackin_diagnostics::active_timing_done(
                "derived image",
                "create_build_context",
                Some("created"),
            );
            let source = if base_image_override.is_some() {
                "published"
            } else {
                "workspace"
            };
            emit_build_context_snapshot(&build.context_dir, source);
            build
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                "derived image",
                "create_build_context",
                Some("error"),
            );
            return Err(error);
        }
    };
    drop(repo_lock);

    // Read the rendered Dockerfile once and drive every downstream decision
    // (debug dump, ROLE_GIT_SHA arg, github_token secret) off the in-memory
    // body instead of re-reading the file 2–3× per build. On read error fall
    // back to each predicate's conservative default (token=true, sha=false).
    let dockerfile_body = std::fs::read_to_string(&build.dockerfile_path).ok();
    if debug {
        let rendered = dockerfile_body.as_deref().unwrap_or("<read failed>");
        jackin_diagnostics::emit_debug_line(
            "image",
            &format!(
                "DerivedDockerfile ({}):\n{rendered}",
                build.dockerfile_path.display(),
            ),
        );
    }
    let requests_role_git_sha = dockerfile_body
        .as_deref()
        .is_some_and(dockerfile_body_requests_role_git_sha_arg);
    let requests_github_token = dockerfile_body
        .as_deref()
        .is_none_or(dockerfile_body_requests_github_token_secret);
    let image = local_image_name.clone();

    let build_arg_role_git_sha =
        format!("ROLE_GIT_SHA={}", head_sha.as_deref().unwrap_or("unknown"));
    let build_arg_run_uid = format!(
        "JACKIN_RUN_UID={}",
        crate::runtime::identity::host_uid().unwrap_or(1000)
    );
    // Always pass the cache-bust arg so Docker matches the correct layer.
    //
    // When rebuilding, generate a fresh timestamp to invalidate fallback
    // installer layers, and persist it so subsequent non-rebuild builds reuse
    // the same layer.
    //
    // When NOT rebuilding, replay the stored bust value.  Without this,
    // Docker resolves the Dockerfile default `JACKIN_CACHE_BUST=0` and hits
    // the original pre-bust layer, causing the installed agent version to
    // ping-pong between old and new on alternate launches.
    // If a derived image already exists locally, check whether it was built
    // against the same construct image as the current invocation. A mismatch
    // means the cached image is tainted — e.g. built with a local construct
    // override while this invocation uses the canonical one, or vice versa —
    // and must be rebuilt from scratch rather than reused.
    let current_construct = jackin_manifest::repo_contract::construct_image();
    // When rebuild is already forced, the mismatch check result cannot change the
    // outcome — skip the round-trip. Treat inspect errors as label-absent (no
    // mismatch) so transient daemon errors never abort an otherwise-proceeding build.
    let construct_mismatch = if force_base_rebuild {
        false
    } else {
        docker
            .inspect_image_label(&image, LABEL_IMAGE_CONSTRUCT)
            .await
            .unwrap_or(None)
            .is_some_and(|cached| cached != current_construct)
    };
    let mint_fresh_cache_bust = mint_fresh_cache_bust || construct_mismatch;

    let cache_bust_value = cache_bust_value_for_build(
        paths,
        &image,
        &validated_repo.manifest,
        mint_fresh_cache_bust,
    )?;
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();
    let recipe = jackin_image::image_recipe::build_image_recipe(
        cached_repo,
        validated_repo,
        head_sha.as_deref(),
        branch_override,
        base_image_override,
        &cache_bust_value,
    )?;
    let recipe_hash = recipe.hash()?;
    let recipe_labels = recipe_labels(&recipe, &recipe_hash);

    let mut build_args = local_image_buildx_args();

    // --pull semantics:
    //
    // The overlay always builds `FROM` the local `jk_<role>__base:<sha>` (restamped
    // from the published image or built locally by ensure_local_role_base), so the
    // derived build never `--pull`s — that would fail on the local-only base tag.
    // (The pull-vs-cache decision for the base itself lives in ensure_local_role_base.)
    emit_image_build_source(base_image_override, build_source_reason, false);

    let cache_bust = format!("JACKIN_CACHE_BUST={cache_bust_value}");
    build_args.extend(["--build-arg", &build_arg_run_uid]);
    if supported_set_uses_cache_bust(&validated_repo.manifest) {
        build_args.extend(["--build-arg", &cache_bust]);
    }
    if requests_role_git_sha {
        build_args.extend(["--build-arg", &build_arg_role_git_sha]);
    }
    for label in &recipe_labels {
        build_args.extend(["--label", label]);
    }
    // Stamp per-agent baked-binary versions as diagnostic labels (D3/D20).
    // Not part of the recipe hash; used for observability and future
    // version-comparison rebuild enforcement.
    let agent_version_labels: Vec<String> = runtime_binaries
        .prefetched_agent_versions
        .iter()
        .map(|(agent, version)| {
            format!(
                "{LABEL_IMAGE_AGENT_VERSION_PREFIX}.{}.version={version}",
                agent.slug()
            )
        })
        .collect();
    for label in &agent_version_labels {
        build_args.extend(["--label", label]);
    }
    let output_arg = local_image_output_arg(&image);
    build_args.extend([
        "--output",
        &output_arg,
        "-f",
        &dockerfile_path,
        &context_dir,
    ]);

    jackin_diagnostics::active_timing_started("derived image", "resolve_github_token", None);
    let github_token = if requests_github_token {
        resolve_github_token(runner).await
    } else {
        None
    };
    jackin_diagnostics::active_timing_done(
        "derived image",
        "resolve_github_token",
        if !requests_github_token {
            Some("skipped")
        } else if github_token.is_some() {
            Some("token")
        } else {
            Some("none")
        },
    );
    let secret_file: Option<tempfile::NamedTempFile> =
        github_token
            .as_ref()
            .and_then(|token| match tempfile::NamedTempFile::new() {
                Err(e) => {
                    emit_compact_image_warning(&format!(
                        "failed to create tempfile for GitHub token: {e}; build will use unauthenticated GitHub API"
                    ),
                    );
                    None
                }
                Ok(mut f) => match std::io::Write::write_all(&mut f, token.as_bytes()) {
                    Err(e) => {
                        emit_compact_image_warning(&format!(
                            "failed to write GitHub token to tempfile: {e}; build will use unauthenticated GitHub API"
                        ),
                        );
                        None
                    }
                    Ok(()) => Some(f),
                },
            });
    let secret_arg = secret_file
        .as_ref()
        .map(|f| format!("id=github_token,src={}", f.path().display()));
    if let Some(ref s) = secret_arg {
        build_args.extend(["--secret", s.as_str()]);
    }

    if let Some(ref mut p) = progress {
        p.stage_progress(LaunchStage::DerivedImage, "Building Docker image");
    }
    emit_non_containerd_image_store_note(runner).await;

    // Tee the build's captured output into the live build-log sink so the
    // loading cockpit can show it on demand (the build is the slowest step).
    // `end` stops teeing but keeps the captured lines for the dialog.
    //
    // `build_log::end()` must always fire — even on cancellation — so the
    // process-global ACTIVE flag is reset before the next launch. The
    // `while_waiting` branch returns `Err` on cancel, which we capture in
    // `build_result` and only `?`-propagate after calling `end()`.
    jackin_diagnostics::build_log::begin();
    jackin_diagnostics::active_timing_started("derived image", "docker_build", None);
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &build_args, None, &build_options);
    // The Docker build is the slowest foreground step, so its await must race
    // the launch cancel token — otherwise Ctrl+C / Exit during the build is
    // ignored until docker finishes (the operator sees the modal hang). With a
    // rich surface, `while_waiting` returns `Err(LaunchCancelled)` the instant
    // the token fires; headless launches just await.
    let build_result = match progress.as_deref() {
        Some(p) => p.while_waiting(build_future).await,
        None => build_future.await,
    };
    jackin_diagnostics::build_log::end();
    jackin_diagnostics::active_timing_done(
        "derived image",
        "docker_build",
        if build_result.is_ok() {
            Some("built")
        } else {
            Some("error")
        },
    );
    emit_docker_build_step_diagnostics();
    build_result?;

    record_built_agent_version(paths, &image, agent, &runtime_binaries, debug, runner).await;

    Ok(image)
}

/// Returns the HEAD commit SHA of the git repo at `dir`, or `None` if the
/// directory is not a git repo or the command fails.
async fn git_head_sha(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    runner
        .capture("git", &["-C", &dir_str, "rev-parse", "HEAD"], None)
        .await
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
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

#[allow(
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
