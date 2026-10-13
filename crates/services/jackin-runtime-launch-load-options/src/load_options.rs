// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Caller-supplied options for `jackin load` ([`LoadOptions`]).
//!
//! The interactive CLI resolves every launch decision through dialogs: the
//! agent picker, the trust prompt, the sensitive-mount confirmation, the
//! on-demand credential picker. A daemon has no terminal to answer any of
//! them, so a programmatic launch must arrive with every decision already
//! made and be *rejected up front* when one is missing — never fall through
//! to a dialog that cannot be drawn.
//!
//! This module owns exactly that surface: the options bag, its validation
//! ([`LoadOptions::validate_programmatic`]), the identity the launch
//! reports back ([`LaunchedInstance`]), and the validation-failure
//! vocabulary ([`LoadOptionsError`]). Moved out of `jackin-runtime`
//! `launch.rs` / `launch/programmatic.rs` (S7 split 87).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use jackin_config::AppConfig;
use jackin_core::{Agent, RoleSelector};

#[expect(
    missing_debug_implementations,
    reason = "LoadOptions contains an injected OpRunner trait object that cannot expose Debug."
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "LoadOptions is a caller-supplied options bag whose flags (debug, rebuild, force, non_interactive) are independent switches, not a state machine."
)]
#[derive(Default)]
pub struct LoadOptions {
    pub debug: bool,
    pub rebuild: bool,

    /// Bypass interactive preflight gates (e.g. dirty host repo).
    /// Wired through to `PreflightContext.force` during workspace
    /// materialization.
    pub force: bool,

    /// Optional test seam: inject a custom `OpRunner` for `op://`
    /// resolution. `None` (the production default) means
    /// `resolve_operator_env` picks the default `OpCli::new()`.
    pub op_runner: Option<Box<dyn jackin_env::OpRunner>>,

    /// Optional test seam: inject a host-env lookup map. `None` (the
    /// production default) means `resolve_operator_env` reads from
    /// `std::env::var`. When `Some(map)`, `$NAME` / `${NAME}`
    /// references are resolved by looking up `name` in `map`.
    pub host_env: Option<BTreeMap<String, String>>,

    /// CLI override for the agent. `None` defers to (in order) workspace
    /// `default_agent`, the role's single supported agent, or a rich launch
    /// dialog. A launch against a multi-agent role with no resolved choice is
    /// an error when the rich dialog is unavailable.
    pub agent: Option<Agent>,

    /// When set, resolve this branch of the role repo instead of the default
    /// branch, build the image locally from the branch's Dockerfile (ignoring
    /// any `published_image`), and tag it with a branch-specific name so the
    /// stable image is not overwritten.
    pub role_branch: Option<String>,

    /// Docker security profile override for this launch.
    pub docker_profile:
        Option<jackin_runtime_docker_profile::docker_profile::DockerSecurityProfile>,

    /// Exact missing instance to restore instead of scanning for candidates.
    pub restore_container_base: Option<String>,

    /// Role source URL captured in the instance manifest for restore paths.
    pub restore_role_source_git: Option<String>,
    /// Non-TTY programmatic launch: every decision the interactive path would
    /// prompt for is pre-supplied, no dialog may be drawn, and the launch does
    /// not attach a foreground session. A missing decision is a validation
    /// error (see `LoadOptions::validate_programmatic`), never a prompt.
    pub non_interactive: bool,

    /// Exclusive account or exact configuration selected for this launch.
    /// `None` resolves the configured defaults.
    pub selection: Option<jackin_core::LaunchSelection>,

    /// Shared ownership of one construct-entry lease, activated after role start.
    /// Pending ownership survives preflight errors and asynchronous cancellation.
    pub entry_claim: Option<Arc<jackin_runtime_universe_claims::claims::EntryClaim>>,

    /// Exact model id for the launched agent, overriding the role manifest's
    /// `[<agent>].model`. Also passed to the in-container Codex role hook so
    /// the hook and the daemon cannot disagree (D-078).
    pub model: Option<String>,

    /// Reasoning effort for the launched agent.
    pub effort: Option<jackin_core::ReasoningEffort>,

    /// Env values injected at launch on top of the resolved manifest and
    /// operator env. Reserved names are rejected by validation.
    pub env: BTreeMap<String, String>,

    /// On-demand credential bindings the caller already approved. Merged with
    /// the bindings collected from config, so a daemon needs no interactive
    /// credential picker (D-082).
    pub on_demand_bindings: Vec<jackin_protocol::ExecBinding>,

    /// Extra bind mounts appended to the resolved workspace's mounts for this
    /// launch only, mirroring repeated `--mount` on the CLI.
    pub extra_mounts: Vec<jackin_config::MountConfig>,

