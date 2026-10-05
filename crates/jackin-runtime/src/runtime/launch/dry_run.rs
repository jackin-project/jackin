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
//! * one explicit account selection with one admitted slot — the same
//!   single-account shape;
//! * anything with multiple admitted slots — the instances shape (`account`
//!   unset, every real admitted instance listed).
//!
//! The role manifest, account/configuration model, model override, and effort
//! override are projected from the same admitted slots and shared resolvers as
//! the launch path. A model or effort override is scoped to the selected agent;
//! sibling agents keep their own effective models and no effort entry.

use jackin_config::{AppConfig, ResolvedInstance};
use jackin_core::{Agent, ReasoningEffort, WorkspaceName};

/// Model and effort requested for the selected agent in a dry-run preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DryRunOverrides<'a> {
    /// Exact CLI model value before provider normalization.
    pub model: Option<&'a str>,
    /// CLI reasoning effort applied to the selected agent's admitted slots.
    pub effort: Option<ReasoningEffort>,
}

/// Identity half of the `--dry-run` plan: one account for an unambiguous
/// fallback launch, or every admitted instance otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DryRunIdentity {
    /// Resolved account id, set only for the single-account shape.
    pub account_id: Option<String>,
    /// Effective model for the single admitted slot when the account shape is
    /// collapsed. It includes the role, account/configuration, and CLI model
    /// precedence used by the launch.
    pub model: Option<String>,
    /// Admitted instances for the instances shape; empty otherwise. Each
    /// carries the effective model for that slot.
    pub instances: Vec<ResolvedInstance>,
    /// Effective effort by real admitted configuration ID. Only slots for the
    /// selected agent appear; collapsed identity shapes do not expose IDs.
    pub efforts: std::collections::BTreeMap<String, String>,
}

/// Resolve the identity half of the `--dry-run` plan.
///
/// `plan_config` is the effective config (already scoped by
/// [`super::programmatic::with_account_selection`] when `explicit_account_pick`
/// is set); `manifest` is from the validated role repository resolved for the
/// image plan; and `selected_agent` is the committed launch agent. Both paths
/// use the exact [`jackin_config::resolve_launch`] admission and shared model
/// and effort projections, so the preview and launch cannot disagree.
///
/// # Errors
///
/// Propagates the admission failure (unknown/unauthorized/incompatible
/// selection, or an ambiguous fallback) unchanged, like the launch would.
pub fn resolve_dry_run_identity(
    plan_config: &AppConfig,
    manifest: &jackin_manifest::RoleManifest,
    selected_agent: Agent,
    workspace: Option<&WorkspaceName>,
    role_key: &str,
    explicit_account_pick: bool,
    overrides: DryRunOverrides<'_>,
) -> anyhow::Result<DryRunIdentity> {
    let mut instances = jackin_config::resolve_launch(
        plan_config,
        workspace,
        role_key,
        None,
        Some(selected_agent),
    )?;
    anyhow::ensure!(
        !instances.is_empty(),
        "no agent instances are admitted for role {role_key:?}"
    );

    if explicit_account_pick {
        let selected_account = &instances[0].account_id;
        anyhow::ensure!(
            instances
                .iter()
                .all(|instance| &instance.account_id == selected_account),
            "explicit account selection admitted multiple account identities"
        );
    }

    let models = super::capsule_setup::resolved_instance_models(
        plan_config,
        manifest,
        &instances,
        selected_agent,
        overrides.model,
    )?;
    let efforts = super::capsule_setup::resolved_instance_efforts(
        &instances,
        selected_agent,
        overrides.effort,
    );
    for instance in &mut instances {
        instance.model = models.get(&instance.config_id).cloned();
    }

    let collapse_single_account = instances.len() == 1
        && (explicit_account_pick || instances[0].synthesized);
    if collapse_single_account {
        let single = &instances[0];
        return Ok(DryRunIdentity {
            account_id: Some(single.account_id.clone()),
            model: single.model.clone(),
            instances: Vec::new(),
            efforts,
        });
    }
    Ok(DryRunIdentity {
        account_id: None,
        model: None,
        instances,
        efforts,
    })
}

#[cfg(test)]
mod tests;
