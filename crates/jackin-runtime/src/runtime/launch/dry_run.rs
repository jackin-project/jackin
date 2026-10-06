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
//!   instance listed; the canonical model projection fills each model).
//!
//! An explicit `--account` pick keeps the single-account shape for the
//! selected agent, while retaining the complete scoped admission internally
//! so other-agent slots and provider-specific model projection stay exact.

use jackin_config::{AppConfig, ResolvedInstance};
use jackin_core::{Agent, WorkspaceName};

/// Identity half of the `--dry-run` plan: one account for an unambiguous
/// fallback launch, or every admitted instance otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DryRunIdentity {
    /// Resolved account id, set only for the single-account shape.
    pub account_id: Option<String>,
    /// Account model hint for [`Self::account_id`]. The launch-equivalent
    /// model is resolved separately by [`resolve_dry_run_model_projection`].
    pub model: Option<String>,
    /// Admitted instances for the instances shape; empty otherwise. Model
    /// values are projected separately after the role manifest is available.
    pub instances: Vec<ResolvedInstance>,
    /// The complete admission result, including a synthesized instance when
    /// the public plan uses the single-account shape. This lets dry-run
    /// resolve role and task-scoped model choices with the same per-slot
    /// resolver as launch without changing the displayed identity shape.
    pub admitted_instances: Vec<ResolvedInstance>,
}

/// Canonical model projection for a resolved dry-run identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DryRunModelProjection {
    /// Effective model in the single-account shape; absent for a multi-slot
    /// plan, whose models belong to individual instances.
    pub model: Option<String>,
    /// Effective models keyed by the exact instance config ID used by launch.
    pub instances: std::collections::BTreeMap<String, String>,
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
        let admitted_instances = jackin_config::resolve_launch(
            plan_config,
            workspace,
            role_key,
            None,
            Some(selected_agent),
        )?;
        return Ok(DryRunIdentity {
            account_id,
            model,
            instances: Vec::new(),
            admitted_instances,
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
            admitted_instances: instances,
        });
    }
    Ok(DryRunIdentity {
        account_id: None,
        model: None,
        admitted_instances: instances.clone(),
        instances,
    })
}

/// Resolve role, account, and task-scoped models exactly as the launch
/// pipeline does, then project them onto the dry-run's single-account or
/// per-instance display shape.
pub fn resolve_dry_run_model_projection(
    config: &AppConfig,
    role_models: &std::collections::BTreeMap<Agent, String>,
    identity: &DryRunIdentity,
    selected_agent: Agent,
    model_override: Option<&str>,
) -> anyhow::Result<DryRunModelProjection> {
    let instances = super::capsule_setup::resolved_instance_models_from_role_models(
        config,
        role_models,
        &identity.admitted_instances,
        selected_agent,
        model_override,
    )?;
    let model = identity.account_id.as_ref().and_then(|_| {
        let selected_models: Vec<Option<&str>> = identity
            .admitted_instances
            .iter()
            .filter(|instance| instance.agent == selected_agent)
            .map(|instance| instances.get(&instance.config_id).map(String::as_str))
            .collect();
        let first = selected_models.first().copied().flatten()?;
        selected_models
            .iter()
            .all(|candidate| *candidate == Some(first))
            .then(|| first.to_owned())
    });
    Ok(DryRunModelProjection { model, instances })
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
