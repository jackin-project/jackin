// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exact credential-source forwarding and binding authorization.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCredentialScope, UsageCredentialSourceIdentity,
};

use super::super::discovery::{
    ProviderCredentialSourceMaterial, ValidatedCredentialBinding, ValidatedCredentialSource,
    ValidatedUsageDiscovery,
};
use super::{HostSurfaceId, capability_for_binding};

/// Secret-free launch facts proving which credential sources reached a Capsule.
#[derive(Debug, Clone, Default)]
pub struct ForwardedUsageSources {
    /// Exact configured account ids admitted to this Capsule. A provider
    /// surface alone is never sufficient when several accounts share it.
    pub selected_account_ids: BTreeSet<String>,
    /// Provider surface paired with each selected configured account id. This
    /// lets the runtime replace the config alias with the canonical authority
    /// discovered for that exact account.
    pub selected_account_surfaces: BTreeMap<String, String>,
    /// Surface ids with a successfully forwarded profile directory.
    pub profile_surface_ids: BTreeSet<String>,
    /// Governed provider env names present in the Capsule's resolved environment.
    pub env_keys: BTreeSet<String>,
    /// Exact source/material proofs staged for this launch.
    pub credential_scope: UsageCredentialScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ForwardingRequirement {
    Profile(String),
    Env {
        surface: String,
        /// Canonical provider usage key used for cache/refresh routing.
        key: String,
        /// Exact launch keys synthesized for this binding's provider source.
        launch_keys: BTreeSet<String>,
        account_ids: BTreeSet<String>,
        material: Option<ProviderCredentialSourceMaterial>,
    },
    Capability,
}

/// Match the staged consumer key to the broker's canonical discovery key.
///
/// The key is an agent/provider contract alias, not the protected source
/// identity. The source declaration, account, surface, and material
/// fingerprint remain exact; only the closed provider alias sets below may
/// bridge a launch-native key to the canonical discovery label.
fn credential_keys_match(surface: &str, canonical: &str, staged: &str) -> bool {
    if canonical == staged {
        return true;
    }
    let Some(surface) = HostSurfaceId::from_id(surface) else {
        return false;
    };
    match surface {
        HostSurfaceId::Kimi => matches!(
            (canonical, staged),
            (
                jackin_core::KIMI_CODE_API_KEY_ENV_NAME
                    | jackin_core::KIMI_API_KEY_ENV_NAME
                    | jackin_core::MOONSHOT_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::KIMI_CODE_API_KEY_ENV_NAME
                    | jackin_core::KIMI_API_KEY_ENV_NAME
                    | jackin_core::MOONSHOT_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Zai => matches!(
            (canonical, staged),
            (
                jackin_core::ZAI_API_KEY_ENV_NAME
                    | jackin_core::ZHIPU_API_KEY_ENV_NAME
                    | "Z_AI_API_KEY"
                    | jackin_core::OPENAI_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::ZAI_API_KEY_ENV_NAME
                    | jackin_core::ZHIPU_API_KEY_ENV_NAME
                    | "Z_AI_API_KEY"
                    | jackin_core::OPENAI_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Minimax => matches!(
            (canonical, staged),
            (
                jackin_core::MINIMAX_API_KEY_ENV_NAME
                    | "MINIMAX_CODING_API_KEY"
                    | "MINIMAX_API_TOKEN"
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::MINIMAX_API_KEY_ENV_NAME
                    | "MINIMAX_CODING_API_KEY"
                    | "MINIMAX_API_TOKEN"
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Google => matches!(
            (canonical, staged),
            (
                jackin_core::GEMINI_API_KEY_ENV_NAME | jackin_core::GOOGLE_API_KEY_ENV_NAME,
                jackin_core::GEMINI_API_KEY_ENV_NAME | jackin_core::GOOGLE_API_KEY_ENV_NAME
            )
        ),
        _ => false,
    }
}

fn credential_scope_has_matching_proof(
    scope: &UsageCredentialScope,
    account_ids: &BTreeSet<String>,
    surface: &str,
    _canonical_key: &str,
    launch_keys: &BTreeSet<String>,
    material: &ProviderCredentialSourceMaterial,
) -> bool {
    scope
        .sources
        .iter()
        .filter(|proof| account_ids.contains(&proof.account_id) && proof.surface_id == surface)
        .any(|proof| {
            launch_keys
                .iter()
                .any(|launch_key| credential_keys_match(surface, launch_key, &proof.key))
                && proof.source == material.source
                && proof.material_fingerprint == material.material_fingerprint
        })
}

fn binding_account_ids(binding: &ValidatedCredentialBinding) -> BTreeSet<String> {
    binding
        .provenance
        .iter()
        .filter_map(|provenance| provenance.strip_prefix("account "))
        .map(str::to_owned)
        .collect()
}

fn binding_matches_proof(
    binding: &ValidatedCredentialBinding,
    proof: &jackin_protocol::usage_broker::UsageCredentialSourceProof,
) -> bool {
    if binding.surface.id() != proof.surface_id
        || !binding_account_ids(binding).contains(&proof.account_id)
    {
        return false;
    }
    let ValidatedCredentialSource::Env {
        launch_keys,
        material: Some(material),
        ..
    } = &binding.source
    else {
        return false;
    };
    launch_keys
        .iter()
        .any(|launch_key| credential_keys_match(binding.surface.id(), launch_key, &proof.key))
        && proof.source == material.source
        && proof.material_fingerprint == material.material_fingerprint
}

fn record_key_binding(
    key_bindings: &mut Vec<(String, String, UsageCredentialSourceIdentity, String)>,
    account_id: &str,
    launch_key: &str,
    material: &ProviderCredentialSourceMaterial,
) -> bool {
    let Some((_, _, source, fingerprint)) =
        key_bindings
            .iter()
            .find(|(existing_account, existing_key, _, _)| {
                existing_account == account_id && existing_key == launch_key
            })
    else {
        key_bindings.push((
            account_id.to_owned(),
            launch_key.to_owned(),
            material.source.clone(),
            material.material_fingerprint.clone(),
        ));
        return true;
    };
    source == &material.source && fingerprint == &material.material_fingerprint
}

/// Authorize a capability against every relevant launch proof and return the
/// exact binding whose source may be refreshed. Multiple route bindings may
/// share one canonical capability, but each proof must resolve to exactly one
/// compatible binding. Unrelated account/surface proofs are ignored;
/// conflicts and unexpected keys fail closed.
pub(super) fn authorize_credential_binding_group(
    bindings: &[ValidatedCredentialBinding],
    surface: &str,
    scope: &UsageCredentialScope,
) -> Option<ValidatedCredentialBinding> {
    let has_profile = bindings
        .iter()
        .any(|binding| matches!(&binding.source, ValidatedCredentialSource::Profile(_)));
    let env_bindings = bindings
        .iter()
        .filter(|binding| {
            binding.surface.id() == surface
                && matches!(&binding.source, ValidatedCredentialSource::Env { .. })
        })
        .collect::<Vec<_>>();

    // A capability group must never let env proof select a profile or let a
    // profile's mere presence authorize an env route. Pure-profile behavior
    // remains the baseline path.
    if has_profile && !env_bindings.is_empty() {
        return None;
    }
    if has_profile {
        return bindings
            .iter()
            .find(|binding| matches!(&binding.source, ValidatedCredentialSource::Profile(_)))
            .cloned();
    }
    if env_bindings.is_empty() {
        return None;
    }
    // A duplicated account/surface/launch-key binding is ambiguous when its
    // source or material differs. Reject that conflict before proof matching;
    // a valid proof for one side must never authorize the other side.
    let mut key_bindings = Vec::<(String, String, UsageCredentialSourceIdentity, String)>::new();
    for binding in &env_bindings {
        let ValidatedCredentialSource::Env {
            launch_keys,
            material: Some(material),
            ..
        } = &binding.source
        else {
            return None;
        };
        for account_id in binding_account_ids(binding) {
            for launch_key in launch_keys {
                if !record_key_binding(&mut key_bindings, &account_id, launch_key, material) {
                    return None;
                }
            }
        }
    }
    let account_ids = env_bindings
        .iter()
        .flat_map(|binding| binding_account_ids(binding))
        .collect::<BTreeSet<_>>();
    let relevant = scope
        .sources
        .iter()
        .filter(|proof| account_ids.contains(&proof.account_id) && proof.surface_id == surface)
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        return None;
    }
    let mut matched = Vec::with_capacity(relevant.len());
    for proof in relevant {
        let candidates = env_bindings
            .iter()
            .filter(|binding| binding_matches_proof(binding, proof))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return None;
        }
        matched.push(candidates[0]);
    }
    let first = (**matched.first()?).clone();
    if matched
        .iter()
        .all(|binding| refresh_authority_equivalent(&first, binding))
    {
        Some(first)
    } else {
        None
    }
}

/// Compare the authority that a refresh would actually use. Launch aliases
/// may differ, but canonical identity, semantic dispatch, opaque handle, and
/// source material must agree before proofs can combine.
fn refresh_authority_equivalent(
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
pub(super) fn unscoped_refresh_binding(
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
    fn is_forwarded(&self, sources: &ForwardedUsageSources) -> bool {
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

fn forwarding_requirement(binding: &ValidatedCredentialBinding) -> ForwardingRequirement {
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

/// Derive an exact per-container capability allowlist before broker startup.
#[must_use]
pub(crate) fn forwarded_usage_capabilities(
    discovery: &ValidatedUsageDiscovery,
    scope_label: &str,
    sources: &ForwardedUsageSources,
) -> Vec<UsageAccountCapability> {
    discovery
        .bindings
        .iter()
        .filter(|binding| {
            if sources.selected_account_ids.is_empty() {
                binding.provenance.contains(scope_label)
            } else {
                binding.provenance.iter().any(|provenance| {
                    sources.selected_account_ids.iter().any(|account_id| {
                        provenance == &format!("account {account_id}")
                            && sources
                                .selected_account_surfaces
                                .get(account_id)
                                .is_some_and(|surface| surface == binding.surface.id())
                    })
                })
            }
        })
        .filter(|binding| forwarding_requirement(binding).is_forwarded(sources))
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Resolve one exact configured account to the canonical broker capability
/// used by Capsule sessions. Multiple source bindings for the same canonical
/// account collapse to one capability; distinct identities are rejected rather
/// than guessed.
#[must_use]
#[cfg(test)]
pub(crate) fn usage_capability_for_selected_account(
    discovery: &ValidatedUsageDiscovery,
    account_id: &str,
    surface_id: &str,
) -> Option<UsageAccountCapability> {
    usage_capability_for_selected_account_with_sources(discovery, account_id, surface_id, None)
}

/// Resolve one exact configured account after intersecting it with the
/// credential sources forwarded into the current Capsule. The source proof is
/// part of launch authority: a provider surface or account id alone cannot
/// select a credential when several routes share that identity.
#[must_use]
pub(crate) fn usage_capability_for_selected_account_with_sources(
    discovery: &ValidatedUsageDiscovery,
    account_id: &str,
    surface_id: &str,
    sources: Option<&ForwardedUsageSources>,
) -> Option<UsageAccountCapability> {
    let provenance = format!("account {account_id}");
    let capabilities = discovery
        .bindings
        .iter()
        .filter(|binding| binding.surface.id() == surface_id)
        .filter(|binding| binding.provenance.contains(&provenance))
        .filter(|binding| {
            sources.is_none_or(|sources| match &binding.source {
                // API-key/OAuth routes need exact staged source proof. Profile
                // and forwarded capability behavior stays on the baseline path.
                ValidatedCredentialSource::Env { .. } => {
                    forwarding_requirement(binding).is_forwarded(sources)
                }
                ValidatedCredentialSource::Profile(_)
                | ValidatedCredentialSource::Capability
                | ValidatedCredentialSource::Unpollable => true,
            })
        })
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>();
    (capabilities.len() == 1)
        .then(|| capabilities.into_iter().next())
        .flatten()
}

/// Every canonical capability in one validated host discovery generation.
#[must_use]
#[cfg(test)]
pub(crate) fn usage_broker_capabilities(
    discovery: &ValidatedUsageDiscovery,
) -> Vec<UsageAccountCapability> {
    discovery
        .bindings
        .iter()
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
