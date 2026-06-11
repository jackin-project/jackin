//! Docker image build pipeline: prepare binaries, build derived image, tag and cache.
//!
//! Stages: collect agent binaries → build derived context → `docker build` →
//! tag. Not responsible for container start, session attach, or identity
//! capture — those live in sibling modules.
//!
//! Key invariant: the derived image label set written here is the authority
//! consumed by `discovery` and `naming` for cache-hit detection.

use anyhow::Context as _;
use futures_util::future::try_join_all;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, HashMap};

use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_image::capsule_binary;
use jackin_image::derived_image::{
    AgentInstall, create_derived_build_context_for_agents, render_derived_dockerfile,
};
use jackin_image::version_check;
use jackin_manifest::repo::CachedRepo;
use std::path::PathBuf;

use super::naming::{
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_RECIPE_HASH,
    LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, LABEL_IMAGE_SELECTED_AGENT,
    LABEL_IMAGE_SELECTED_AGENT_VERSION, image_name,
};
use super::progress::{LaunchProgress, LaunchStage};

const IMAGE_RECIPE_VERSION: &str = "v2";
const LABEL_IMAGE_RECIPE_ROLE_SOURCE_REF: &str = "jackin.recipe.role_source_ref";
const LABEL_IMAGE_RECIPE_BASE_IMAGE: &str = "jackin.recipe.base_image";
const LABEL_IMAGE_RECIPE_GENERATED_RUNTIME: &str = "jackin.recipe.generated_runtime_hash";
const LABEL_IMAGE_RECIPE_SUPPORTED_AGENTS: &str = "jackin.recipe.supported_agents";
const LABEL_IMAGE_RECIPE_SELECTED_AGENT_INSTALL: &str = "jackin.recipe.selected_agent_install";
const LABEL_IMAGE_RECIPE_CACHE_BUST: &str = "jackin.recipe.cache_bust";
const LABEL_IMAGE_RECIPE_CAPSULE_VERSION: &str = "jackin.recipe.capsule_version";
const LABEL_IMAGE_RECIPE_HOOKS: &str = "jackin.recipe.hooks_hash";
const LABEL_IMAGE_RECIPE_CLAUDE_PLUGIN: &str = "jackin.recipe.claude_plugin_hash";
const LABEL_IMAGE_RECIPE_HOST_IDENTITY_STRATEGY: &str = "jackin.recipe.host_identity_strategy";
const HOST_IDENTITY_STRATEGY: &str = "construct-agent-user-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImageInvalidationReason {
    ExplicitRebuild,
    LocalImageMissing,
    ImageListFailed,
    MissingRecipeLabel,
    RecipeVersionChanged,
    RecipeHashChanged,
    RoleGitShaChanged,
    RoleSourceRefChanged,
    BaseImageChanged,
    ConstructImageChanged,
    GeneratedRuntimeChanged,
    SupportedAgentsChanged,
    SelectedAgentChanged,
    SelectedAgentInstallChanged,
    CacheBustChanged,
    CapsuleVersionChanged,
    HooksHashChanged,
    ClaudePluginRecipeChanged,
    HostIdentityStrategyChanged,
    InspectFailed,
}

