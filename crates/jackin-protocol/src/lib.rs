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
pub use account_credentials::{
    AgentCredentialEnv, InstanceCredentialEnv, StagedInstanceCredential,
};

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

/// Normalized runtime config path read by the Capsule daemon.
pub const CAPSULE_CONFIG_PATH: &str = container_paths::CAPSULE_CONFIG;

/// Launch metadata naming the Capsule supervisor PID for local peer checks.
pub const CAPSULE_SUPERVISOR_PID_ENV: &str = "JACKIN_CAPSULE_SUPERVISOR_PID";

/// Apple `container run` starts the Capsule entrypoint after `vminitd`.
pub const APPLE_CAPSULE_SUPERVISOR_PID: u32 = 2;

/// Path inside the role container of the `jackin-exec` host credential
/// resolver socket. The host creates it under the bind-mounted `/jackin/run`
/// dir; the in-container capsule connects here to resolve on-demand
/// credentials. Single source of truth so the mount side and the connect side
/// cannot drift.
pub const HOST_SOCK_CONTAINER_PATH: &str = container_paths::HOST_SOCK;

/// Environment variable carrying the daemon-issued bearer capability for one
/// child session's target-scoped control RPCs. It is never a host passthrough.
pub const SESSION_CAPABILITY_ENV: &str = "JACKIN_SESSION_CAPABILITY";

/// Internal daemon-assigned numeric session id consumed only by the root
/// isolation wrapper. It is present for every child, including shell panes,
/// but is not an agent runtime/status identity.
pub const ISOLATION_SESSION_ID_ENV: &str = "JACKIN_ISOLATION_SESSION_ID";

/// Daemon-assigned numeric session id exposed to agent runtimes and status
/// hooks. Shell panes intentionally do not receive this variable.
pub const SESSION_ID_ENV: &str = "JACKIN_SESSION_ID";
/// Private mutable setup/state root allocated for one session.
pub const SESSION_STATE_DIR_ENV: &str = "JACKIN_SESSION_STATE_DIR";
/// Bounded, non-secret auth-mode carrier from Capsule config to runtime setup.
pub const AUTH_MODE_ENV: &str = "JACKIN_AUTH_MODE";

/// Spawn env carrying the admitted instance config ID for a pane. The
/// daemon sets it on every agent spawn; per-session setup uses it only
/// for log attribution, never to resolve paths (paths come from
/// [`INSTANCE_FORWARDED_DIR_ENV`] and the agent's folder env var).
pub const INSTANCE_ENV: &str = "JACKIN_INSTANCE";

/// Spawn env carrying the instance's host-forwarded credential directory
/// (`/jackin/<agent>` for primary slots, `/jackin/<agent>-<suffix>` for
/// secondary same-agent slots). Per-session setup reads forwarded files
/// from here instead of hardcoded per-agent constants.
pub const INSTANCE_FORWARDED_DIR_ENV: &str = "JACKIN_FORWARDED_DIR";

/// Container-side directory containing one read-only credential file per
/// admitted secret-bearing instance. The capsule supervisor reads these files;
/// agent sessions never receive the directory as an unrestricted mount.
pub const ACCOUNT_CREDENTIALS_DIR: &str = "/jackin/account-credentials";

/// Filename the capsule writes the operator's dirty-exit choice to, under the
/// per-instance state dir, for the host to read and execute on cleanup.
pub const EXIT_ACTION_FILENAME: &str = "exit-action.json";

/// In-container path the capsule writes [`ExitAction`] to. The host's state-dir
/// mount makes this readable from outside the container at
/// `<data_dir>/<container>/state/exit-action.json`.
pub const EXIT_ACTION_PATH: &str = container_paths::EXIT_ACTION;

/// Stable, path-safe filename for one instance's staged credential file.
/// Encoding every byte avoids collisions and makes hostile config IDs inert.
#[must_use]
pub fn account_credentials_filename(instance: &str) -> String {
    let mut encoded = String::with_capacity(instance.len() * 2 + 5);
    encoded.push_str("acct-");
    for byte in instance.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded.push_str(".json");
    encoded
}

