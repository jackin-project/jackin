//! Shared host CLI ↔ in-container Capsule contracts.
//!
//! Lives in its own crate so the host (`jackin`) and the
//! in-container binary (`jackin-capsule`) can both depend on it
//! without the host pulling in `jackin-capsule`'s tokio + PTY +
//! VT-parser stack. Most declarations here are wire-format types;
//! small constants that name the host↔Capsule runtime contract live
//! here too so the two binaries cannot drift.
//!
//! **Architecture Invariant:** L0 domain crate (wire types). Allowed
//! dependencies: `jackin-core`. Wire types stay free of presentation
//! and infrastructure concerns; DTOs and their conversions live at the
//! edges, never here.

use jackin_core::container_paths;

pub mod agent_status;
pub mod attach;
pub mod control;
pub mod provider_adapter;
pub mod snapshot;

pub use provider_adapter::ProviderAdapter;
pub use snapshot::InstanceSnapshot;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How an [`ExecBinding`]'s `source` is resolved by the host credential
/// resolver. Serializes as `"op"` / `"env"` / `"literal"`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecKind {
    /// Resolve via `op read <source>` on the host.
    Op,
    /// Read the host env var named by `source` (a `$VAR` / `${VAR}` reference).
    Env,
    /// Return `source` verbatim.
    Literal,
}

/// One on-demand credential binding the operator configured for a session.
///
/// Built host-side from the workspace's `on_demand` env entries and handed to
/// the host credential resolver (`jackin-runtime`'s `exec_host`) as the
/// allow-list of (name, kind, source) triples it will resolve.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecBinding {
    pub name: String,
    pub kind: ExecKind,
    pub source: String,
}

/// `jackin-exec` host.sock request: the operator-selected credentials the
/// in-container capsule asks the host resolver to resolve. Framed with
/// [`control::frame`], same as the control socket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredRequest {
    pub refs: Vec<ExecBinding>,
}

/// `jackin-exec` host.sock reply. Internally tagged so the capsule decodes it
/// in one parse instead of trying success-then-error struct shapes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CredReply {
    /// Every requested credential resolved: `name -> value`.
    Ok { values: BTreeMap<String, String> },
    /// Resolution failed; `error` is operator-facing (no secret material).
    Error { error: String },
}

/// Filename written under `/jackin/run/` by the host launcher.
pub const CAPSULE_CONFIG_FILENAME: &str = "agent.toml";

/// Normalized runtime config path read by Capsule PID 1.
pub const CAPSULE_CONFIG_PATH: &str = container_paths::CAPSULE_CONFIG;

/// Path inside the role container of the `jackin-exec` host credential
/// resolver socket. The host creates it under the bind-mounted `/jackin/run`
/// dir; the in-container capsule connects here to resolve on-demand
/// credentials. Single source of truth so the mount side and the connect side
/// cannot drift.
pub const HOST_SOCK_CONTAINER_PATH: &str = container_paths::HOST_SOCK;

/// Filename the capsule writes the operator's dirty-exit choice to, under the
/// per-instance state dir, for the host to read and execute on cleanup.
pub const EXIT_ACTION_FILENAME: &str = "exit-action.json";

/// In-container path the capsule writes [`ExitAction`] to. The host's state-dir
/// mount makes this readable from outside the container at
/// `<data_dir>/<container>/state/exit-action.json`.
pub const EXIT_ACTION_PATH: &str = container_paths::EXIT_ACTION;

/// The operator's choice for dirty isolated work at in-capsule exit. Decided
/// inside the capsule (the dirty-exit modal); the host only **executes** it,
/// never prompts. The capsule writes this before draining; the host reads it on
/// cleanup. Absent file means a clean exit (no dirty work) — nothing to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitAction {
    /// Preserve the instance as resumable dirty state.
    Keep,
    /// Discard the instance and its dirty work.
    Discard,
}

/// Host-validated role/session facts Capsule needs to spawn panes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapsuleConfig {
    pub role: String,
    pub workdir: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,
    /// Per-(agent, provider) model overrides from the role manifest. Outer key
    /// is the agent slug, inner key the provider's lowercase slug
    /// ([`Provider::manifest_id`]). Selects the model the capsule uses when the
    /// operator picks that provider for that agent, overriding the agent default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_models: BTreeMap<String, BTreeMap<String, String>>,
    /// When the operator picked a specific provider in the console's
    /// launch flow (before the container existed), this field tells the
    /// capsule's initial spawn to use that provider and env overrides
    /// instead of defaulting to Anthropic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_provider: Option<InitialProvider>,
    /// Claude plugin marketplaces declared by the role manifest. The capsule
    /// registers them at container start — the agent binary is mounted, not
    /// baked, so plugin setup moved out of the image build into runtime-setup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_marketplaces: Vec<ClaudeMarketplace>,
    /// Claude plugins declared by the role manifest, installed at container
    /// start by the capsule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_plugins: Vec<String>,
    /// On-demand credential bindings (`jackin-exec`). Carries the
    /// `(name, kind, source)` triples the host credential resolver allow-lists;
    /// the container only learns the names (via `JACKIN_EXEC_BINDINGS`), never
    /// resolved values. Empty when the workspace declares no on-demand vars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exec_bindings: Vec<ExecBinding>,
    /// Resolved dirty-exit policy (`"ask"` | `"keep"` | `"discard"`). The
    /// in-container daemon shows the dirty-exit modal only when this is `"ask"`;
    /// `"keep"`/`"discard"` exit straight to the host executing that policy.
    /// `None` resolves to `"ask"`. Carried as a string so `jackin-protocol` need
    /// not depend on `jackin-config`'s `DirtyExitPolicy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_exit_policy: Option<String>,
    /// Container-side paths of isolated `worktree`/`clone` mounts the daemon
    /// assesses for dirty/unpushed work at last-session exit. `shared` mounts are
    /// never listed (host-owned).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub isolated_worktrees: Vec<String>,
}