impl ImageInvalidationReason {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRebuild => "explicit_rebuild",
            Self::LocalImageMissing => "local_image_missing",
            Self::ImageListFailed => "image_list_failed",
            Self::MissingRecipeLabel => "missing_recipe_label",
            Self::RecipeVersionChanged => "recipe_version_changed",
            Self::RecipeHashChanged => "recipe_hash_changed",
            Self::RoleGitShaChanged => "role_git_sha_changed",
            Self::RoleSourceRefChanged => "role_source_ref_changed",
            Self::BaseImageChanged => "base_image_changed",
            Self::ConstructImageChanged => "construct_image_changed",
            Self::GeneratedRuntimeChanged => "generated_runtime_changed",
            Self::SupportedAgentsChanged => "supported_agents_changed",
            Self::SelectedAgentChanged => "selected_agent_changed",
            Self::SelectedAgentInstallChanged => "selected_agent_install_changed",
            Self::CacheBustChanged => "cache_bust_changed",
            Self::CapsuleVersionChanged => "capsule_version_changed",
            Self::HooksHashChanged => "hooks_hash_changed",
            Self::ClaudePluginRecipeChanged => "claude_plugin_recipe_changed",
            Self::HostIdentityStrategyChanged => "host_identity_strategy_changed",
            Self::InspectFailed => "inspect_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImageDecision {
    Reuse {
        image: String,
        selected_agent_version: Option<String>,
    },
    Build {
        reason: ImageInvalidationReason,
        role_git_sha: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
struct ImageRecipe {
    version: &'static str,
    role_git_sha: String,
    role_source_ref: Option<String>,
    base_image: Option<String>,
    construct_image: String,
    generated_runtime_hash: String,
    supported_agents: Vec<String>,
    selected_agent: String,
    selected_agent_install: String,
    cache_bust: String,
    capsule_version: String,
    hooks_hash: String,
    claude_plugin_recipe_hash: String,
    host_identity_strategy: &'static str,
}

impl ImageRecipe {
    fn hash(&self) -> anyhow::Result<String> {
        let bytes = serde_json::to_vec(self)?;
        Ok(sha256_hex(&bytes))
    }
}

struct ExpectedImageRecipe {
    recipe: ImageRecipe,
    hash: String,
}

pub(super) struct PreparedRuntimeBinaries {
    agent_installs: BTreeMap<Agent, AgentInstall<PathBuf>>,
    prefetched_agent_versions: BTreeMap<Agent, String>,
    jackin_capsule_src: String,
}

#[expect(clippy::too_many_arguments)]
pub(super) async fn decide_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    rebuild: bool,
    branch_override: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<ImageDecision> {
    let image = branch_override.map_or_else(
        || image_name(selector),
        |branch| super::naming::image_name_for_branch(selector, branch),
    );
    if rebuild {
        emit_image_decision(&image, ImageInvalidationReason::ExplicitRebuild);
        return Ok(ImageDecision::Build {
            reason: ImageInvalidationReason::ExplicitRebuild,
            role_git_sha: None,
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
            jackin_diagnostics::debug_log!(
                "image",
                "could not list local image tags for {image}; rebuilding: {error:#}"
            );
            emit_image_decision(&image, ImageInvalidationReason::ImageListFailed);
            return Ok(ImageDecision::Build {
                reason: ImageInvalidationReason::ImageListFailed,
                role_git_sha: None,
            });
        }
    };
    if tags.is_empty() {
        emit_image_decision(&image, ImageInvalidationReason::LocalImageMissing);
        return Ok(ImageDecision::Build {
            reason: ImageInvalidationReason::LocalImageMissing,
            role_git_sha: None,
        });
    }

    jackin_diagnostics::active_timing_started("derived image", "role_git_sha", None);
    let head_sha = git_head_sha(&cached_repo.repo_dir, runner).await;
    jackin_diagnostics::active_timing_done(
        "derived image",
        "role_git_sha",
        if head_sha.is_some() {
            Some("resolved")
        } else {
            Some("unavailable")
        },
    );
    let cache_bust =
        version_check::stored_cache_bust(paths, &image).unwrap_or_else(|| "0".to_owned());
    let base_image_override = decision_base_image_override(validated_repo, branch_override);
    jackin_diagnostics::active_timing_started("derived image", "image_recipe", None);
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        agent,
        head_sha.as_deref(),
        branch_override,
        base_image_override,
        &cache_bust,
    )?;
    let mut expected_recipes = vec![ExpectedImageRecipe {
        hash: recipe.hash()?,
        recipe,
    }];
    if base_image_override.is_some() {
        let workspace_recipe = build_image_recipe(
            cached_repo,
            validated_repo,
            agent,
            head_sha.as_deref(),
            branch_override,
            None,
            &cache_bust,
        )?;
        expected_recipes.push(ExpectedImageRecipe {
            hash: workspace_recipe.hash()?,
            recipe: workspace_recipe,
        });
    }
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
            jackin_diagnostics::debug_log!(
                "image",
                "local image {image} exists but label inspection failed; rebuilding: {error:#}"
            );
            emit_image_decision(&image, ImageInvalidationReason::InspectFailed);
            return Ok(ImageDecision::Build {
                reason: ImageInvalidationReason::InspectFailed,
                role_git_sha: head_sha,
            });
        }
    };

    match classify_image_labels(&labels, &expected_recipes, agent) {
        None => {
            let selected_agent_version = labels.get(LABEL_IMAGE_SELECTED_AGENT_VERSION).cloned();
            jackin_diagnostics::debug_log!(
                "image",
                "reusing derived image {image}; recipe hash matches one current recipe"
            );
            emit_image_reuse(&image, selected_agent_version.as_deref());
            Ok(ImageDecision::Reuse {
                selected_agent_version,
                image,
            })
        }
        Some(reason) => {
            jackin_diagnostics::debug_log!(
                "image",
                "derived image {image} invalidated ({}); expected one of current recipe hashes",
                reason.as_str()
            );
            emit_image_decision(&image, reason);
            Ok(ImageDecision::Build {
                reason,
                role_git_sha: head_sha,
            })
        }
    }
}

fn decision_base_image_override<'a>(
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    branch_override: Option<&str>,
) -> Option<&'a str> {
    let custom_construct = jackin_manifest::repo_contract::construct_image()
        != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
    if branch_override.is_none() && !custom_construct {
        validated_repo.manifest.published_image.as_deref()
    } else {
        None
    }
}