    /// Slot the launch writes its claimed instance identity into.
    pub identity_sink: Option<IdentitySink>,

    /// Test seam for workspace `git pull` so fast-restore tests can prove the
    /// pull path did not run without mutating process-wide PATH. Un-gated
    /// at the split-87 move: a `cfg(test)` field would vanish from the leaf's
    /// non-test build that hub tests link against.
    pub git_program: Option<std::path::PathBuf>,
}

impl LoadOptions {
    /// Build options for `jackin load`.
    pub fn for_load(debug: bool, rebuild: bool) -> Self {
        Self {
            debug,
            rebuild,
            ..Self::default()
        }
    }

    /// Build options for the operator console (`jackin console`).
    pub fn for_launch(debug: bool) -> Self {
        Self {
            debug,
            ..Self::default()
        }
    }
}

/// Identity of the instance a programmatic launch claimed.
///
/// `instance_id` is the short id `jackin status` and `jackin hardline` accept;
/// `container_base` is the full Docker container base name it was derived
/// from. Both are recorded because a container base that predates the
/// instance-id naming scheme cannot be shortened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchedInstance {
    /// Short instance id (`jackin status <instance id>`).
    pub instance_id: String,
    /// Full container base name backing the instance.
    pub container_base: String,
}

impl LaunchedInstance {
    /// Derive the identity from a claimed container base name.
    #[must_use]
    pub fn from_container_base(container_base: &str) -> Self {
        let instance_id = jackin_core::instance_id_from_container_base(container_base)
            .map_or_else(|| container_base.to_owned(), ToOwned::to_owned);
        Self {
            instance_id,
            container_base: container_base.to_owned(),
        }
    }
}

/// Shared slot a programmatic launch writes its claimed identity into.
///
/// The pipeline threads `&LoadOptions` through every phase and through its own
/// restore/rebuild recursion, so a `&mut` out-parameter would have to be
/// plumbed through all of them. A shared sink records the identity at the one
/// point the container name is locked, without widening any signature.
pub type IdentitySink = Arc<Mutex<Option<LaunchedInstance>>>;

/// A launch decision a programmatic caller failed to supply, or supplied in a
/// form the non-interactive path cannot honor.
///
/// Every variant is a *validation* failure raised before any Docker work
/// starts, so a daemon gets a precise reason instead of a mid-launch dialog
/// error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOptionsError {
    /// No agent was pre-selected. A multi-agent role would need the launch
    /// dialog to choose one.
    AgentNotResolved {
        /// Role the launch targeted.
        role: String,
    },
    /// The role source has no trust grant on this host. The trust prompt is
    /// interactive, so a daemon can never answer it (Q-022/D-053).
    TrustNotGranted {
        /// Role whose grant is missing.
        role: String,
    },
    /// `--role-branch` was requested. Loading an unreviewed branch needs the
    /// branch-trust prompt, which is interactive by construction.
    RoleBranchNotAllowed {
        /// The branch that was requested.
        branch: String,
    },
    /// The requested account is absent from the registry.
    AccountMissing {
        /// Registered account ID requested by the caller.
        account: String,
    },
    /// An empty model string was supplied.
    EmptyModel,
    /// A pre-supplied env name is reserved by the jackin runtime.
    ReservedEnvName {
        /// The rejected name.
        name: String,
    },
    /// A pre-supplied env name is empty.
    EmptyEnvName,
    /// The same on-demand binding name was pre-approved twice.
    DuplicateOnDemandBinding {
        /// The duplicated binding name.
        name: String,
    },
    /// An on-demand binding was pre-approved with an empty name or source.
    IncompleteOnDemandBinding {
        /// The binding name (possibly empty).
        name: String,
    },
}

impl std::fmt::Display for LoadOptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentNotResolved { role } => write!(
                f,
                "programmatic launch of {role:?} did not resolve an agent; a non-TTY launch \
                 must name the agent because the launch dialog is unavailable"
            ),
            Self::TrustNotGranted { role } => write!(
                f,
                "role source {role:?} is not trusted; a non-TTY launch cannot answer the trust \
                 prompt — run `jackin config trust grant {role}` on this host first"
            ),
            Self::RoleBranchNotAllowed { branch } => write!(
                f,
                "role branch {branch:?} cannot be loaded non-interactively; loading an \
                 unreviewed branch requires the branch-trust prompt"
            ),
            Self::AccountMissing { account } => write!(f, "account {account:?} is not registered"),
            Self::EmptyModel => f.write_str("model override cannot be empty"),
            Self::ReservedEnvName { name } => write!(
                f,
                "env name {name:?} is reserved by the jackin runtime and cannot be supplied"
            ),
            Self::EmptyEnvName => f.write_str("env name cannot be empty"),
            Self::DuplicateOnDemandBinding { name } => write!(
                f,
                "on-demand binding {name:?} was pre-approved more than once"
            ),
            Self::IncompleteOnDemandBinding { name } => write!(
                f,
                "on-demand binding {name:?} must carry a non-empty name and source"
            ),
        }
    }
}

