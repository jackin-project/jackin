// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential binding authorization proofs.

use std::collections::BTreeSet;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCredentialScope, UsageCredentialSourceIdentity,
};

use super::super::HostSurfaceId;
use super::super::discovery::{
    ProviderCredentialSourceMaterial, ValidatedCredentialBinding, ValidatedCredentialSource,
};
use super::refresh_authority_equivalent;

#[derive(Debug, Clone)]
pub(crate) struct ScopedCapability {
    pub(crate) capability: UsageAccountCapability,
    pub(crate) requirement: ForwardingRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ForwardingRequirement {
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
pub(crate) fn credential_keys_match(surface: &str, canonical: &str, staged: &str) -> bool {
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

pub(crate) fn credential_scope_has_matching_proof(
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

pub(crate) fn binding_account_ids(binding: &ValidatedCredentialBinding) -> BTreeSet<String> {
    binding
        .provenance
        .iter()
        .filter_map(|provenance| provenance.strip_prefix("account "))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn binding_matches_proof(
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

pub(crate) fn record_key_binding(
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
pub(crate) fn authorize_credential_binding_group(
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
