//! jackin-protocol: attach/control wire protocol types shared by host and capsule.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`ClientFrame`] — attach-protocol client frame.

#![deny(
    clippy::string_slice,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::unchecked_time_subtraction
)]
#![deny(missing_docs)]

mod account_credentials;
pub use account_credentials::AgentCredentialEnv;

use jackin_core::container_paths;

pub mod agent_status;
pub mod attach;
pub mod control;
pub mod snapshot;
pub mod telemetry_context;
pub mod usage_broker;

pub use telemetry_context::TelemetryContext;

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
    /// `name` field.
    pub name: String,
    /// `kind` field.
    pub kind: ExecKind,
    /// Host-owned source. Capsule-facing projections replace literal values
    /// with the fixed `literal` marker; `op` and env references remain intact.
    pub source: String,
}

/// `jackin-exec` host.sock request: the operator-selected credentials the
/// in-container capsule asks the host resolver to resolve. Framed with
/// [`control::frame`], same as the control socket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredRequest {
    /// Cross-process trace and product correlation.
    pub ctx: TelemetryContext,
    /// `refs` field.
    pub refs: Vec<ExecBinding>,
}

/// `jackin-exec` host.sock reply. Internally tagged so the capsule decodes it
/// in one parse instead of trying success-then-error struct shapes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CredReply {
    /// Every requested credential resolved: `name -> value`.
    Ok {
        /// Map of binding name to resolved secret value.
        values: BTreeMap<String, String>,
    },
    /// Resolution failed; `error` is operator-facing (no secret material).
    Error {
        /// Operator-facing failure text (never secret material).
        error: String,
    },
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
/// Bounded, non-secret auth-mode carrier from Capsule config to runtime setup.
pub const AUTH_MODE_ENV: &str = "JACKIN_AUTH_MODE";

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
    /// `role` field.
    pub role: String,
    /// `workdir` field.
    pub workdir: String,
    #[serde(default)]
    /// `agents` field.
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    /// `models` field.
    pub models: BTreeMap<String, String>,
    /// Resolved per-agent auth modes (`sync|api_key|oauth_token|ignore`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auth_modes: BTreeMap<String, String>,
    /// Claude plugin marketplaces declared by the role manifest. The capsule
    /// registers them at container start — the agent binary is mounted, not
    /// baked, so plugin setup moved out of the image build into runtime-setup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_marketplaces: Vec<ClaudeMarketplace>,
    /// Claude plugins declared by the role manifest, installed at container
    /// start by the capsule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_plugins: Vec<String>,
    /// On-demand credential bindings (`jackin-exec`). The host keeps exact
    /// `(name, kind, source)` allowlist entries. The serialized Capsule copy
    /// redacts literal sources to the fixed `literal` marker; `op` and env
    /// references remain so the picker can identify them. Resolved values never
    /// enter this data contract. Empty when no on-demand vars are declared.
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
    /// `source` field.
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// `sparse` field.
    pub sparse: Vec<String>,
}

/// Provider identity used for telemetry and display metadata.
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
    /// Every provider variant, in display order. Native providers
    /// (Anthropic for `claude`, `OpenAI` for `codex`) lead the catalog.
    pub const ALL: [Provider; 5] = [
        Provider::Anthropic,
        Provider::Openai,
        Provider::Zai,
        Provider::Minimax,
        Provider::Kimi,
    ];

    /// Display label, also used as the tab suffix and the string carried
    /// when displaying account provider metadata.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::Openai => "OpenAI",
            Self::Zai => "Z.AI",
            Self::Minimax => "MiniMax",
            Self::Kimi => "Kimi",
        }
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
}

impl CapsuleConfig {
    /// `supported_agents` method.
    pub fn supported_agents(&self) -> Vec<String> {
        self.agents.clone()
    }

    /// `model_for_agent` method.
    pub fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.models.get(agent).map(String::as_str)
    }

    /// Resolved bounded authentication mode for an agent runtime.
    #[must_use]
    pub fn auth_mode_for_agent(&self, agent: &str) -> Option<&str> {
        self.auth_modes.get(agent).map(String::as_str)
    }
}

pub mod host_terminal;
#[cfg(test)]
mod tests;
