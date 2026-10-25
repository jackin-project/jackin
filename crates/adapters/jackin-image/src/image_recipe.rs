// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_manifest::repo::CachedRepo;

use crate::capsule_binary;
use crate::derived_image::{AgentInstall, render_derived_dockerfile};
use crate::naming::{
    HOST_IDENTITY_STRATEGY, LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT,
    LABEL_IMAGE_MANIFEST_VERSION, LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION,
    LABEL_IMAGE_ROLE_GIT_SHA,
};

pub const IMAGE_RECIPE_VERSION: &str = "v10";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageRecipe {
    pub(crate) version: &'static str,
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
    pub(crate) plugin_recipe_hash: String,
    pub(crate) host_identity_strategy: &'static str,
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
        role_git_sha: head_sha
            .map_or("unknown", crate::naming::short_git_sha)
            .to_owned(),
        role_source_ref: branch_override.map(ToOwned::to_owned),
        base_image: base_image_override.map(ToOwned::to_owned),
        construct_image,
        generated_runtime_hash: hash_str(&runtime_dockerfile),
        supported_agents,
        cache_bust: cache_bust.to_owned(),
        capsule_version: capsule_binary::REQUIRED_VERSION.to_owned(),
        hooks_hash: hooks_hash(&cached_repo.repo_dir, validated_repo)?,
        plugin_recipe_hash: plugin_recipe_hash(validated_repo),
        host_identity_strategy: HOST_IDENTITY_STRATEGY,
        host_uid: host_uid(),
    })
}

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

fn canonical_supported_agent_slugs(manifest: &jackin_core::RoleManifest) -> Vec<String> {
    let mut agents = manifest
        .supported_agents()
        .into_iter()
        .map(|agent| agent.slug().to_owned())
        .collect::<Vec<_>>();
    agents.sort();
    agents
}

pub fn supported_set_uses_cache_bust(manifest: &jackin_core::RoleManifest) -> bool {
    manifest.supported_agents().iter().any(|agent| {
        matches!(
            agent,
            Agent::Claude
                | Agent::Grok
                | Agent::Gemini
                | Agent::Cursor
                | Agent::Muse
                | Agent::Omp
                | Agent::Hermes
        )
    })
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

#[expect(clippy::expect_used, reason = "test-only API surface")]
#[expect(
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

#[expect(clippy::expect_used, reason = "test-only API surface")]
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
