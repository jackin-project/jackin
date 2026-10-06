// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential forwarding requirements.

use std::collections::BTreeSet;

use super::super::discovery::{ValidatedCredentialBinding, ValidatedCredentialSource};

use super::{ForwardedUsageSources, ForwardingRequirement, credential_scope_has_matching_proof};

/// Compare the authority that a refresh would actually use. Launch aliases
/// may differ, but canonical identity, semantic dispatch, opaque handle, and
/// source material must agree before proofs can combine.
pub(crate) fn refresh_authority_equivalent(
    left: &ValidatedCredentialBinding,
    right: &ValidatedCredentialBinding,
) -> bool {
    match (&left.source, &right.source) {
        (
            ValidatedCredentialSource::Env {
                handle: left_handle,
                key: left_key,
                dispatch_key: left_dispatch,
                material: Some(left_material),
                ..
            },
            ValidatedCredentialSource::Env {
                handle: right_handle,
                key: right_key,
                dispatch_key: right_dispatch,
                material: Some(right_material),
                ..
            },
        ) => {
            left.surface == right.surface
                && left.identity == right.identity
                && left_handle == right_handle
                && left_key == right_key
                && left_dispatch == right_dispatch
                && left_material == right_material
        }
        (ValidatedCredentialSource::Profile(_), ValidatedCredentialSource::Profile(_)) => {
            left.surface == right.surface && left.identity == right.identity
        }
        _ => false,
    }
}

/// Select an unscoped refresh only when the whole group has one refresh
/// authority. This prevents background work from depending on vector order.
pub(crate) fn unscoped_refresh_binding(
    bindings: &[ValidatedCredentialBinding],
) -> Option<ValidatedCredentialBinding> {
    let first = bindings.first()?.clone();
    if matches!(&first.source, ValidatedCredentialSource::Profile(_)) {
        return bindings
            .iter()
            .all(|binding| matches!(&binding.source, ValidatedCredentialSource::Profile(_)))
            .then_some(first);
    }
    bindings
        .iter()
        .all(|binding| refresh_authority_equivalent(&first, binding))
        .then_some(first)
}

impl ForwardingRequirement {
    pub(crate) fn is_forwarded(&self, sources: &ForwardedUsageSources) -> bool {
        match self {
            Self::Profile(surface) => sources.profile_surface_ids.contains(surface),
            Self::Env {
                surface,
                key,
                launch_keys,
                account_ids,
                material,
            } => {
                if sources.selected_account_ids.is_empty() {
                    return launch_keys
                        .iter()
                        .any(|launch_key| sources.env_keys.contains(launch_key));
                }
                let Some(material) = material else {
                    return false;
                };
                let account_ids = account_ids
                    .iter()
                    .filter(|account_id| sources.selected_account_ids.contains(*account_id))
                    .cloned()
                    .collect::<BTreeSet<_>>();
                credential_scope_has_matching_proof(
                    &sources.credential_scope,
                    &account_ids,
                    surface,
                    key,
                    launch_keys,
                    material,
                )
            }
            Self::Capability => false,
        }
    }
}

pub(crate) fn forwarding_requirement(
    binding: &ValidatedCredentialBinding,
) -> ForwardingRequirement {
    match &binding.source {
        ValidatedCredentialSource::Profile(_) => {
            ForwardingRequirement::Profile(binding.surface.id().to_owned())
        }
        ValidatedCredentialSource::Env {
            key,
            launch_keys,
            material,
            ..
        } => ForwardingRequirement::Env {
            surface: binding.surface.id().to_owned(),
            key: key.clone(),
            launch_keys: launch_keys.clone(),
            account_ids: binding
                .provenance
                .iter()
                .filter_map(|provenance| provenance.strip_prefix("account "))
                .map(str::to_owned)
                .collect(),
            material: material.clone(),
        },
        ValidatedCredentialSource::Capability => ForwardingRequirement::Capability,
        // Unpollable bindings carry no host source to forward; like
        // capability-only bindings, they stay host-local.
        ValidatedCredentialSource::Unpollable => ForwardingRequirement::Capability,
    }
}