/// Container-side path for one staged instance credential file.
#[must_use]
pub fn account_credentials_container_path(instance: &str) -> String {
    format!(
        "{}/{}",
        ACCOUNT_CREDENTIALS_DIR,
        account_credentials_filename(instance)
    )
}

/// Unix identity used by exactly one capsule session class.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionIdentity {
    /// Effective and filesystem UID after the supervisor drops privilege.
    pub uid: u32,
    /// Effective and filesystem GID after the supervisor drops privilege.
    pub gid: u32,
}

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
    /// Admitted launch instance config IDs, in launch order. Several
    /// instances may share one agent runtime; per-instance facts live in
    /// `agents`, `models`, `auth_modes`, `accounts`, `labels`,
    /// `instance_home_dirs`, `instance_forwarded_dirs`, and the
    /// protected [`AgentCredentialEnv`] envelope, all keyed by these same IDs.
    pub instances: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    /// Agent runtime slug per admitted instance, keyed by instance config
    /// ID. The capsule needs this to select the runtime binary: config IDs
    /// are opaque (`work-claude` carries no slug) and sync-mode instances
    /// have no credential envelope to consult.
    pub agents: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    /// Effective per-instance model, keyed by instance config ID.
    pub models: BTreeMap<String, String>,
    /// Effective per-instance reasoning effort, keyed by instance config ID.
    /// Values use the closed `low|medium|high|max` vocabulary.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub efforts: BTreeMap<String, String>,
    /// Resolved per-instance auth modes (`sync|api_key|oauth_token|ignore`),
    /// keyed by instance config ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auth_modes: BTreeMap<String, String>,
    /// Owning account ID per admitted instance, keyed by instance config ID.
    /// The capsule stamps spawned sessions, tabs, and history records from
    /// this map: `sync`/`ignore` instances have no credential-envelope entry,
    /// so the envelope alone cannot answer which account owns an instance.
    /// Identifiers only — never credential material.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accounts: BTreeMap<String, String>,
    /// Exact host usage-broker capability per admitted instance. The capsule
    /// must carry this opaque authority unchanged; account labels and agent
    /// surface names are not sufficient to select a same-provider account.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub usage_capabilities: BTreeMap<String, usage_broker::UsageAccountCapability>,
    /// Display label per admitted instance, keyed by instance config ID.
    /// `{Agent} · {account name}` unless the operator overrode it. Tab/pane
    /// chrome renders these so two same-agent instances stay distinguishable.
    /// Names only — never secrets.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
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
    /// Value the daemon assigns to the agent's folder env var
    /// (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, …) when spawning this
    /// instance, keyed by instance config ID. Primary slots carry the
    /// legacy home (`/home/agent/.claude`); secondary same-agent slots
    /// carry a suffixed home (`/home/agent/.claude-<suffix>`) so two
    /// accounts for one agent never share credentials or history.
    /// Every admitted instance has an entry; a missing entry fails the
    /// spawn closed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_home_dirs: BTreeMap<String, String>,
    /// Per-instance XDG cache roots, keyed by instance config ID. These are
    /// distinct container paths even when the instances share an agent
    /// runtime; the capsule uses the value as `XDG_CACHE_HOME` for that
    /// instance instead of falling back to the PTY-session cache.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_cache_dirs: BTreeMap<String, String>,
    /// Container handoff directory holding this instance's host-forwarded
    /// credential files, keyed by instance config ID (`/jackin/<agent>`
    /// for primary slots, `/jackin/<agent>-<suffix>` for secondary
    /// same-agent slots). Every admitted instance has an entry; a missing
    /// entry fails the spawn closed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_forwarded_dirs: BTreeMap<String, String>,
    /// Read-only per-instance credential file paths. Secret-bearing admitted
    /// instances have exactly one entry; a container-wide credential file is
    /// invalid and is never represented here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_credential_files: BTreeMap<String, String>,
    /// Mount paths owned by each instance. The session boundary uses this
    /// allowlist to prevent a sibling from reaching another slot's home or
    /// auth handoff, including when host bind mounts share numeric ownership.
    /// The separate `instance_credential_files` are deliberately omitted:
    /// only the root supervisor may read those files; selected credentials
    /// reach an agent through its already-admitted environment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_mount_paths: BTreeMap<String, Vec<String>>,
    /// Distinct Unix identity per admitted instance.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instance_identities: BTreeMap<String, SessionIdentity>,
    /// Identity used for an unscoped interactive shell. It receives no
    /// instance credential/home allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_identity: Option<SessionIdentity>,
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
    /// Admitted instance config IDs, in launch order.
    pub fn supported_instances(&self) -> Vec<String> {
        self.instances.clone()
    }

    /// Per-instance model override for an instance config ID.
    pub fn model_for_instance(&self, instance: &str) -> Option<&str> {
        self.models.get(instance).map(String::as_str)
    }

    /// Per-instance reasoning effort for an instance config ID.
    #[must_use]
    pub fn effort_for_instance(&self, instance: &str) -> Option<&str> {
        self.efforts.get(instance).map(String::as_str)
    }

    /// Resolved bounded authentication mode for an instance config ID.
    #[must_use]
    pub fn auth_mode_for_instance(&self, instance: &str) -> Option<&str> {
        self.auth_modes.get(instance).map(String::as_str)
    }

    /// Agent runtime slug for an instance config ID.
    #[must_use]
    pub fn agent_for_instance(&self, instance: &str) -> Option<&str> {
        self.agents.get(instance).map(String::as_str)
    }

    /// Owning account ID for an instance config ID.
    #[must_use]
    pub fn account_for_instance(&self, instance: &str) -> Option<&str> {
        self.accounts.get(instance).map(String::as_str)
    }

    /// Exact host usage capability for an instance config ID.
    #[must_use]
    pub fn usage_capability_for_instance(
        &self,
        instance: &str,
    ) -> Option<&usage_broker::UsageAccountCapability> {
        self.usage_capabilities.get(instance)
    }

    /// Display label for an instance config ID.
    #[must_use]
    pub fn label_for_instance(&self, instance: &str) -> Option<&str> {
        self.labels.get(instance).map(String::as_str)
    }

    /// Folder-var target (container config-home dir) for an instance
    /// config ID.
    #[must_use]
    pub fn home_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_home_dirs.get(instance).map(String::as_str)
    }

    /// Per-instance XDG cache root for an instance config ID.
    #[must_use]
    pub fn cache_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_cache_dirs.get(instance).map(String::as_str)
    }

    /// Host-forwarded credential directory for an instance config ID.
    #[must_use]
    pub fn forwarded_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_forwarded_dirs
            .get(instance)
            .map(String::as_str)
    }

    /// Read-only staged credential file for an instance.
    #[must_use]
    pub fn credential_file_for_instance(&self, instance: &str) -> Option<&str> {
        self.instance_credential_files
            .get(instance)
            .map(String::as_str)
    }

    /// Unix identity for an admitted instance.
    #[must_use]
    pub fn identity_for_instance(&self, instance: &str) -> Option<SessionIdentity> {
        self.instance_identities.get(instance).copied()
    }

    /// Container paths belonging to an admitted instance.
    #[must_use]
    pub fn mount_paths_for_instance(&self, instance: &str) -> &[String] {
        self.instance_mount_paths
            .get(instance)
            .map_or(&[][..], Vec::as_slice)
    }

    /// Resolve a spawn target to its admitted instance config ID. An exact
    /// config-ID match wins; otherwise an agent slug resolves only when
    /// exactly one admitted instance uses that runtime. Ambiguity and
    /// unknown targets are explicit errors — never a silent pick.
    pub fn resolve_instance(&self, raw: &str) -> Result<&str, &'static str> {
        if let Some(id) = self.instances.iter().find(|id| id.as_str() == raw) {
            return if self.agents.contains_key(id) {
                Ok(id.as_str())
            } else {
                Err("instance has no agent runtime in launch config")
            };
        }
        let mut hit = None;
        for id in &self.instances {
            if self.agents.get(id).is_some_and(|slug| slug == raw) {
                if hit.is_some() {
                    return Err("ambiguous agent: several instances share it; pick an instance");
                }
                hit = Some(id.as_str());
            }
        }
        hit.ok_or("not in launch config allowlist")
    }
}

pub mod host_terminal;
#[cfg(test)]
mod tests;
