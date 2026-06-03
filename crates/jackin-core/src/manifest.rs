//! Role manifest serde types: `RoleManifest` and per-agent config structs
//! read from `jackin.role.toml`.
//!
//! Filesystem I/O (`load`), migration validation, and agent-consistency
//! validation live in the binary crate (`src/manifest/`), not here, because
//! they depend on `toml_edit`, `config::migrations`, and `env_model`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agent::Agent;
use crate::constants::current_manifest_version;

/// Top-level role manifest parsed from `jackin.role.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    #[serde(default = "current_manifest_version", rename = "version")]
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
    pub agents: Option<Vec<Agent>>,
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

impl RoleManifest {
    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_string(), |id| id.name.clone())
    }

    /// Returns the agents this manifest supports. Legacy manifests
    /// without an `agents` field default to claude-only.
    pub fn supported_agents(&self) -> Vec<Agent> {
        self.agents.clone().unwrap_or_else(|| vec![Agent::Claude])
    }
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
    pub label: &'static str,
    pub filename: &'static str,
    pub path: &'a str,
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