#[expect(clippy::too_many_arguments)]
fn build_image_recipe(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> anyhow::Result<ImageRecipe> {
    build_image_recipe_with_construct_image(
        cached_repo,
        validated_repo,
        agent,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
        jackin_manifest::repo_contract::construct_image(),
    )
}

#[expect(clippy::too_many_arguments)]
fn build_image_recipe_with_construct_image(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
    construct_image: String,
) -> anyhow::Result<ImageRecipe> {
    let runtime_dockerfile =
        render_runtime_dockerfile(validated_repo, base_image_override, &[agent])?;
    let supported_agents = validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .map(|agent| agent.slug().to_owned())
        .collect::<Vec<_>>();

    Ok(ImageRecipe {
        version: IMAGE_RECIPE_VERSION,
        role_git_sha: head_sha.unwrap_or("unknown").to_owned(),
        role_source_ref: branch_override.map(ToOwned::to_owned),
        base_image: base_image_override.map(ToOwned::to_owned),
        construct_image,
        generated_runtime_hash: hash_str(&runtime_dockerfile),
        supported_agents,
        selected_agent: agent.slug().to_owned(),
        selected_agent_install: agent_install_recipe(agent),
        cache_bust: cache_bust.to_owned(),
        capsule_version: env!("CARGO_PKG_VERSION").to_owned(),
        hooks_hash: hooks_hash(&cached_repo.repo_dir, validated_repo)?,
        claude_plugin_recipe_hash: claude_plugin_recipe_hash(validated_repo)?,
        host_identity_strategy: HOST_IDENTITY_STRATEGY,
    })
}

fn render_runtime_dockerfile(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    base_image_override: Option<&str>,
    agents_to_install: &[Agent],
) -> anyhow::Result<String> {
    let base_dockerfile = if let Some(image) = base_image_override {
        format!("FROM {image}\n")
    } else {
        validated_repo.dockerfile.dockerfile_contents.clone()
    };
    let agent_installs = derived_agent_install_recipe(validated_repo);
    Ok(render_derived_dockerfile(
        &base_dockerfile,
        validated_repo.manifest.hooks.as_ref(),
        agents_to_install,
        validated_repo.manifest.claude.as_ref(),
        Some(".jackin-runtime/jackin-capsule"),
        &agent_installs,
    ))
}

fn derived_agent_install_recipe(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
) -> BTreeMap<Agent, AgentInstall<String>> {
    validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .map(|agent| {
            (
                agent,
                AgentInstall::Prefetched(format!(
                    ".jackin-runtime/agent-binaries/{}",
                    agent.slug()
                )),
            )
        })
        .collect()
}

fn agent_install_recipe(agent: Agent) -> String {
    hash_str(&agent.install_block(&format!(".jackin-runtime/agent-binaries/{}", agent.slug())))
}

fn hooks_hash(
    repo_dir: &std::path::Path,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
) -> anyhow::Result<String> {
    let mut entries = Vec::new();
    if let Some(hooks) = validated_repo.manifest.hooks.as_ref() {
        for hook in hooks.entries() {
            let bytes = std::fs::read(repo_dir.join(hook.path))
                .with_context(|| format!("reading {} for image recipe", hook.path))?;
            entries.push(serde_json::json!({
                "label": hook.label,
                "filename": hook.filename,
                "path": hook.path,
                "content_hash": sha256_hex(&bytes),
            }));
        }
    }
    let bytes = serde_json::to_vec(&entries)?;
    Ok(sha256_hex(&bytes))
}

fn claude_plugin_recipe_hash(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(&validated_repo.manifest.claude)?;
    Ok(sha256_hex(&bytes))
}

fn classify_image_labels(
    labels: &HashMap<String, String>,
    expected_recipes: &[ExpectedImageRecipe],
    agent: Agent,
) -> Option<ImageInvalidationReason> {
    match labels.get(LABEL_IMAGE_RECIPE_VERSION).map(String::as_str) {
        Some(IMAGE_RECIPE_VERSION) => {}
        Some(_) => return Some(ImageInvalidationReason::RecipeVersionChanged),
        None => return Some(ImageInvalidationReason::MissingRecipeLabel),
    }
    let Some(stored_hash) = labels.get(LABEL_IMAGE_RECIPE_HASH) else {
        return Some(ImageInvalidationReason::MissingRecipeLabel);
    };
    let Some(stored_agent) = labels.get(LABEL_IMAGE_SELECTED_AGENT) else {
        return Some(ImageInvalidationReason::MissingRecipeLabel);
    };
    if stored_agent != agent.slug() {
        return Some(ImageInvalidationReason::SelectedAgentChanged);
    }

    for expected in expected_recipes {
        if &expected.hash == stored_hash {
            return recipe_label_mismatch(labels, &expected.recipe);
        }
    }

    let Some(first_expected) = expected_recipes.first() else {
        return Some(ImageInvalidationReason::RecipeHashChanged);
    };
    recipe_label_mismatch(labels, &first_expected.recipe)
        .or(Some(ImageInvalidationReason::RecipeHashChanged))
}

fn recipe_label_mismatch(
    labels: &HashMap<String, String>,
    recipe: &ImageRecipe,
) -> Option<ImageInvalidationReason> {
    for (key, expected, reason) in recipe_diagnostic_labels(recipe) {
        let Some(stored) = labels.get(key) else {
            return Some(ImageInvalidationReason::MissingRecipeLabel);
        };
        if stored != &expected {
            return Some(reason);
        }
    }
    None
}

fn emit_image_decision(image: &str, reason: ImageInvalidationReason) {
    jackin_diagnostics::debug_log!(
        "image",
        "derived image {image} requires build: {}",
        reason.as_str()
    );
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "image_cache_miss",
            "derived image",
            &format!("derived image {image} requires build"),
            Some(reason.as_str()),
        );
    }
}

