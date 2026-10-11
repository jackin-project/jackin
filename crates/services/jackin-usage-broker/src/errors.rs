// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker coordination errors.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageIdentityKindV1};

use crate::{capability_for_binding, publish};
use jackin_usage_discovery::ValidatedUsageDiscovery;
use jackin_usage_host_accounts::CanonicalAccountSubject;

/// Preserve the canonical identity evidence that host discovery already
/// merged before the broker publisher turns generation views into the
/// Capsule-facing projection. Labels are deliberately not consulted.
pub(crate) fn publication_identity_metadata(
    discovery: &ValidatedUsageDiscovery,
) -> BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata> {
    let mut evidence =
        BTreeMap::<UsageAccountCapability, (UsageIdentityKindV1, BTreeSet<String>)>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        let identity_kind = match binding.identity.as_ref().map(|identity| &identity.subject) {
            Some(CanonicalAccountSubject::ProviderId(_)) => UsageIdentityKindV1::ProviderAccountId,
            Some(CanonicalAccountSubject::ProviderStableHandle(_)) => {
                UsageIdentityKindV1::ProviderStableHandle
            }
            Some(CanonicalAccountSubject::SourceCapability(_)) => {
                UsageIdentityKindV1::LocalSourceHandle
            }
            None => UsageIdentityKindV1::UnverifiedHandle,
        };
        let entry = evidence
            .entry(capability)
            .or_insert_with(|| (identity_kind, BTreeSet::new()));
        // A provider-issued id is stronger evidence than a stable display
        // handle if malformed input ever aliases them to one capability.
        if identity_kind == UsageIdentityKindV1::ProviderAccountId {
            entry.0 = UsageIdentityKindV1::ProviderAccountId;
        }
        entry.1.extend(binding.provenance.iter().cloned());
    }
    evidence
        .into_iter()
        .map(|(capability, (identity_kind, provenance))| {
            (
                capability,
                publish::AccountIdentityMetadata {
                    identity_kind,
                    provenance_count: u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                },
            )
        })
        .collect()
}