/// A Claude plugin marketplace the capsule registers at container start via
/// `claude plugin marketplace add`. Mirrors the role manifest's
/// `[[claude.marketplaces]]` without `jackin-protocol` depending on `jackin-core`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeMarketplace {
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sparse: Vec<String>,
}

/// Provider selection for the capsule's initial session spawn. Carries
/// only the label; the daemon re-derives the env redirection from it (and
/// the container's `ZAI_API_KEY`) at spawn time, so there is a single
/// source of truth for the provider's overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialProvider {
    pub label: String,
}

/// Z.AI's Anthropic-compatible API base URL.
pub const ZAI_BASE_URL: &str = "https://api.z.ai/api/anthropic";
/// Z.AI's OpenAI-compatible API base URL (Codex / `OpenCode`).
pub const ZAI_OPENAI_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
/// Z.AI default model mapping: Opus tier → GLM-5.1.
pub const ZAI_DEFAULT_OPUS_MODEL: &str = "glm-5.1";
/// Z.AI default model mapping: Sonnet tier → GLM-5-Turbo.
pub const ZAI_DEFAULT_SONNET_MODEL: &str = "glm-5-turbo";
/// Z.AI default model mapping: Haiku tier → GLM-4.5-Air.
pub const ZAI_DEFAULT_HAIKU_MODEL: &str = "glm-4.5-air";
/// Z.AI recommended API timeout (50 minutes) for long-running agent operations through the proxy.
pub const ZAI_API_TIMEOUT_MS: &str = "3000000";

/// `MiniMax` Anthropic-compatible API base URL (Claude Code and `OpenCode`).
pub const MINIMAX_BASE_URL: &str = "https://api.minimax.io/anthropic";
/// `MiniMax` OpenAI-compatible API base URL (Codex Responses API).
pub const MINIMAX_OPENAI_BASE_URL: &str = "https://api.minimax.io/v1";
/// `MiniMax` Token Plan model — all three Claude tiers map to this single model.
pub const MINIMAX_DEFAULT_MODEL: &str = "MiniMax-M3";
/// `MiniMax-M3` context window (tokens). Codex ships no metadata for this custom
/// model, so jackin❯ registers it via a Codex model catalog; the value cannot be
/// raised through a profile-scoped `model_context_window` (Codex clamps that to
/// the model's fallback cap).
pub const MINIMAX_CONTEXT_WINDOW: u64 = 512_000;
/// `MiniMax` recommended API timeout, matching the Z.AI value.
pub const MINIMAX_API_TIMEOUT_MS: &str = "3000000";

/// Kimi Code Anthropic-compatible API base URL (Claude Code and `OpenCode`).
pub const KIMI_BASE_URL: &str = "https://api.kimi.com/coding";
/// Kimi Code model — all three Claude tiers map to this single model.
pub const KIMI_DEFAULT_MODEL: &str = "kimi-for-coding";
/// Kimi Code recommended API timeout, matching the Z.AI value.
pub const KIMI_API_TIMEOUT_MS: &str = "3000000";

/// API provider a Claude-compatible agent can be routed through. The
/// single source of truth for provider labels, endpoints, and env
/// redirection — the host console, the wire (`InitialProvider` /
/// `SpawnRequest::AgentWithProvider`), and the in-container daemon all
/// match on this enum so the provider catalog cannot drift across sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    /// The agent's own Anthropic auth — no env redirection.
    Anthropic,
    /// The agent's own `OpenAI` auth — no env redirection. Native to Codex.
    Openai,
    /// Z.AI (GLM Coding Plan) via its Anthropic-compatible endpoint.
    Zai,
    /// `MiniMax` Token Plan via its Anthropic-compatible endpoint.
    Minimax,
    /// Kimi Code via its Anthropic-compatible endpoint.
    /// Distinct from the `kimi` agent runtime — this is the provider backend.
    Kimi,
}

