// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Discovery capability mapping.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCatalogEntry};

use crate::{ValidatedCredentialBinding, ValidatedUsageDiscovery};

/// Canonical broker capability for one validated credential binding.
pub fn capability_for_binding(
    binding: &ValidatedCredentialBinding,
    catalog_revision: Option<&str>,
) -> UsageAccountCapability {
    let subject = if let Some(identity) = &binding.identity {
        identity.account_key()
    } else {
        format!("provisional-capability-v1:{}", binding.capability_id)
    };
    let subject = match catalog_revision {
        Some(revision) => format!(
            "usage-capability-v2:catalog-revision:{}:{revision}:subject:{}:{subject}",
            revision.len(),
            subject.len()
        ),
        None => subject,
    };
    let hashed = jackin_core::account_key_hash(binding.surface.id(), &subject);
    let account_id = hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned();
    UsageAccountCapability {
        account_id,
        surface_id: binding.surface.id().to_owned(),
    }
}

/// Every canonical capability in one validated host discovery generation.
#[must_use]
pub fn usage_broker_capabilities(
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

/// Catalog entries with source revisions for one validated generation.
pub fn usage_catalog_entries(discovery: &ValidatedUsageDiscovery) -> Vec<UsageCatalogEntry> {
    let mut source_revisions = BTreeMap::<UsageAccountCapability, BTreeSet<String>>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        source_revisions
            .entry(capability)
            .or_default()
            .insert(format!(
                "{}:{}:{}:{}",
                binding.capability_id.len(),
                binding.capability_id,
                binding.credential_revision.len(),
                binding.credential_revision,
            ));
    }
    source_revisions
        .into_iter()
        .map(|(capability, source_revisions)| {
            let revision_material = source_revisions
                .iter()
                .map(|source_revision| format!("{}:{source_revision}", source_revision.len()))
                .collect::<Vec<_>>()
                .join("|");
            UsageCatalogEntry {
                revision: jackin_core::account_key_hash(
                    "usage-catalog-entry-v3",
                    &revision_material,
                ),
                capability,
            }
        })
        .collect()
}