fn emit_image_reuse(image: &str, selected_agent_version: Option<&str>) {
    if let Some(run) = jackin_diagnostics::active_run() {
        let detail = serde_json::json!({
            "reason": "recipe_hash_match",
            "skipped": [
                "prepare_runtime_binaries",
                "create_derived_build_context",
                "resolve_github_token",
                "docker_build",
                "selected_agent_version_probe"
            ],
            "selected_agent_version": selected_agent_version,
        })
        .to_string();
        run.stage(
            "image_cache_hit",
            "derived image",
            &format!("reusing derived image {image}"),
            Some(&detail),
        );
    }
}

fn recipe_labels(recipe: &ImageRecipe, recipe_hash: &str) -> Vec<String> {
    let mut labels = vec![
        format!("{LABEL_IMAGE_RECIPE_VERSION}={}", recipe.version),
        format!("{LABEL_IMAGE_RECIPE_HASH}={recipe_hash}"),
    ];
    labels.extend(
        recipe_diagnostic_labels(recipe)
            .into_iter()
            .map(|(key, value, _)| format!("{key}={value}")),
    );
    labels
}

fn selected_agent_version_label(
    runtime_binaries: &PreparedRuntimeBinaries,
    agent: Agent,
) -> Option<String> {
    runtime_binaries
        .prefetched_agent_versions
        .get(&agent)
        .map(|version| format!("{LABEL_IMAGE_SELECTED_AGENT_VERSION}={version}"))
}

fn recipe_diagnostic_labels(
    recipe: &ImageRecipe,
) -> Vec<(&'static str, String, ImageInvalidationReason)> {
    vec![
        (
            LABEL_IMAGE_ROLE_GIT_SHA,
            recipe.role_git_sha.clone(),
            ImageInvalidationReason::RoleGitShaChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_ROLE_SOURCE_REF,
            recipe.role_source_ref.clone().unwrap_or_default(),
            ImageInvalidationReason::RoleSourceRefChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_BASE_IMAGE,
            recipe.base_image.clone().unwrap_or_default(),
            ImageInvalidationReason::BaseImageChanged,
        ),
        (
            LABEL_IMAGE_CONSTRUCT,
            recipe.construct_image.clone(),
            ImageInvalidationReason::ConstructImageChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_GENERATED_RUNTIME,
            recipe.generated_runtime_hash.clone(),
            ImageInvalidationReason::GeneratedRuntimeChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_SUPPORTED_AGENTS,
            hash_str(&recipe.supported_agents.join(",")),
            ImageInvalidationReason::SupportedAgentsChanged,
        ),
        (
            LABEL_IMAGE_SELECTED_AGENT,
            recipe.selected_agent.clone(),
            ImageInvalidationReason::SelectedAgentChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_SELECTED_AGENT_INSTALL,
            recipe.selected_agent_install.clone(),
            ImageInvalidationReason::SelectedAgentInstallChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_CACHE_BUST,
            recipe.cache_bust.clone(),
            ImageInvalidationReason::CacheBustChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_CAPSULE_VERSION,
            recipe.capsule_version.clone(),
            ImageInvalidationReason::CapsuleVersionChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_HOOKS,
            recipe.hooks_hash.clone(),
            ImageInvalidationReason::HooksHashChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_CLAUDE_PLUGIN,
            recipe.claude_plugin_recipe_hash.clone(),
            ImageInvalidationReason::ClaudePluginRecipeChanged,
        ),
        (
            LABEL_IMAGE_RECIPE_HOST_IDENTITY_STRATEGY,
            recipe.host_identity_strategy.to_owned(),
            ImageInvalidationReason::HostIdentityStrategyChanged,
        ),
    ]
}

