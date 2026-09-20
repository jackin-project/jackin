// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Programmatic (non-TTY) launch surface for [`super::LoadOptions`].
//!
//! The interactive CLI resolves every launch decision through dialogs: the
//! agent picker, the trust prompt, the sensitive-mount confirmation, the
//! on-demand credential picker. A daemon has no terminal to answer any of
//! them, so a programmatic launch must arrive with every decision already
//! made and be *rejected up front* when one is missing — never fall through
//! to a dialog that cannot be drawn.
//!
//! This module owns exactly that: the extra decisions a caller pre-supplies
//! ([`super::LoadOptions`] fields), their validation ([`LoadOptions::validate_programmatic`]),
//! the identity the launch reports back ([`LaunchedInstance`]), and the
//! agent-specific model/effort env mapping. The launch itself keeps running
//! through the one shared pipeline the CLI uses — nothing here forks it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use jackin_config::{AgentConfiguration, AppConfig};
use jackin_core::{Agent, ReasoningEffort, RoleSelector};

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

impl super::LoadOptions {
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
    pub(super) fn record_launched_instance(&self, container_base: &str) {
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
        if let Some(account) = self.account.as_ref()
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

/// Env var carrying the Codex model to the in-container role hook (`SCHED-014`).
pub const CODEX_LANE_MODEL_ENV: &str = jackin_core::CODEX_LANE_MODEL_ENV_NAME;
/// Env var carrying the Codex reasoning effort to the same role hook.
pub const CODEX_LANE_EFFORT_ENV: &str = jackin_core::CODEX_LANE_EFFORT_ENV_NAME;
/// Env var Claude Code reads for its model.
pub const CLAUDE_MODEL_ENV: &str = jackin_core::CLAUDE_MODEL_ENV_NAME;
/// Env var Claude Code reads for its reasoning effort.
pub const CLAUDE_EFFORT_ENV: &str = jackin_core::CLAUDE_EFFORT_ENV_NAME;

/// Container env that pins `model` and reasoning effort for `agent`.
///
/// Codex reads neither from its argv: the sourced role hook writes `model` and
/// `model_reasoning_effort` into `$CODEX_HOME/config.toml` from these two
/// variables. Passing the launch's model through the same pair — while the
/// capsule also receives it as the agent's model — is what keeps the hook and
/// the daemon from disagreeing about which model is running (D-078).
///
/// Returns entries in a stable order so the launch env is reproducible.
#[must_use]
pub fn lane_agent_env(
    agent: Agent,
    model: Option<&str>,
    effort: Option<ReasoningEffort>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (model_key, effort_key) = match agent {
        Agent::Codex => (CODEX_LANE_MODEL_ENV, CODEX_LANE_EFFORT_ENV),
        Agent::Claude => (CLAUDE_MODEL_ENV, CLAUDE_EFFORT_ENV),
        // Every other runtime takes its model on argv (the capsule passes
        // `-m`/`--model`), and declares no effort knob today.
        _ => return out,
    };
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        out.push((model_key.to_owned(), model.to_owned()));
    }
    if let Some(effort) = effort {
        out.push((effort_key.to_owned(), effort.as_str().to_owned()));
    }
    out
}

/// Free configuration id for a synthesized one-launch pick.
///
/// Persisted configuration ids are validated slugs and can never contain
/// `@`, so `{account}@{agent}` cannot collide with them; the suffix loop
/// only covers hand-built configs that skipped validation.
fn free_ephemeral_config_id(selected: &AppConfig, agent: Agent, id: &str) -> String {
    let base = format!("{id}@{}", agent.slug());
    match selected.agent_configurations.get(&base) {
        // A hand-built config may use the synthesized ID even though
        // persisted IDs normally reject `@`. Reuse it intact: replacing it
        // would silently discard its model, endpoint, label, and wrapper.
        Some(existing) if existing.agent == agent && existing.account == id => return base,
        None => return base,
        Some(_) => {}
    }
    let mut counter = 2_u32;
    while selected
        .agent_configurations
        .contains_key(&format!("{base}-{counter}"))
    {
        counter += 1;
    }
    format!("{base}-{counter}")
}

fn ensure_account_allowed(
    config: &AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    account_id: &str,
) -> anyhow::Result<()> {
    let Some(workspace) = workspace else {
        return Ok(());
    };
    let workspace_config = config
        .workspaces
        .get(workspace.as_str())
        .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?;
    anyhow::ensure!(
        workspace_config
            .accounts
            .iter()
            .any(|allowed| allowed == account_id),
        "account {account_id:?} is not assigned to workspace {workspace}"
    );
    Ok(())
}

fn bind_selected_account(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: Agent,
    account_id: &str,
) -> anyhow::Result<()> {
    if let Some(workspace) = workspace {
        let override_config = selected
            .workspaces
            .get_mut(workspace.as_str())
            .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?
            .roles
            .entry(role.to_owned())
            .or_default();
        override_config
            .account_bindings
            .insert(agent, account_id.to_owned());
    } else {
        selected
            .account_bindings
            .insert(agent, account_id.to_owned());
    }
    Ok(())
}

fn write_default_launch(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    configuration_ids: &[String],
) -> anyhow::Result<()> {
    if let Some(workspace) = workspace {
        selected
            .workspaces
            .get_mut(workspace.as_str())
            .ok_or_else(|| anyhow::anyhow!("workspace {workspace} is not configured"))?
            .roles
            .entry(role.to_owned())
            .or_default()
            .default_launch = Some(configuration_ids.to_vec());
    } else {
        selected.default_launch = Some(configuration_ids.to_vec());
    }
    Ok(())
}

/// Replace only one agent's configurations in the effective launch list.
/// Other-agent configurations remain admitted, while the selected agent gets
/// the exact identities chosen by the caller.
fn replace_agent_default_launch(
    selected: &mut AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agent: Agent,
    chosen_ids: &[String],
) -> anyhow::Result<()> {
    anyhow::ensure!(!chosen_ids.is_empty(), "launch selection cannot be empty");
    let inherited = selected
        .effective_default_launch(workspace, role)
        .map(<[String]>::to_vec)
        .ok_or_else(|| anyhow::anyhow!("no default launch set is configured"))?;
    let mut replaced = false;
    let mut launch = Vec::with_capacity(inherited.len() + chosen_ids.len());
    for configuration_id in inherited {
        let configuration = selected
            .agent_configurations
            .get(&configuration_id)
            .ok_or_else(|| anyhow::anyhow!("unknown agent configuration {configuration_id:?}"))?;
        if configuration.agent == agent {
            if !replaced {
                launch.extend(chosen_ids.iter().cloned());
                replaced = true;
            }
        } else {
            launch.push(configuration_id);
        }
    }
    if !replaced {
        launch.extend(chosen_ids.iter().cloned());
    }
    write_default_launch(selected, workspace, role, &launch)?;
    Ok(())
}

fn ensure_account_is_selected(
    instances: &[jackin_config::ResolvedInstance],
    agent: Agent,
    account_id: &str,
    exact_configuration: Option<&str>,
) -> anyhow::Result<()> {
    let selected: Vec<_> = instances
        .iter()
        .filter(|instance| instance.agent == agent)
        .collect();
    anyhow::ensure!(
        !selected.is_empty(),
        "account {account_id:?} is not admitted for {agent} by the configured default launch set"
    );
    anyhow::ensure!(
        selected
            .iter()
            .all(|instance| instance.account_id == account_id),
        "account selection admitted a sibling account for {agent}"
    );
    if let Some(configuration) = exact_configuration {
        anyhow::ensure!(
            selected.len() == 1 && selected[0].config_id == configuration,
            "configuration {configuration:?} was not the exact launch identity"
        );
    }
    Ok(())
}

/// Make an ephemeral account selection, preserving workspace admission checks.
///
/// Records a one-launch pick for (`agent`, `id`) on a cloned config and
/// validates the result through `jackin_config::resolve_launch` — the same
/// resolver the launch pipeline provisions from — so the console pre-check
/// and the runtime cannot admit different sets.
///
/// Authorization (the workspace allowlist) and admission (the
/// `default_launch` set) stay distinct: the allowlist check rejects foreign
/// accounts up front, while the launch-set check rejects picks the
/// configured defaults do not admit instead of silently substituting
/// another account. When no default is configured at any scope, the pick is
/// synthesized into an ephemeral configuration plus a one-entry role
/// (saved workspace) or global (ad-hoc) default, so the multi-instance
/// pipeline provisions exactly the picked account instead of failing with
/// ambiguity. The supplied configuration is never mutated.
///
/// # Errors
/// Rejects unknown accounts, incompatible agents, accounts outside the
/// workspace allowlist, picks the configured defaults do not admit, and
/// invalid `default_launch` sets.
pub fn with_account_selection(
    config: &AppConfig,
    agent: Agent,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    id: &str,
) -> anyhow::Result<AppConfig> {
    let account = config
        .accounts
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("account {id:?} is not registered"))?;
    anyhow::ensure!(
        account.supports_agent(agent),
        "account {id:?} does not support {agent}"
    );
    ensure_account_allowed(config, workspace, id)?;

