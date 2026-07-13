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
use crate::{DindGrant, DockerSecurityProfile};

/// Top-level role manifest parsed from `jackin.role.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleManifest {
    /// Manifest schema version string.
    #[serde(default = "current_manifest_version", rename = "version")]
    pub version: String,
    /// Path to the role Dockerfile (relative to the role root).
    pub dockerfile: String,
    /// Pre-built Docker image published to a registry. When set, `jackin
    /// console` pulls this image and layers only the agent install on top,
    /// skipping the full workspace Dockerfile build. Pass `--rebuild` to
    /// force a local rebuild from the Dockerfile instead.
    #[serde(default)]
    pub published_image: Option<String>,
    /// Optional `[identity]` table (display name, …).
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    /// Top-level list of supported agents. `None` means the field
    /// was omitted, which `supported_agents()` treats as
    /// claude-only (the implicit default). `Some(empty)` is
    /// rejected by validate as a user error.
    #[serde(default)]
    pub agents: Option<Vec<Agent>>,
    /// Optional `[claude]` agent table.
    #[serde(default)]
    pub claude: Option<ClaudeConfig>,
    /// Optional `[codex]` agent table.
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    /// Optional `[amp]` agent table.
    #[serde(default)]
    pub amp: Option<AmpConfig>,
    /// Optional `[kimi]` agent table.
    #[serde(default)]
    pub kimi: Option<KimiConfig>,
    /// Optional `[opencode]` agent table.
    #[serde(default)]
    pub opencode: Option<OpencodeConfig>,
    /// Optional `[grok]` agent table.
    #[serde(default)]
    pub grok: Option<GrokConfig>,
    /// Optional lifecycle hook scripts.
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    /// Declared interactive/default env vars for the role.
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
    /// Optional `[docker]` security settings.
    #[serde(default)]
    pub docker: Option<ManifestDockerConfig>,
}

/// Docker security settings a role author can declare in `[docker]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDockerConfig {
    /// Minimum Docker security profile this role requires.
    #[serde(default)]
    pub min_profile: Option<DockerSecurityProfile>,
    /// Docker-in-Docker grant requested by the role.
    #[serde(default)]
    pub dind: Option<DindGrant>,
    /// Extra hostnames allowed under allowlist networking.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Extra Linux capabilities to add to the container.
    #[serde(default)]
    pub capabilities_add: Vec<String>,
}

impl RoleManifest {
    /// Display name from `[identity].name`, or `fallback` when absent.
    pub fn display_name(&self, fallback: &str) -> String {
        self.identity
            .as_ref()
            .map_or_else(|| fallback.to_owned(), |id| id.name.clone())
    }

    /// Returns the agents this manifest supports. Legacy manifests
    /// without an `agents` field default to claude-only.
    pub fn supported_agents(&self) -> Vec<Agent> {
        self.agents.clone().unwrap_or_else(|| vec![Agent::Claude])
    }

    /// Returns `true` when the manifest has a `[<agent>]` table declared.
    ///
    /// Used by `jackin_manifest::validate` to check the agent/config-table
    /// consistency rule without a per-caller match arm. This named-field
    /// accessor is the schema-preserving exception until role manifests move to
    /// a schema-bumped agent map.
    pub const fn has_agent_config(&self, agent: Agent) -> bool {
        match agent {
            Agent::Claude => self.claude.is_some(),
            Agent::Codex => self.codex.is_some(),
            Agent::Amp => self.amp.is_some(),
            Agent::Kimi => self.kimi.is_some(),
            Agent::Opencode => self.opencode.is_some(),
            Agent::Grok => self.grok.is_some(),
        }
    }

    /// Returns the per-agent model override from the manifest, if any.
    ///
    /// Same named-field exception as `has_agent_config`: role manifests expose
    /// `[claude]`/`[codex]` tables today, so call sites route through this one
    /// accessor rather than matching over `Agent` themselves.
    pub fn agent_model(&self, agent: Agent) -> Option<&str> {
        match agent {
            Agent::Claude => self.claude.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Codex => self.codex.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Amp => None,
            Agent::Kimi => self.kimi.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Opencode => self.opencode.as_ref().and_then(|c| c.model.as_deref()),
            Agent::Grok => self.grok.as_ref().and_then(|c| c.model.as_deref()),
        }
    }

    /// Per-(agent, provider) model override from the manifest, if the role set
    /// one under `[<agent>.providers.<provider_id>]`. `provider_id` is the
    /// provider's stable lowercase slug (e.g. `minimax`). Only `claude`, `codex`,
    /// and `opencode` carry provider tables; other agents always return `None`.
    pub fn agent_provider_model(&self, agent: Agent, provider_id: &str) -> Option<&str> {
        let providers = match agent {
            Agent::Claude => &self.claude.as_ref()?.providers,
            Agent::Codex => &self.codex.as_ref()?.providers,
            Agent::Opencode => &self.opencode.as_ref()?.providers,
            Agent::Amp | Agent::Kimi | Agent::Grok => return None,
        };
        providers.get(provider_id)?.model.as_deref()
    }