impl Provider {
    /// Every provider variant, in picker/display order. Native providers
    /// (Anthropic for `claude`, `OpenAI` for `codex`) lead the catalog.
    pub const ALL: [Provider; 5] = [
        Provider::Anthropic,
        Provider::Openai,
        Provider::Zai,
        Provider::Minimax,
        Provider::Kimi,
    ];

    /// The adapter for this provider. Single dispatch point for all
    /// provider-specific behavior — adding a new provider requires one
    /// adapter struct + one match arm here + one variant in `ALL`, not N
    /// scattered match arms.
    #[must_use]
    pub fn adapter(self) -> &'static dyn ProviderAdapter {
        use provider_adapter::{
            AnthropicAdapter, KimiAdapter, MinimaxAdapter, OpenaiAdapter, ZaiAdapter,
        };
        match self {
            Self::Anthropic => &AnthropicAdapter,
            Self::Openai => &OpenaiAdapter,
            Self::Zai => &ZaiAdapter,
            Self::Minimax => &MinimaxAdapter,
            Self::Kimi => &KimiAdapter,
        }
    }

    /// Display label, also used as the tab suffix and the string carried
    /// on the wire in `InitialProvider` / `AgentWithProvider`.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.adapter().label()
    }

    /// Inverse of [`Provider::label`], derived from the same labels so the
    /// two cannot drift. `None` for an unrecognized label (a stale or
    /// hostile peer naming a provider this build does not know).
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.label() == label)
    }

    /// Stable lowercase slug used as the provider key in role manifests
    /// (`[<agent>.providers.<id>]`) and the capsule provider-model map. Distinct
    /// from [`Provider::label`] (the display/wire identifier) so the operator-
    /// facing config key stays stable and lowercase.
    #[must_use]
    pub const fn manifest_id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Openai => "openai",
            Self::Zai => "zai",
            Self::Minimax => "minimax",
            Self::Kimi => "kimi",
        }
    }

    /// Env overrides that redirect Claude Code to this provider via the
    /// Anthropic-compatible surface. Anthropic needs none. Each alt provider
    /// sets the base URL, auth token (when present), model-tier mapping vars,
    /// a generous API timeout, and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`.
    /// Codex and `OpenCode` route via config files generated at runtime-setup,
    /// not via this method.
    #[must_use]
    pub fn env_overrides(self, token: Option<&str>) -> Vec<(String, String)> {
        self.adapter().env_overrides(token)
    }

    /// Codex v2 profile name for this provider, or `None` if no profile is
    /// needed (native `OpenAI` auth or provider unsupported for Codex).
    #[must_use]
    pub fn codex_profile(self) -> Option<&'static str> {
        self.adapter().codex_profile()
    }

    /// Providers selectable for `(agent_slug, has_key)`. Returns an empty
    /// list when no picker is needed (the agent's native auth is the
    /// implicit choice).
    ///
    /// `has_key(p)` returns `true` when the operator has configured a key for
    /// provider `p`. Each adapter's `needs_key_for_agent` + `supports_agent`
    /// determine membership — no closed match required to add a new provider.
    /// A non-native sole option (e.g. only `Zai` for `opencode`) is still
    /// returned so the caller can auto-route through it without a picker.
    #[must_use]
    pub fn available_for(agent_slug: &str, has_key: impl Fn(Provider) -> bool) -> Vec<Provider> {
        let providers: Vec<Provider> = Self::ALL
            .iter()
            .filter(|&&p| {
                let a = p.adapter();
                a.supports_agent(agent_slug) && (!a.needs_key_for_agent(agent_slug) || has_key(p))
            })
            .copied()
            .collect();
        match providers.as_slice() {
            [] | [Provider::Anthropic | Provider::Openai] => Vec::new(),
            _ => providers,
        }
    }

    /// Model string in `provider/model` format for `OpenCode`'s `-m` flag.
    /// `None` for Anthropic (use `OpenCode`'s own default selection).
    #[must_use]
    pub fn opencode_model(self) -> Option<&'static str> {
        self.adapter().opencode_model()
    }

    /// Env var that holds the API key for this provider, if any.
    ///
    /// Convenience wrapper around `self.adapter().key_env_var()` so callers
    /// do not need to import the `ProviderAdapter` trait.
    #[must_use]
    pub fn key_env_var(self) -> Option<&'static str> {
        self.adapter().key_env_var()
    }
}

impl CapsuleConfig {
    pub fn supported_agents(&self) -> Vec<String> {
        self.agents.clone()
    }

    pub fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.models.get(agent).map(String::as_str)
    }

    /// Model override for `(agent, provider_id)`, where `provider_id` is a
    /// [`Provider::manifest_id`] slug. `None` when the role set no override for
    /// that pair, leaving the caller's own default in force.
    #[must_use]
    pub fn provider_model(&self, agent: &str, provider_id: &str) -> Option<&str> {
        self.provider_models
            .get(agent)?
            .get(provider_id)
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