    let admitted_ids = if config.effective_default_launch(workspace, role).is_some() {
        let instances = jackin_config::resolve_launch(config, workspace, role, None, Some(agent))?;
        let ids: Vec<String> = instances
            .into_iter()
            .filter(|instance| instance.agent == agent && instance.account_id == id)
            .map(|instance| instance.config_id)
            .collect();
        anyhow::ensure!(
            !ids.is_empty(),
            "account {id:?} is not admitted for {agent} by the configured default launch set"
        );
        Some(ids)
    } else {
        None
    };

    let mut selected = config.clone();
    bind_selected_account(&mut selected, workspace, role, agent, id)?;
    if let Some(configuration_ids) = admitted_ids {
        replace_agent_default_launch(&mut selected, workspace, role, agent, &configuration_ids)?;
    } else {
        let config_id = free_ephemeral_config_id(&selected, agent, id);
        selected
            .agent_configurations
            .entry(config_id.clone())
            .or_insert_with(|| AgentConfiguration {
                agent,
                account: id.to_owned(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            });
        write_default_launch(&mut selected, workspace, role, &[config_id])?;
    }
    let instances = jackin_config::resolve_launch(&selected, workspace, role, None, Some(agent))?;
    ensure_account_is_selected(&instances, agent, id, None)?;
    Ok(selected)
}

/// Select one exact registered agent configuration on a cloned config and
/// make that configuration the only selected identity for its agent.
pub fn with_configuration_selection(
    config: &AppConfig,
    agent: Agent,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    configuration_id: &str,
) -> anyhow::Result<AppConfig> {
    let requested = [configuration_id.to_owned()];
    let instances = jackin_config::resolve_launch(config, workspace, role, Some(&requested), None)?;
    let instance = instances
        .first()
        .ok_or_else(|| anyhow::anyhow!("configuration {configuration_id:?} resolved empty"))?;
    anyhow::ensure!(
        instance.agent == agent,
        "configuration {configuration_id:?} belongs to {}, not {agent}",
        instance.agent
    );
    let account_id = instance.account_id.clone();
    ensure_account_allowed(config, workspace, &account_id)?;

    let mut selected = config.clone();
    bind_selected_account(&mut selected, workspace, role, agent, &account_id)?;
    if selected.effective_default_launch(workspace, role).is_some() {
        replace_agent_default_launch(&mut selected, workspace, role, agent, &requested)?;
    } else {
        write_default_launch(&mut selected, workspace, role, &requested)?;
    }
    let resolved = jackin_config::resolve_launch(&selected, workspace, role, None, Some(agent))?;
    ensure_account_is_selected(&resolved, agent, &account_id, Some(configuration_id))?;
    Ok(selected)
}

#[cfg(test)]
mod tests;