impl std::error::Error for LoadOptionsError {}

impl LoadOptions {
    /// Options for a non-TTY programmatic launch.
    ///
    /// Every decision the interactive path would prompt for is supplied here.
    /// Call [`Self::validate_programmatic`] (or let the pipeline call it) before
    /// launching: a missing decision is a validation error, never a dialog.
    #[must_use]
    pub fn programmatic(agent: Agent) -> Self {
        Self {
            agent: Some(agent),
            non_interactive: true,
            identity_sink: Some(Arc::new(Mutex::new(None))),
            ..Self::default()
        }
    }

    /// Identity claimed by the launch these options drove, if it got that far.
    #[must_use]
    pub fn launched_instance(&self) -> Option<LaunchedInstance> {
        self.identity_sink
            .as_ref()
            .and_then(|sink| sink.lock().ok().and_then(|slot| slot.clone()))
    }

    /// Record the claimed container base as this launch's identity.
    ///
    /// A no-op when no sink was installed (every interactive launch), and
    /// first-write-wins so a restore that recurses into a second launch does
    /// not overwrite the identity the caller is waiting for.
    ///
    /// `pub` (widened from `pub(super)` at the split-87 move) for the launch
    /// pipeline, which records the locked container name from another crate.
    pub fn record_launched_instance(&self, container_base: &str) {
        let Some(sink) = self.identity_sink.as_ref() else {
            return;
        };
        let Ok(mut slot) = sink.lock() else {
            return;
        };
        if slot.is_none() {
            *slot = Some(LaunchedInstance::from_container_base(container_base));
        }
    }

    /// Validate the pre-supplied decisions against the host config.
    ///
    /// Interactive launches skip every check: they can still answer a prompt.
    ///
    /// # Errors
    ///
    /// Returns the first missing or unusable decision.
    pub fn validate_programmatic(
        &self,
        config: &AppConfig,
        selector: &RoleSelector,
    ) -> Result<(), LoadOptionsError> {
        if !self.non_interactive {
            return Ok(());
        }
        let role = selector.key();
        if self.agent.is_none() {
            return Err(LoadOptionsError::AgentNotResolved { role });
        }
        if let Some(branch) = self.role_branch.as_ref() {
            return Err(LoadOptionsError::RoleBranchNotAllowed {
                branch: branch.clone(),
            });
        }
        if !role_trust_granted(config, &role) {
            return Err(LoadOptionsError::TrustNotGranted { role });
        }
        if let Some(jackin_core::LaunchSelection::Account(account)) = self.selection.as_ref()
            && !config.accounts.contains_key(account)
        {
            return Err(LoadOptionsError::AccountMissing {
                account: account.clone(),
            });
        }
        if self.model.as_ref().is_some_and(|m| m.trim().is_empty()) {
            return Err(LoadOptionsError::EmptyModel);
        }
        validate_env(&self.env)?;
        validate_on_demand_bindings(&self.on_demand_bindings)?;
        Ok(())
    }
}

/// Whether the host already granted trust for this role source.
///
/// A built-in role ships trusted, so it needs no explicit grant; every other
/// source needs `trusted = true` recorded in the host config.
fn role_trust_granted(config: &AppConfig, role_key: &str) -> bool {
    if AppConfig::is_builtin_agent(role_key) {
        return true;
    }
    config
        .roles
        .get(role_key)
        .is_some_and(|source| source.trusted)
}

fn validate_env(env: &BTreeMap<String, String>) -> Result<(), LoadOptionsError> {
    for name in env.keys() {
        if name.is_empty() {
            return Err(LoadOptionsError::EmptyEnvName);
        }
        if jackin_core::is_reserved(name) {
            return Err(LoadOptionsError::ReservedEnvName { name: name.clone() });
        }
    }
    Ok(())
}

fn validate_on_demand_bindings(
    bindings: &[jackin_protocol::ExecBinding],
) -> Result<(), LoadOptionsError> {
    let mut seen = std::collections::BTreeSet::new();
    for binding in bindings {
        if binding.name.trim().is_empty() || binding.source.trim().is_empty() {
            return Err(LoadOptionsError::IncompleteOnDemandBinding {
                name: binding.name.clone(),
            });
        }
        if !seen.insert(binding.name.as_str()) {
            return Err(LoadOptionsError::DuplicateOnDemandBinding {
                name: binding.name.clone(),
            });
        }
    }
    Ok(())
}
