// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage source validation entry points.

use super::{
    AccountAccumulator, CachingProfileCredentialReader, DiscoveredAccountDescriptor,
    ProfileCredentialReader, ProfileValidation, ProviderCredentialEnvResolver,
    SystemProfileCredentialReader, UsageDiscoveryCatalog, ValidatedCredentialSource,
    ValidatedSourceParts, ValidatedUsageDiscovery, accumulate_validated_source, validate_source,
};
use std::collections::{BTreeMap, BTreeSet};

use super::super::CanonicalAccountIdentity;
use super::super::CanonicalAccountSubject;
use super::super::HostSurfaceId;

/// Validate every pre-deduplicated source and merge authenticated identities.
///
/// Missing/malformed/denied sources produce diagnostics and never account rows.
pub fn validate_usage_sources(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ValidatedUsageDiscovery {
    validate_usage_sources_with_reader(catalog, env_resolver, &SystemProfileCredentialReader)
}

pub(crate) fn validate_usage_sources_with_reader(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedUsageDiscovery {
    let mut diagnostics = catalog.diagnostics;
    let mut bindings = Vec::new();
    let mut accounts = BTreeMap::<CanonicalAccountIdentity, AccountAccumulator>::new();

    let profile_reader = CachingProfileCredentialReader::new(profile_reader);
    let validated: Vec<ValidatedSourceParts> = catalog
        .sources
        .into_iter()
        .map(|source| validate_source(source, env_resolver, &profile_reader))
        .collect();
    // Provider-issued identities per surface, from any source form. An
    // anonymous env/key credential carries no identity evidence of its own;
    // when exactly one same-surface provider identity exists, the key joins
    // that canonical account instead of minting a source-scoped row.
    let mut strong = BTreeMap::<HostSurfaceId, BTreeSet<CanonicalAccountIdentity>>::new();
    for (surface, _, _, _, _, _, outcome) in &validated {
        if let ProfileValidation::Authenticated {
            provider_id: Some(id),
            ..
        } = outcome
            && !id.trim().is_empty()
        {
            strong
                .entry(*surface)
                .or_default()
                .insert(CanonicalAccountIdentity {
                    surface: *surface,
                    subject: CanonicalAccountSubject::ProviderId(id.trim().to_owned()),
                });
        }
    }
    let (primary, attachable): (Vec<ValidatedSourceParts>, Vec<ValidatedSourceParts>) = validated
        .into_iter()
        .partition(|parts| !is_attachable_env_source(&parts.5, &parts.6));
    // Strong sources accumulate first so canonical labels come from
    // authenticated evidence, never from an attached anonymous key.
    for parts in primary {
        accumulate_validated_source(parts, None, &mut diagnostics, &mut bindings, &mut accounts);
    }
    for parts in attachable {
        let attach_to = match strong.get(&parts.0) {
            Some(ids) if ids.len() == 1 => ids.iter().next().cloned(),
            _ => None,
        };
        accumulate_validated_source(
            parts,
            attach_to,
            &mut diagnostics,
            &mut bindings,
            &mut accounts,
        );
    }

    let accounts = accounts
        .into_iter()
        .map(|(identity, account)| DiscoveredAccountDescriptor {
            surface_id: identity.surface.id().to_owned(),
            account_key: identity.account_key(),
            account_label: account.label,
            provenance: account.provenance.into_iter().collect(),
            source_ids: account.source_ids.into_iter().collect(),
            identity,
        })
        .collect();

    ValidatedUsageDiscovery {
        config_generation: catalog.config_generation,
        accounts,
        diagnostics,
        candidates: catalog.candidates,
        bindings,
    }
}

/// Whether an env/key source proved no identity of its own.
///
/// Anonymous API-key/OAuth-token credentials are bearer material without
/// local identity evidence. Unlike profiles (distinct local logins) and
/// forwarded capabilities (a separate trust domain), they may join the one
/// same-surface provider-authenticated account when it exists.
pub(crate) fn is_attachable_env_source(
    source: &ValidatedCredentialSource,
    outcome: &ProfileValidation,
) -> bool {
    if !matches!(source, ValidatedCredentialSource::Env { .. }) {
        return false;
    }
    match outcome {
        ProfileValidation::Authenticated { provider_id, .. } => {
            provider_id.as_deref().is_none_or(|id| id.trim().is_empty())
        }
        ProfileValidation::Anonymous(_) => true,
        ProfileValidation::Missing
        | ProfileValidation::Denied
        | ProfileValidation::ConsentRequired
        | ProfileValidation::Malformed => false,
    }
}