    /// Every `(provider_id, model)` override declared for `agent`, used by the
    /// host to populate the capsule's provider-model map. Empty when the agent
    /// has no provider tables or none of them set a model.
    pub fn agent_provider_models(&self, agent: Agent) -> Vec<(&str, &str)> {
        let providers = match agent {
            Agent::Claude => self.claude.as_ref().map(|c| &c.providers),
            Agent::Codex => self.codex.as_ref().map(|c| &c.providers),
            Agent::Opencode => self.opencode.as_ref().map(|c| &c.providers),
            Agent::Amp | Agent::Kimi | Agent::Grok => None,
        };
        providers
            .into_iter()
            .flatten()
            .filter_map(|(id, override_)| override_.model.as_deref().map(|m| (id.as_str(), m)))
            .collect()
    }
}

/// Per-provider model override, keyed by the provider's stable lowercase slug
/// (e.g. `minimax`) under `[<agent>.providers.<id>]`. Applied when that provider
/// is the selected provider for the agent — `OpenCode` has no model of its own and
/// Claude/Codex map a selected provider to a model, so a role can pin a different
/// model per provider without changing the agent's default `model`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderModelOverride {
    /// Model id to use when this provider is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Per-role Codex configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// Optional model override; passed to Codex with `-m` when present,
    /// otherwise Codex's own default is used.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-provider model overrides keyed by provider slug.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderModelOverride>,
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
    /// Optional model override for Kimi (`--model`).
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
    /// Optional default model in `provider/model` form.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-provider model overrides keyed by provider slug.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderModelOverride>,
}

/// Per-role Grok Build configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrokConfig {
    /// Optional model override; passed to Grok with `-m` when present.
    #[serde(default)]
    pub model: Option<String>,
}

/// One declared role env var under `[env.<name>]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDecl {
    /// Default value when the operator does not supply one.
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    /// Whether launch should prompt the operator for this var.
    #[serde(default)]
    pub interactive: bool,
    /// Whether the operator may skip providing a value.
    #[serde(default)]
    pub skippable: bool,
    /// Optional prompt text shown during interactive resolution.
    pub prompt: Option<String>,
    /// Optional fixed choice list for interactive selection.
    #[serde(default)]
    pub options: Vec<String>,
    /// Dependency names (`env.OTHER`) that must resolve first.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Lifecycle hook script paths under `[hooks]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    /// Repo-relative path to the setup-once hook script.
    #[serde(default)]
    pub setup_once: Option<String>,
    /// Repo-relative path to the per-start source hook script.
    #[serde(default)]
    pub source: Option<String>,
    /// Repo-relative path to the preflight hook script.
    #[serde(default)]
    pub preflight: Option<String>,
}

/// Centralizes the (label, in-image filename, repo-relative path) triple
/// so repo validation, Dockerfile rendering, and `.dockerignore`
/// allowlisting cannot disagree about a hook's identity.
#[derive(Debug, Clone, Copy)]
pub struct HookEntry<'a> {
    /// Operator-facing label (`"setup_once hook"`, …).
    pub label: &'static str,
    /// Filename used inside the image (`setup-once.sh`, …).
    pub filename: &'static str,
    /// Repo-relative path from the manifest.
    pub path: &'a str,
}

impl HooksConfig {
    /// Iterate configured hooks in entrypoint runtime contract order.
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

/// Role identity table (`[identity]`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// Human-readable display name for the role.
    pub name: String,
}

/// One Claude Code marketplace source declaration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeMarketplaceConfig {
    /// Marketplace source URL or path.
    pub source: String,
    /// Optional sparse-checkout paths within the marketplace repo.
    #[serde(default)]
    pub sparse: Vec<String>,
}

/// Per-role Claude Code configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    /// Optional model override; passed to Claude Code with `--model`
    /// when present, otherwise Claude Code's own default is used.
    #[serde(default)]
    pub model: Option<String>,
    /// Claude Code marketplaces to register.
    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,
    /// Plugin identifiers to enable.
    #[serde(default)]
    pub plugins: Vec<String>,
    /// Per-provider model overrides keyed by provider slug.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderModelOverride>,
}

/// Non-fatal warning collected while loading or validating a manifest.
#[derive(Debug, Clone)]
pub struct ManifestWarning {
    /// Operator-facing warning text.
    pub message: String,
}

impl ManifestWarning {
    /// Construct a warning from any string-like message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
