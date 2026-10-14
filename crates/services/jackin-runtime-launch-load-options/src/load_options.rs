// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

    pub force: bool,

    pub op_runner: Option<Box<dyn jackin_env::OpRunner>>,

    pub host_env: Option<BTreeMap<String, String>>,

    pub agent: Option<Agent>,

    pub role_branch: Option<String>,

    pub docker_profile:
        Option<jackin_runtime_docker_profile::docker_profile::DockerSecurityProfile>,

    pub restore_container_base: Option<String>,

    pub restore_role_source_git: Option<String>,
    pub non_interactive: bool,

    pub selection: Option<jackin_core::LaunchSelection>,

    pub entry_claim: Option<Arc<jackin_runtime_universe_claims::claims::EntryClaim>>,

    pub model: Option<String>,

    pub effort: Option<jackin_core::ReasoningEffort>,

    pub env: BTreeMap<String, String>,

    pub on_demand_bindings: Vec<jackin_protocol::ExecBinding>,

    pub extra_mounts: Vec<jackin_config::MountConfig>,

    pub identity_sink: Option<IdentitySink>,

    pub git_program: Option<std::path::PathBuf>,
}

impl LoadOptions {
    pub fn for_load(debug: bool, rebuild: bool) -> Self {
        Self {
            debug,
            rebuild,
            ..Self::default()
        }
    }

    pub fn for_launch(debug: bool) -> Self {
        Self {
            debug,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchedInstance {
    pub instance_id: String,
    pub container_base: String,
}

impl LaunchedInstance {
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

pub type IdentitySink = Arc<Mutex<Option<LaunchedInstance>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOptionsError {
    AgentNotResolved { role: String },

    TrustNotGranted { role: String },

    RoleBranchNotAllowed { branch: String },

    AccountMissing { account: String },

    EmptyModel,

    ReservedEnvName { name: String },

    EmptyEnvName,

    DuplicateOnDemandBinding { name: String },

    IncompleteOnDemandBinding { name: String },
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
    #[must_use]
    pub fn programmatic(agent: Agent) -> Self {
        Self {
            agent: Some(agent),
            non_interactive: true,
            identity_sink: Some(Arc::new(Mutex::new(None))),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn launched_instance(&self) -> Option<LaunchedInstance> {
        self.identity_sink
            .as_ref()
            .and_then(|sink| sink.lock().ok().and_then(|slot| slot.clone()))
    }

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

#[cfg(test)]
mod programmatic_model_tests {
    use super::{LoadOptions, LoadOptionsError};

    #[test]
    fn programmatic_whitespace_model_is_rejected() {
        let mut options = LoadOptions {
            non_interactive: true,
            agent: Some(jackin_core::Agent::Codex),
            model: Some("   ".into()),
            effort: Some(jackin_core::ReasoningEffort::Max),
            ..LoadOptions::default()
        };
        let selector = jackin_core::RoleSelector::new(None, "agent-smith");
        let config = jackin_config::AppConfig::default();

        assert_eq!(
            options.validate_programmatic(&config, &selector),
            Err(LoadOptionsError::EmptyModel)
        );

        options.model = Some("gpt-6-luna".into());
        assert_eq!(options.validate_programmatic(&config, &selector), Ok(()));
    }
}
