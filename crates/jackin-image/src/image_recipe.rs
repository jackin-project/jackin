// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ImageRecipe` value type and Dockerfile/recipe generation helpers.
//!
//! Architecture Invariant: L1 application crate module. Depends on
//! `jackin-core`, `jackin-manifest`, `jackin-image` siblings
//! (`capsule_binary`, `derived_image`, `naming`). Pure value-type
//! machinery — no Docker calls, no async — so the recipe decision
//! path stays synchronous and unit-testable.

use anyhow::Context as _;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_manifest::repo::CachedRepo;

use crate::capsule_binary;
use crate::derived_image::{AgentInstall, render_derived_dockerfile};
use crate::naming::{
    HOST_IDENTITY_STRATEGY, LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT,
    LABEL_IMAGE_MANIFEST_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION,
    LABEL_IMAGE_ROLE_GIT_SHA,
};

// Bumped v7 -> v8: derived images now bake the host UID into `/home/agent`
// ownership. Group-0 write is not enough for tools that call owner-only
// syscalls such as chmod(2) while running under `--user UID:GID`.
pub const IMAGE_RECIPE_VERSION: &str = "v8";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageRecipe {
    pub(crate) version: &'static str,
    /// `jackin.role.toml` schema version (e.g. `v1alpha4`). A manifest-schema
    /// bump changes what jackin generates, so it is a recipe input.
    pub(crate) manifest_version: String,
    pub(crate) role_git_sha: String,
    pub(crate) role_source_ref: Option<String>,
    pub(crate) base_image: Option<String>,
    pub(crate) construct_image: String,
    pub(crate) generated_runtime_hash: String,
    pub(crate) supported_agents: Vec<String>,
    pub(crate) cache_bust: String,
    pub(crate) capsule_version: String,
    pub(crate) hooks_hash: String,
    /// SHA-256 of the serialised Claude marketplaces + plugins list. Plugin
    /// changes rebuild the image because plugins are now baked at build time (D2).
    pub(crate) plugin_recipe_hash: String,
    pub(crate) host_identity_strategy: &'static str,
    /// Host UID that will run the container. Public so the
    /// `host_uid_changes_recipe_hash` integration test can mutate the field to
    /// assert that UID changes participate in the derived-image recipe hash.
    pub host_uid: Option<u32>,
}

