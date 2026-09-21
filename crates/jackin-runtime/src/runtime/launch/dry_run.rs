// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical `--dry-run` identity resolution: the same resolver launch
//! admission provisions from, projected onto the plan's two shapes.
//!
//! Launch admission resolves instances with
//! `jackin_config::resolve_launch(config, workspace, role, None, Some(agent))`
//! and provisions exactly that set. A dry-run plan that resolved identity any
//! other way could name an account the launch would never admit — a full
//! launch list beats a bare per-agent account preference at any scope, so
//! resolving the binding first (bindings win) disagrees with the launch
//! (lists win) whenever both coexist. [`resolve_dry_run_identity`] runs the
//! admission call itself, so the hook and the capsule daemon cannot disagree
//! (D-078); it only decides how the admitted set is displayed:
//!
//! * one synthesized instance (binding/sole-eligible fallback, no list) — the
//!   single-account shape (`account` set, `instances` empty);
//! * anything else — the instances shape (`account` unset, every admitted
//!   instance listed, each carrying its effective model).
//!
//! An explicit `--account` pick keeps the single-account shape: the scoped
//! config rewrites both the binding and the launch list to the pick, so the
//! list it carries is the pick itself, not an ambient default the plan must
//! surface.

use jackin_config::{AppConfig, ResolvedInstance};
use jackin_core::{Agent, WorkspaceName};

/// Identity half of the `--dry-run` plan: one account for an unambiguous
/// fallback launch, or every admitted instance otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DryRunIdentity {
    /// Resolved account id, set only for the single-account shape.
    pub account_id: Option<String>,
    /// Effective model for [`Self::account_id`], when the account pins one.
    /// Lets the plan echo the exact model the launch would provision.
    pub model: Option<String>,
    /// Admitted instances for the instances shape; empty otherwise. Each
    /// carries its effective model (configuration override, else account
    /// default) byte-exact from the resolvers.
    pub instances: Vec<ResolvedInstance>,
}

/// Resolve the identity half of the `--dry-run` plan.
///
/// `plan_config` is the effective config (already scoped by
/// [`super::programmatic::with_account_selection`] when `explicit_account_pick`
/// is set); `selected_agent` is the committed launch agent. The non-pick path
/// issues the exact [`jackin_config::resolve_launch`] call launch admission
/// provisions from, so dry-run and launch admit the same set by construction.
///
/// # Errors
///
/// Propagates the admission failure (unknown/unauthorized/incompatible
/// selection, or an ambiguous fallback) unchanged, like the launch would.
pub fn resolve_dry_run_identity(
    plan_config: &AppConfig,
    selected_agent: Agent,
    workspace: Option<&WorkspaceName>,
    role_key: &str,
    explicit_account_pick: bool,
) -> anyhow::Result<DryRunIdentity> {
    if explicit_account_pick {
        let selected =
            jackin_config::resolve_account(plan_config, selected_agent, workspace, role_key)?;
        let account_id = selected.and_then(|account| {
            plan_config
                .accounts
                .iter()
                .find(|(_, known)| std::ptr::eq(*known, account))
                .map(|(id, _)| id.clone())
        });
        let model = account_id
            .as_ref()
            .and_then(|id| plan_config.accounts.get(id).and_then(account_model));
        return Ok(DryRunIdentity {
            account_id,
            model,
            instances: Vec::new(),
        });
    }
    let instances = jackin_config::resolve_launch(
        plan_config,
        workspace,
        role_key,
        None,
        Some(selected_agent),
    )?;
    if let [single] = instances.as_slice()
        && single.synthesized
    {
        return Ok(DryRunIdentity {
            account_id: Some(single.account_id.clone()),
            model: single.model.clone(),
            instances: Vec::new(),
        });
    }
    Ok(DryRunIdentity {
        account_id: None,
        model: None,
        instances,
    })
}

/// Account-level model default: only API-key credentials pin one.
fn account_model(account: &jackin_config::AccountConfig) -> Option<String> {
    match &account.credential {
        jackin_config::AccountCredential::ApiKey { model, .. } => model.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