#[cfg(test)]
pub(crate) fn image_recipe_label_map_for_test(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> HashMap<String, String> {
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        agent,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
    )
    .expect("test image recipe should build");
    let recipe_hash = recipe.hash().expect("test image recipe should hash");
    recipe_labels(&recipe, &recipe_hash)
        .into_iter()
        .filter_map(|label| {
            let (key, value) = label.split_once('=')?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

#[cfg(test)]
fn expected_image_recipe_for_test(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> ExpectedImageRecipe {
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        agent,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
    )
    .expect("test image recipe should build");
    let hash = recipe.hash().expect("test image recipe should hash");
    ExpectedImageRecipe { recipe, hash }
}

fn hash_str(input: &str) -> String {
    sha256_hex(input.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
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

    // Resolve + download all agent binaries and jackin-capsule concurrently.
    // Each ensure_available call is network-bound (HTTP resolve + optional download),
    // so running them in parallel cuts wall-clock time to the slowest single binary
    // rather than the sum of all.
    //
    // Derived image ENTRYPOINT is `/jackin/runtime/jackin-capsule`, so a missing
    // capsule binary would produce an opaque "exec: file not found" at `docker run`.
    // Failing fast here gives an actionable error message.
    let agent_futures = agents.into_iter().map(|agent| async move {
        let timing_name = format!("ensure_{}_binary", agent.slug());
        jackin_diagnostics::active_timing_started("agent binaries", &timing_name, None);
        match jackin_image::agent_binary::ensure_available(paths, agent).await {
            Ok(binary) => {
                jackin_diagnostics::active_timing_done(
                    "agent binaries",
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
                    "agent binaries",
                    &timing_name,
                    Some("script fallback"),
                );
                jackin_diagnostics::emit_compact_line(
                    "warning",
                    &format!(
                        "[jackin] could not resolve or download the hard-coded {} binary; the upstream release layout may have changed or the server may be unavailable, so the Docker build will run fallback installer `{}`: {error:#}",
                        agent.slug(),
                        agent.fallback_install_command()
                    ),
                );
                Ok((agent, AgentInstall::ScriptFallback, None))
            }
        }
    });
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

    let (agent_install_pairs, jackin_capsule_binary) = if let Some(p) = &mut progress {
        p.while_waiting(async { tokio::try_join!(try_join_all(agent_futures), capsule_future) })
            .await?
    } else {
        tokio::try_join!(try_join_all(agent_futures), capsule_future)?
    };
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

/// Build the Docker image for the role. Returns the image name.
#[expect(
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub(super) async fn build_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    runtime_binaries: PreparedRuntimeBinaries,
    rebuild: bool,
    agent_update: bool,
    debug: bool,
    branch_override: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
    known_head_sha: Option<&str>,
    progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<String> {
    // Decide the build mode up front.
    //
    // Pre-built mode: the manifest declares a `published_image` and the
    // caller has not passed `--rebuild`. The heavy workspace layers (apt
    // installs, Rust toolchain, etc.) are already baked into that image; we
    // only need to layer the agent install on top.
    //
    // Workspace mode: either `--rebuild` was requested or no `published_image`
    // is declared. We build from the workspace Dockerfile from scratch.
    let published_image = validated_repo.manifest.published_image.as_deref();
    // Branch builds always use the workspace Dockerfile regardless of
    // `published_image` — the operator is testing uncommitted code that has
    // not been pushed to the registry.
    // Skip the pre-built image when JACKIN_CONSTRUCT_IMAGE points at a local
    // build: the published image was built against the canonical construct, so
    // using it as base would silently ignore the local construct override.
    let custom_construct = jackin_manifest::repo_contract::construct_image()
        != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
    let mut use_prebuilt =
        published_image.is_some() && !rebuild && branch_override.is_none() && !custom_construct;
    let mut base_image_override = if use_prebuilt { published_image } else { None };

    // Resolve the role repo HEAD SHA once — used for the published-image
    // staleness check, the local-image freshness check, and as a build-arg
    // so local builds carry the same label.
    let head_sha = match known_head_sha {
        Some(sha) => Some(sha.to_owned()),
        None => git_head_sha(&cached_repo.repo_dir, runner).await,
    };

    // Compute the local workspace tag early so the local-freshness check
    // below can read its labels before we commit to a rebuild.
    let local_image_name = branch_override.map_or_else(
        || image_name(selector),
        |b| super::naming::image_name_for_branch(selector, b),
    );

    // When using the pre-built published image, check whether it is current:
    // - Primary check: `jackin.role_git_sha` label matches the HEAD of the
    //   cached role repo → image was built from the exact same commit, fresh.
    // - Fallback (images predating this feature): `jackin.construct_version`
    //   label matches the Dockerfile's pinned version → still usable.
    //
    // When the published image is stale, do NOT rebuild blindly — the local
    // workspace image from a previous `docker build` may already carry the
    // correct `jackin.role_git_sha` label. Without this short-circuit, every
    // launch declares "published image is out of date" and busts the Claude
    // install layer via a fresh `JACKIN_CACHE_BUST` timestamp, even when
    // nothing in the role repo or agent version has actually changed.
    let rebuild = if let Some(published) = published_image.filter(|_| use_prebuilt) {
        if published_image_is_stale(
            published,
            &validated_repo.dockerfile.construct_version,
            head_sha.as_deref(),
            docker,
        )
        .await
        {
            let local_is_fresh = match head_sha.as_deref() {
                Some(sha) => docker
                    .inspect_image_label(&local_image_name, LABEL_IMAGE_ROLE_GIT_SHA)
                    .await
                    .unwrap_or(None)
                    .is_some_and(|cached| cached == sha),
                None => false,
            };
            if local_is_fresh {
                jackin_diagnostics::debug_log!(
                    "image",
                    "published image {published} is out of date; reusing local workspace image {local_image_name} (role SHA matches)"
                );
                use_prebuilt = false;
                base_image_override = None;
                rebuild
            } else {
                jackin_diagnostics::debug_log!(
                    "image",
                    "published image {published} is out of date; rebuilding from workspace Dockerfile"
                );
                use_prebuilt = false;
                base_image_override = None;
                true
            }
        } else {
            rebuild
        }
    } else {
        rebuild
    };

    // create_derived_build_context copies the repo into a temp directory,
    // creating an immutable snapshot.  After this point the shared cached
    // repo can be safely modified by a parallel load.
    jackin_diagnostics::active_timing_started("derived image", "create_build_context", None);
    let agents_to_install = [agent];
    let build_result = create_derived_build_context_for_agents(
        &cached_repo.repo_dir,
        validated_repo,
        base_image_override,
        Some(&runtime_binaries.jackin_capsule_src),
        &runtime_binaries.agent_installs,
        &agents_to_install,
    );
    let build = match build_result {
        Ok(build) => {
            jackin_diagnostics::active_timing_done(
                "derived image",
                "create_build_context",
                Some("created"),
            );
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

    if debug {
        let dockerfile_body = std::fs::read_to_string(&build.dockerfile_path)
            .unwrap_or_else(|e| format!("<read failed: {e}>"));
        jackin_diagnostics::emit_debug_line(
            "image",
            &format!(
                "DerivedDockerfile ({}):\n{dockerfile_body}",
                build.dockerfile_path.display(),
            ),
        );
    }
    let image = local_image_name.clone();

    let build_arg_role_git_sha =
        format!("ROLE_GIT_SHA={}", head_sha.as_deref().unwrap_or("unknown"));
    // Always pass the cache-bust arg so Docker matches the correct layer.
    //
    // When rebuilding (update available / --rebuild), generate a fresh
    // timestamp to invalidate the cached agent install layer, and persist it
    // so subsequent non-rebuild builds reuse the same layer.
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
    let construct_mismatch = if rebuild {
        false
    } else {
        docker
            .inspect_image_label(&image, LABEL_IMAGE_CONSTRUCT)
            .await
            .unwrap_or(None)
            .is_some_and(|cached| cached != current_construct)
    };
    let rebuild = rebuild || construct_mismatch;

    let cache_bust_value = if rebuild || agent_update {
        // System clock before UNIX_EPOCH is essentially impossible, but if it
        // happens we must not silently fall back to 0 — that collapses to the
        // Dockerfile's `JACKIN_CACHE_BUST=0` default and defeats the operator's
        // explicit `--rebuild` request.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system clock is before UNIX epoch: {e}"))?
            .as_secs()
            .to_string();
        version_check::store_cache_bust(paths, &image, &ts);
        ts
    } else {
        version_check::stored_cache_bust(paths, &image).unwrap_or_else(|| "0".to_owned())
    };
    let cache_bust = format!("JACKIN_CACHE_BUST={cache_bust_value}");
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        agent,
        head_sha.as_deref(),
        branch_override,
        base_image_override,
        &cache_bust_value,
    )?;
    let recipe_hash = recipe.hash()?;
    let mut recipe_labels = recipe_labels(&recipe, &recipe_hash);
    if let Some(label) = selected_agent_version_label(&runtime_binaries, agent) {
        recipe_labels.push(label);
    }

    let mut build_args: Vec<&str> = vec!["build"];

    // --pull semantics:
    //
    // Pre-built mode: pass --pull so Docker always checks the registry for an
    // updated published image. A pull with an unchanged digest is a fast
    // no-op, so this adds negligible overhead while ensuring the local daemon
    // picks up any newly pushed workspace image.
    //
    // Workspace mode with rebuild=true (explicit --rebuild or staleness-driven
    // fallback): pass --pull to refresh the upstream construct base before
    // rebuilding from the workspace Dockerfile.
    //
    // Workspace mode without rebuild (no published_image): omit --pull so
    // Docker's layer cache is respected across invocations. The base image is
    // not re-evaluated and heavy apt / toolchain layers stay cached.
    if use_prebuilt || rebuild {
        build_args.push("--pull");
    }

    build_args.extend(["--build-arg", &cache_bust]);
    build_args.extend(["--build-arg", &build_arg_role_git_sha]);
    for label in &recipe_labels {
        build_args.extend(["--label", label]);
    }
    build_args.extend(["-t", &image, "-f", &dockerfile_path, &context_dir]);

    let dockerfile_requests_github_token =
        dockerfile_requests_github_token_secret(&build.dockerfile_path);
    jackin_diagnostics::active_timing_started("derived image", "resolve_github_token", None);
    let github_token = if dockerfile_requests_github_token {
        resolve_github_token(runner).await
    } else {
        None
    };
    jackin_diagnostics::active_timing_done(
        "derived image",
        "resolve_github_token",
        if !dockerfile_requests_github_token {
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

    // Tee the build's captured output into the live build-log sink so the
    // loading cockpit can show it on demand (the build is the slowest step).
    // `end` stops teeing but keeps the captured lines for the dialog.
    //
    // `build_log::end()` must always fire — even on cancellation — so the
    // process-global ACTIVE flag is reset before the next launch. The
    // `while_waiting` branch returns `Err` on cancel, which we capture in
    // `build_result` and only `?`-propagate after calling `end()`.
    jackin_launch::build_log::begin();
    jackin_diagnostics::active_timing_started("derived image", "docker_build", None);
    let build_result = runner
        .run(
            "docker",
            &build_args,
            None,
            &RunOptions {
                capture_stderr: true,
                capture_stdout: true,
                null_stdin: true,
                stream_captured_output: should_stream_build_output(debug),
                tee_to_build_log: true,
                extra_env: docker_build_env(github_token.is_some()),
                ..RunOptions::default()
            },
        )
        .await;
    jackin_launch::build_log::end();
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

fn should_stream_build_output(debug: bool) -> bool {
    !debug && !jackin_diagnostics::rich_terminal_owned()
}

fn docker_build_env(has_github_token: bool) -> Vec<(String, String)> {
    let mut env = vec![("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned())];
    if has_github_token {
        env.push(("DOCKER_BUILDKIT".to_owned(), "1".to_owned()));
    }
    env
}

fn dockerfile_requests_github_token_secret(dockerfile_path: &std::path::Path) -> bool {
    match std::fs::read_to_string(dockerfile_path) {
        Ok(body) => dockerfile_body_requests_github_token_secret(&body),
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "image",
                "could not read DerivedDockerfile {} before token lookup ({error}); resolving GitHub token conservatively",
                dockerfile_path.display()
            );
            true
        }
    }
}

fn dockerfile_body_requests_github_token_secret(dockerfile_body: &str) -> bool {
    dockerfile_body.contains("id=github_token")
}

fn emit_docker_build_step_diagnostics() {
    let Some(run) = jackin_diagnostics::active_run() else {
        return;
    };
    let path = run.command_output_path("docker-build");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    for step in parse_docker_build_steps(&contents) {
        run.docker_build_step(&step.step, &step.label, step.duration_ms, step.cached);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockerBuildStep {
    step: String,
    label: String,
    duration_ms: Option<u64>,
    cached: bool,
}

fn parse_docker_build_steps(contents: &str) -> Vec<DockerBuildStep> {
    let mut labels = HashMap::new();
    let mut steps = Vec::new();
    for line in contents.lines() {
        let Some((step, rest)) = parse_buildkit_line(line) else {
            continue;
        };
        if is_buildkit_step_description(rest, labels.contains_key(&step)) {
            labels.insert(step.clone(), rest.to_owned());
            continue;
        }
        if let Some(completed) = parse_completed_buildkit_step(&step, rest, &labels) {
            steps.push(completed);
        }
    }
    steps
}

fn parse_buildkit_line(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let (prefix, rest) = trimmed.split_once(' ')?;
    let step = prefix.strip_prefix('#')?;
    if !step.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((step.to_owned(), rest.trim()))
}

fn is_buildkit_step_description(rest: &str, has_label: bool) -> bool {
    if rest.starts_with('[') {
        return true;
    }
    !has_label
        && !matches!(split_buildkit_duration(rest).0, "DONE" | "CACHED")
        && !rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !rest.ends_with(" done")
}

fn parse_completed_buildkit_step(
    step: &str,
    rest: &str,
    labels: &HashMap<String, String>,
) -> Option<DockerBuildStep> {
    let (label, duration_ms) = split_buildkit_duration(rest);
    let completed = label == "DONE" || label == "CACHED";
    if !completed {
        return None;
    }
    let cached = label == "CACHED";
    let label = labels
        .get(step)
        .map_or_else(|| label.to_owned(), ToOwned::to_owned);
    Some(DockerBuildStep {
        step: step.to_owned(),
        label,
        duration_ms,
        cached,
    })
}

fn split_buildkit_duration(rest: &str) -> (&str, Option<u64>) {
    let Some((label, duration)) = rest.rsplit_once(' ') else {
        return (rest, None);
    };
    let Some(duration_ms) = parse_buildkit_duration_ms(duration) else {
        return (rest, None);
    };
    (label.trim_end(), Some(duration_ms))
}

fn parse_buildkit_duration_ms(value: &str) -> Option<u64> {
    let seconds = value.strip_suffix('s')?;
    let (whole, fraction) = seconds.split_once('.').map_or((seconds, ""), |parts| parts);
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole_ms = whole.parse::<u64>().ok()?.checked_mul(1000)?;
    let fraction_ms = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()?.checked_mul(100)?,
        2 => fraction.parse::<u64>().ok()?.checked_mul(10)?,
        _ => fraction[..3].parse::<u64>().ok()?,
    };
    whole_ms.checked_add(fraction_ms)
}

fn emit_compact_image_warning(message: &str) {
    jackin_diagnostics::emit_compact_line("warning", &compact_image_warning_line(message));
}

fn compact_image_warning_line(message: &str) -> String {
    format!("jackin: warning: {message}")
}

/// Returns `true` when the published image is out of date relative to the
/// current role repo state.
///
/// Checks in order:
/// 1. `jackin.role_git_sha` label: if present and matches `head_sha`, the
///    image was built from the exact same commit — fresh, no rebuild needed.
///    If present and different, the image is stale.
/// 2. Fallback for images predating role-git-sha tracking:
///    `jackin.construct_version` label must match `dockerfile_version`.
///    Absent label is treated as fresh (backward compatibility).
///
/// If `docker pull` fails the image may not exist locally at all. Treating a
/// missing image as "not stale" would let the prebuilt path proceed and produce
/// a confusing late failure inside `docker build`. Return `true` (stale) so
/// jackin falls back to workspace mode, which gives the operator a clearer
/// error if the construct base is also unreachable.
async fn published_image_is_stale(
    published: &str,
    dockerfile_version: &str,
    head_sha: Option<&str>,
    docker: &impl DockerApi,
) -> bool {
    if let Err(e) = docker.pull_image(published).await {
        emit_compact_image_warning(&format!(
            "docker pull {published} failed ({e}); treating published image as stale and rebuilding from workspace Dockerfile"
        ));
        return true;
    }

    let labels = match docker.inspect_image_labels(published).await {
        Err(e) => {
            emit_compact_image_warning(&format!(
                "could not read labels from {published} ({e}); treating published image as stale"
            ));
            return true;
        }
        Ok(map) => map,
    };

    if let Some(sha) = head_sha {
        match labels.get(LABEL_IMAGE_ROLE_GIT_SHA).map(String::as_str) {
            Some(label_sha) if label_sha == sha => return false,
            Some(_) => return true,
            None => {}
        }
    }

    // Fallback: construct-version check for pre-role-git-sha images.
    labels
        .get(LABEL_IMAGE_CONSTRUCT_VERSION)
        .is_some_and(|stored| stored != dockerfile_version)
}

// Phase 2: collapsed from 5-arm match to AgentRuntime adapter dispatch.
// `runtime().label()` → display label; `runtime().parse_version(raw)` → semver parse;
// `version_check::store_version(paths, agent, image, version)` → unified store.
async fn extract_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: Agent,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    let runtime = agent.runtime();
    let slug = agent.slug();
    jackin_diagnostics::active_timing_started(
        "derived image",
        "selected_agent_version_probe",
        Some(slug),
    );
    let raw_result = runner
        .capture(
            "docker",
            &["run", "--rm", "--entrypoint", slug, image, "--version"],
            None,
        )
        .await;
    let Ok(raw) = raw_result else {
        jackin_diagnostics::active_timing_done(
            "derived image",
            "selected_agent_version_probe",
            Some("error"),
        );
        if debug {
            jackin_diagnostics::emit_debug_line(
                "image",
                &format!(
                    "could not probe {} version from {image}; version check skipped",
                    runtime.label()
                ),
            );
        }
        return;
    };
    jackin_diagnostics::active_timing_done(
        "derived image",
        "selected_agent_version_probe",
        Some("probed"),
    );
    let version = raw.trim();
    if version.is_empty() {
        return;
    }
    if debug {
        jackin_diagnostics::emit_debug_line("image", &format!("{} {version}", runtime.label()));
    }
    if let Some(semver) = runtime.parse_version(version) {
        version_check::store_version(paths, agent, image, semver);
    } else if debug {
        jackin_diagnostics::emit_debug_line(
            "image",
            &format!("unexpected {slug} --version output: {version:?}"),
        );
    }
}

async fn record_built_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: Agent,
    runtime_binaries: &PreparedRuntimeBinaries,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    if matches!(
        runtime_binaries.agent_installs.get(&agent),
        Some(AgentInstall::Prefetched(_))
    ) {
        if let Some(version) = runtime_binaries.prefetched_agent_versions.get(&agent) {
            jackin_diagnostics::active_timing_started(
                "derived image",
                "selected_agent_version_probe",
                Some(agent.slug()),
            );
            jackin_diagnostics::active_timing_done(
                "derived image",
                "selected_agent_version_probe",
                Some("prefetched"),
            );
            version_check::store_version(paths, agent, image, version);
            if debug {
                jackin_diagnostics::emit_debug_line(
                    "image",
                    &format!(
                        "{} {version} recorded from prefetched binary metadata; Docker probe skipped",
                        agent.runtime().label()
                    ),
                );
            }
            return;
        }
    }
    extract_agent_version(paths, image, agent, debug, runner).await;
}

/// Resolves a GitHub token for authenticating mise's GitHub API calls during
/// Docker image builds. Checks `GITHUB_TOKEN` and `GH_TOKEN` env vars first
/// (set in CI and by operators), then falls back to `gh auth token` for local
/// development where the user is already logged in via the gh CLI.
///
/// Returns `None` when no token is available; callers must degrade gracefully
/// (build still works, mise falls back to unauthenticated GitHub API access).
async fn resolve_github_token(runner: &mut impl CommandRunner) -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Some(t) = std::env::var(var).ok().filter(|t| !t.trim().is_empty()) {
            return Some(t.trim().to_owned());
        }
    }
    match runner.capture_secret("gh", &["auth", "token"], None).await {
        Ok(s) => {
            let s = s.trim().to_owned();
            (!s.is_empty()).then_some(s)
        }
        Err(e) => {
            jackin_diagnostics::debug_log!("github_token", "gh auth token failed (no token): {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests;