impl ImageRecipe {
    pub fn hash(&self) -> anyhow::Result<String> {
        let bytes = serde_json::to_vec(self)?;
        Ok(sha256_hex(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedImageRecipe {
    pub recipe: ImageRecipe,
    pub hash: String,
}

pub fn build_image_recipe(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> anyhow::Result<ImageRecipe> {
    build_image_recipe_with_construct_image(
        cached_repo,
        validated_repo,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
        jackin_manifest::repo_contract::construct_image(),
    )
}

#[cfg(unix)]
fn host_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

#[cfg(not(unix))]
fn host_uid() -> Option<u32> {
    None
}

pub fn build_image_recipe_with_construct_image(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
    construct_image: String,
) -> anyhow::Result<ImageRecipe> {
    let runtime_dockerfile = render_runtime_dockerfile(validated_repo, base_image_override)?;
    let supported_agents = canonical_supported_agent_slugs(&validated_repo.manifest);

    Ok(ImageRecipe {
        version: IMAGE_RECIPE_VERSION,
        manifest_version: validated_repo.manifest.version.clone(),
        // Short (7-char) role-repo commit SHA — matches the image tag and the
        // `jackin.role.git.sha` label form.
        role_git_sha: head_sha
            .map_or("unknown", crate::naming::short_git_sha)
            .to_owned(),
        role_source_ref: branch_override.map(ToOwned::to_owned),
        base_image: base_image_override.map(ToOwned::to_owned),
        construct_image,
        generated_runtime_hash: hash_str(&runtime_dockerfile),
        supported_agents,
        cache_bust: cache_bust.to_owned(),
        // Key on the same SHA-suffixed JACKIN_VERSION the capsule binary is
        // selected by (capsule_binary::REQUIRED_VERSION), not CARGO_PKG_VERSION:
        // two non-tag builds share a cargo version but ship different capsule
        // binaries, so CARGO_PKG_VERSION would reuse a stale capsule on dev builds.
        capsule_version: capsule_binary::REQUIRED_VERSION.to_owned(),
        hooks_hash: hooks_hash(&cached_repo.repo_dir, validated_repo)?,
        plugin_recipe_hash: plugin_recipe_hash(validated_repo),
        host_identity_strategy: HOST_IDENTITY_STRATEGY,
        host_uid: host_uid(),
    })
}

/// Render the canonical derived Dockerfile used for the image recipe hash.
///
/// Agent-independent by construction: every supported agent is installed from
/// its prefetched binary, so the hash depends only on the role's supported set
/// (plus base image, hooks, plugins, capsule version), never on which agent was
/// selected at launch. The real build may fall back to a script install for an
/// agent whose binary failed to prefetch, but that produces a functionally
/// equivalent image for the same agent set, so it is labelled with — and reused
/// against — this same canonical recipe rather than forking a new image.
fn render_runtime_dockerfile(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    base_image_override: Option<&str>,
) -> anyhow::Result<String> {
    let base_dockerfile = if let Some(image) = base_image_override {
        format!("FROM {image}\n")
    } else {
        validated_repo.dockerfile.dockerfile_contents.clone()
    };
    let agents_to_install = validated_repo.manifest.supported_agents();
    Ok(render_derived_dockerfile(
        &base_dockerfile,
        validated_repo.manifest.hooks.as_ref(),
        &agents_to_install,
        Some(".jackin-runtime/jackin-capsule"),
        &BTreeMap::new(),
        validated_repo.manifest.claude.as_ref(),
    ))
}

fn canonical_supported_agent_slugs(manifest: &jackin_core::manifest::RoleManifest) -> Vec<String> {
    let mut agents = manifest
        .supported_agents()
        .into_iter()
        .map(|agent| agent.slug().to_owned())
        .collect::<Vec<_>>();
    agents.sort();
    agents
}

/// The derived image carries a non-reproducible install step (Claude / Grok run
/// a network installer at build time) when either agent is in the supported set,
/// so the recipe folds in the stored cache-bust token to force a rebuild when
/// that token advances. Other supported sets install purely from prefetched
/// binaries and need no cache bust.
pub fn supported_set_uses_cache_bust(manifest: &jackin_core::manifest::RoleManifest) -> bool {
    manifest
        .supported_agents()
        .iter()
        .any(|agent| matches!(agent, Agent::Claude | Agent::Grok))
}

pub fn cache_bust_recipe_value(
    paths: &JackinPaths,
    image: &str,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
) -> String {
    if supported_set_uses_cache_bust(&validated_repo.manifest) {
        crate::version_check::stored_cache_bust(paths, image).unwrap_or_else(|| "0".to_owned())
    } else {
        "unused".to_owned()
    }
}

pub fn expected_image_recipes(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    paths: &JackinPaths,
    image: &str,
) -> anyhow::Result<Vec<ExpectedImageRecipe>> {
    let cache_bust = cache_bust_recipe_value(paths, image, validated_repo);
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        head_sha,
        branch_override,
        base_image_override,
        &cache_bust,
    )?;
    Ok(vec![ExpectedImageRecipe {
        hash: recipe.hash()?,
        recipe,
    }])
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

/// Hash the Claude marketplace + plugin list so changes force a rebuild (D2).
/// Empty config → stable constant hash so the field never changes for
/// roles without any Claude plugin config.
fn plugin_recipe_hash(validated_repo: &jackin_manifest::repo::ValidatedRoleRepo) -> String {
    let entry = validated_repo.manifest.claude.as_ref().map(|c| {
        serde_json::json!({
            "marketplaces": c.marketplaces.iter().map(|m| serde_json::json!({
                "source": m.source,
                "sparse": m.sparse,
            })).collect::<Vec<_>>(),
            "plugins": c.plugins,
        })
    });
    let bytes = serde_json::to_vec(&entry).unwrap_or_default();
    sha256_hex(&bytes)
}

pub fn recipe_labels(recipe: &ImageRecipe, recipe_hash: &str) -> Vec<String> {
    let mut labels = vec![
        format!("{LABEL_IMAGE_RECIPE_VERSION}={}", recipe.version),
        format!("{LABEL_IMAGE_RECIPE_HASH}={recipe_hash}"),
    ];
    labels.extend(
        recipe
            .recipe_diagnostic_label_keys()
            .into_iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    labels
}

impl ImageRecipe {
    /// Minimal, human-meaningful label set. Every other recipe input
    /// (`role_source_ref`, `base_image`, `generated_runtime_hash`,
    /// `supported_agents`, `cache_bust`, `hooks_hash`,
    /// `claude_plugin_recipe_hash`, `host_identity_strategy`)
    /// still lives inside `ImageRecipe` and so still invalidates the image via
    /// the master `jackin.image.recipe.hash` — it just no longer gets its own
    /// opaque diagnostic label. A mismatch on those surfaces as the generic
    /// `RecipeHashChanged` reason rather than a component-specific one.
    pub fn recipe_diagnostic_label_keys(&self) -> Vec<(&'static str, String)> {
        vec![
            (LABEL_IMAGE_ROLE_GIT_SHA, self.role_git_sha.clone()),
            (LABEL_IMAGE_MANIFEST_VERSION, self.manifest_version.clone()),
            (LABEL_IMAGE_CONSTRUCT, self.construct_image.clone()),
            (LABEL_IMAGE_CAPSULE_VERSION, self.capsule_version.clone()),
        ]
    }
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

/// Test-only label-map builder mirroring the runtime prewarm path. Public
/// because runtime's `#[cfg(test)]` import surfaces it across the crate
/// boundary; non-test builds never call it.
#[allow(clippy::expect_used, reason = "test-only API surface")]
pub fn image_recipe_label_map_for_test(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> std::collections::HashMap<String, String> {
    image_recipe_label_map_for_install_test(
        cached_repo,
        validated_repo,
        agent,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
        AgentInstall::Prefetched(format!(".jackin-runtime/agent-binaries/{}", agent.slug())),
    )
}

/// Test-only label-map builder that takes an explicit install variant.
#[allow(clippy::expect_used, reason = "test-only API surface")]
#[allow(
    clippy::too_many_arguments,
    reason = "Test-only API surface: passes the full recipe input set to the \
              label-map builder. Each arg is a real recipe input; bundling is a \
              parallel-pass refactor."
)]
pub fn image_recipe_label_map_for_install_test(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    _agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
    // The derived image is agent-independent; the selected agent / install
    // method no longer affect the recipe. Params retained so existing tests
    // keep compiling without churn.
    _selected_install: AgentInstall<String>,
) -> std::collections::HashMap<String, String> {
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
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

/// Test-only `ExpectedImageRecipe` builder.
#[allow(clippy::expect_used, reason = "test-only API surface")]
pub fn expected_image_recipe_for_test(
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    _agent: Agent,
    head_sha: Option<&str>,
    branch_override: Option<&str>,
    base_image_override: Option<&str>,
    cache_bust: &str,
) -> ExpectedImageRecipe {
    let recipe = build_image_recipe(
        cached_repo,
        validated_repo,
        head_sha,
        branch_override,
        base_image_override,
        cache_bust,
    )
    .expect("test image recipe should build");
    let hash = recipe.hash().expect("test image recipe should hash");
    ExpectedImageRecipe { recipe, hash }
}

#[cfg(test)]
mod tests;
