//! Role manifest: `RoleManifest` serde shape and per-agent config types read
//! from `jackin.role.toml`.
//!
//! The struct represents the manifest *as parsed and migrated* by
//! `manifest/migrations.rs`. Rules that serde alone cannot enforce live in
//! `manifest/validate.rs`.
//!
//! Not responsible for: filesystem validation of the role repo (`repo.rs`),
//! migration logic (`manifest/migrations.rs`), or environment-var resolution
//! (`operator_env.rs`).

pub use crate::env_model::{JACKIN_DIND_HOSTNAME_ENV_NAME, JACKIN_ENV_NAME, JACKIN_ENV_VALUE};
use crate::repo_contract::MANIFEST_FILENAME;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod migrations;
pub mod validate;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    #[serde(default = "migrations::current_manifest_version", rename = "version")]
    pub version: String,
    pub dockerfile: String,
    /// Pre-built Docker image published to a registry. When set, `jackin
    /// console` pulls this image and layers only the agent install on top,
    /// skipping the full workspace Dockerfile build. Pass `--rebuild` to
    /// force a local rebuild from the Dockerfile instead.
    #[serde(default)]
    pub published_image: Option<String>,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    /// Top-level list of supported agents. `None` means the field
    /// was omitted, which `supported_agents()` treats as
    /// claude-only (the implicit default). `Some(empty)` is
    /// rejected by validate as a user error.
    #[serde(default)]
    pub agents: Option<Vec<crate::agent::Agent>>,
    #[serde(default)]
    pub claude: Option<ClaudeConfig>,
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    #[serde(default)]
    pub amp: Option<AmpConfig>,
    #[serde(default)]
    pub kimi: Option<KimiConfig>,
    #[serde(default)]
    pub opencode: Option<OpencodeConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// Optional model override; passed to Codex with `-m` when present,
    /// otherwise Codex's own default is used.
    #[serde(default)]
    pub model: Option<String>,
}

/// Per-role Amp configuration.
///
/// Has no fields. Declared so manifests that list
/// `agents = [..., "amp"]` can carry an `[amp]` table that satisfies
/// the agent/table consistency check.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmpConfig {}

/// Per-role Kimi configuration.
///
/// `model` is passed to Kimi with `--model` when present, otherwise
/// Kimi's own default model selection is used.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KimiConfig {
    #[serde(default)]
    pub model: Option<String>,
}

/// Per-role `OpenCode` configuration.
///
/// `model` is passed to `OpenCode` with `-m` in `provider/model` format
/// (e.g. `zai-coding-plan/glm-5.1`). When absent, `OpenCode` uses its
/// own default model selection.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpencodeConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDecl {
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub skippable: bool,
    pub prompt: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    #[serde(default)]
    pub setup_once: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub preflight: Option<String>,
}

/// Centralizes the (label, in-image filename, repo-relative path) triple
/// so repo validation, Dockerfile rendering, and `.dockerignore`
/// allowlisting cannot disagree about a hook's identity.
#[derive(Debug, Clone, Copy)]
pub struct HookEntry<'a> {
    pub(crate) label: &'static str,
    pub(crate) filename: &'static str,
    pub(crate) path: &'a str,
}

impl HooksConfig {
    pub fn entries(&self) -> impl Iterator<Item = HookEntry<'_>> {
        // Order is the entrypoint.sh runtime contract; pinned by
        // `hook_entries_yield_runtime_contract_order`.
        [
            (
                "setup_once hook",
                "setup-once.sh",
                self.setup_once.as_deref(),
            ),
            ("source hook", "source.sh", self.source.as_deref()),
            ("preflight hook", "preflight.sh", self.preflight.as_deref()),
        ]
        .into_iter()
        .filter_map(|(label, filename, path)| {
            path.map(|path| HookEntry {
                label,
                filename,
                path,
            })
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeMarketplaceConfig {
    pub source: String,
    #[serde(default)]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    /// Optional model override; passed to Claude Code with `--model`
    /// when present, otherwise Claude Code's own default is used.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,
    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ManifestWarning {
    pub message: String,
}

impl ManifestWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl RoleManifest {
    /// Parse `jackin.role.toml` and pin the load-time invariants other code
    /// relies on: `version` is not newer than this binary understands,
    /// version-gated features match the manifest's declared minimum schema,
    /// and the `agents` / `[<agent>]` tables are consistent (so
    /// `instance::prepare` can dereference `manifest.codex` /
    /// `manifest.claude` without re-checking).
    ///
    /// Env-var validation, interpolation cycle detection, and the rest of
    /// `validate()` still need explicit calls — they emit warnings the load
    /// path cannot surface.
    pub fn load(repo_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = repo_dir.join(MANIFEST_FILENAME);
        let contents = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let doc: toml_edit::DocumentMut = contents
            .parse()
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let role_name = display_role_name(repo_dir);
        let manifest_version = crate::manifest::migrations::validate_manifest_version(&doc)
            .with_context(|| format!("validating version of {}", manifest_path.display()))?;
        let manifest: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing {} as RoleManifest", manifest_path.display()))?;
        validate_feature_versions(&manifest, &manifest_version, &role_name)
            .with_context(|| format!("validating version of {}", manifest_path.display()))?;
        let _warnings = crate::manifest::validate::validate_agent_consistency(&manifest)?;
        Ok(manifest)
    }

    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_string(), |id| id.name.clone())
    }

    /// Returns the agents this manifest supports. Legacy manifests
    /// without an `agents` field default to claude-only.
    pub fn supported_agents(&self) -> Vec<crate::agent::Agent> {
        self.agents
            .clone()
            .unwrap_or_else(|| vec![crate::agent::Agent::Claude])
    }
}

fn display_role_name(repo_dir: &Path) -> String {
    let leaf = repo_dir.file_name().and_then(|name| name.to_str());
    let parent = repo_dir
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str());
    match (parent, leaf) {
        (Some(parent), Some("default" | "branches")) => parent.to_string(),
        (_, Some(name)) => name.to_string(),
        _ => repo_dir.display().to_string(),
    }
}

fn validate_feature_versions(
    manifest: &RoleManifest,
    manifest_version: &crate::config::migrations::SchemaVersion,
    role_name: &str,
) -> anyhow::Result<()> {
    let v1alpha3 = crate::config::migrations::parse_version("v1alpha3")?;
    let v1alpha4 = crate::config::migrations::parse_version("v1alpha4")?;
    if manifest_version < &v1alpha3
        && (manifest
            .agents
            .as_ref()
            .is_some_and(|agents| agents.contains(&crate::agent::Agent::Opencode))
            || manifest.opencode.is_some())
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha3 agent fields, which requires v1alpha3; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    if manifest_version < &v1alpha4
        && (manifest
            .agents
            .as_ref()
            .is_some_and(|agents| agents.contains(&crate::agent::Agent::Kimi))
            || manifest.kimi.is_some())
    {
        anyhow::bail!(
            "role \"{role_name}\" manifest is at {manifest_version} but uses v1alpha4 agent fields, which requires v1alpha4; run \"jackin role migrate <role-repo-path>\" to upgrade the local copy"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
