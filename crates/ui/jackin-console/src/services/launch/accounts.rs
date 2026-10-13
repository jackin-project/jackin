// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch account choices and admission.

use jackin_config::{AccountConfig, AppConfig, ResolvedInstance, resolve_launch};
use jackin_core::{Agent, WorkspaceName};

/// Secret-free account row used by launch and session pickers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountChoice {
    pub id: String,
    pub name: String,
    pub provider: jackin_config::AiProvider,
    pub agents: Vec<Agent>,
    /// Exact pre-container launch configuration. `None` for legacy account
    /// rows and live rows, which use their own routing identity.
    pub configuration_id: Option<String>,
    /// Exact live launch instance. `None` for pre-container launch rows.
    pub instance_id: Option<String>,
}

impl AccountChoice {
    /// Consume this pre-container picker row as one exclusive launch identity.
    #[must_use]
    pub fn into_launch_selection(self) -> jackin_core::LaunchSelection {
        match self.configuration_id {
            Some(id) => jackin_core::LaunchSelection::Configuration(id),
            None => jackin_core::LaunchSelection::Account(self.id),
        }
    }

    pub fn label(&self) -> String {
        if let Some(instance_id) = self.instance_id.as_deref() {
            return format!(
                "{} · {} ({}) · instance {instance_id}",
                self.name, self.provider, self.id
            );
        }
        self.configuration_id.as_deref().map_or_else(
            || format!("{} · {} ({})", self.name, self.provider, self.id),
            |configuration_id| {
                format!(
                    "{} · {} ({}) · configuration {configuration_id}",
                    self.name, self.provider, self.id
                )
            },
        )
    }
}

/// Secret-free row for one registered account: id, display name, provider,
/// and every agent the account can authenticate.
pub(crate) fn account_row(id: &str, account: &AccountConfig) -> AccountChoice {
    AccountChoice {
        id: id.to_owned(),
        name: account.name.clone(),
        provider: account.provider,
        agents: Agent::ALL
            .iter()
            .copied()
            .filter(|agent| account.supports_agent(*agent))
            .collect(),
        configuration_id: None,
        instance_id: None,
    }
}

/// One exact account/agent binding admitted by a live container manifest.
/// This is deliberately separate from [`ResolvedInstance`]: the console
/// refresh service must not re-resolve mutable host defaults to describe a
/// running container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInstanceAdmission {
    pub instance_id: String,
    pub agent: Agent,
    pub account_id: String,
}

/// List only registered accounts authorized by the saved workspace.
/// Ad-hoc launches have no workspace allowlist; choosing a registered account
/// here is the explicit selection. A missing saved workspace yields no choices.
///
/// This is the authorization-and-compatibility view, not the admission view:
/// it ignores `default_launch`. Defaults-aware callers use
/// [`admitted_account_choices`] instead.
pub fn account_choices(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
) -> Vec<AccountChoice> {
    config
        .accounts
        .iter()
        .filter(|(id, account)| {
            account.enabled
                && workspace.is_none_or(|workspace| {
                    config
                        .workspaces
                        .get(workspace.as_str())
                        .is_some_and(|workspace| workspace.accounts.contains(id))
                })
        })
        .map(|(id, account)| account_row(id, account))
        .collect()
}

/// Filter authorized registered accounts by coding-agent compatibility.
///
/// Like [`account_choices`], this ignores `default_launch`; see
/// [`admitted_account_choices`] for the admission-constrained rows.
pub fn accounts_for_launch(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    agent: Agent,
) -> Vec<AccountChoice> {
    account_choices(config, workspace)
        .into_iter()
        .filter(|account| account.agents.contains(&agent))
        .collect()
}

/// Map admitted launch instances to secret-free picker rows.
///
/// One row per admitted configuration, in ascending account/configuration
/// order. Configurations sharing an account remain distinct: the
/// configuration, not just the account, is the launch identity.
/// Instances naming an unregistered account are skipped:
/// `jackin_config::resolve_launch` never produces them, so only a foreign
/// instance list can hit that.
#[must_use]
pub fn account_choices_for_instances(
    config: &AppConfig,
    instances: &[ResolvedInstance],
) -> Vec<AccountChoice> {
    let mut choices: Vec<AccountChoice> = instances
        .iter()
        .filter_map(|instance| {
            config.accounts.get(&instance.account_id).map(|account| {
                let mut choice = account_row(&instance.account_id, account);
                choice.configuration_id = Some(instance.config_id.clone());
                choice
            })
        })
        .collect();
    choices.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.configuration_id.cmp(&right.configuration_id))
    });
    choices
}

/// Build live-session rows from the manifest's admitted instance set.
/// Duplicate accounts remain distinct because `instance_id` is the routing
/// identity; a later picker commit must never collapse them back to account ID.
#[must_use]
pub fn account_choices_for_live_instances(
    config: &AppConfig,
    admissions: &[LiveInstanceAdmission],
) -> Vec<AccountChoice> {
    let mut choices: Vec<AccountChoice> = admissions
        .iter()
        .filter_map(|admission| {
            let account = config.accounts.get(&admission.account_id)?;
            if !account.enabled || !account.supports_agent(admission.agent) {
                return None;
            }
            let mut choice = account_row(&admission.account_id, account);
            choice.agents = vec![admission.agent];
            choice.instance_id = Some(admission.instance_id.clone());
            Some(choice)
        })
        .collect();
    choices.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    choices
}

/// Admitted picker rows for a committed (role + agent) launch.
///
/// Returns `None` when no `default_launch` is configured at any scope —
/// the legacy account-bindings path applies. Otherwise resolves through
/// `jackin_config::resolve_launch`, the same resolver the runtime
/// provisions from, so this pre-check cannot drift from it, and returns
/// the admitted rows for `agent` (possibly empty when the defaults admit
/// nothing for this agent).
///
/// # Errors
///
/// Returns the resolver error verbatim when the configured defaults are
/// invalid (unknown configuration, unauthorized or incompatible account):
/// an explicit default fails atomically and never falls back to the
/// eligible-candidate list.
pub fn admitted_account_choices(
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
    agent: Agent,
) -> anyhow::Result<Option<Vec<AccountChoice>>> {
    if config.effective_default_launch(workspace, role).is_none() {
        return Ok(None);
    }
    let instances = resolve_launch(config, workspace, role, None, Some(agent))?;
    let mine: Vec<ResolvedInstance> = instances
        .into_iter()
        .filter(|instance| instance.agent == agent)
        .collect();
    Ok(Some(account_choices_for_instances(config, &mine)))
}
