// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Revoked account retention.

use std::collections::BTreeMap;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV1, UsageCatalogEntry, UsageFreshnessPhaseV1,
    UsageLifecycleV1, UsageProjectionV1,
};

pub fn retain_revoked_accounts(
    projection: &mut UsageProjectionV1,
    previous: &UsageProjectionV1,
    catalog: &BTreeMap<UsageAccountCapability, String>,
    previous_catalog: Option<&BTreeMap<UsageAccountCapability, String>>,
) {
    for provider in &mut projection.providers {
        provider.accounts.retain(|account| {
            let capability = UsageAccountCapability {
                account_id: account.canonical_account_id.clone(),
                surface_id: provider.provider_id.clone(),
            };
            let revision_changed = previous_catalog.is_some_and(|previous_catalog| {
                previous_catalog
                    .get(&capability)
                    .zip(catalog.get(&capability))
                    .is_some_and(|(previous, current)| previous != current)
            });
            !(catalog.contains_key(&capability)
                && !revision_changed
                && is_revoked_tombstone(account))
        });
        for account in &mut provider.accounts {
            let capability = UsageAccountCapability {
                account_id: account.canonical_account_id.clone(),
                surface_id: provider.provider_id.clone(),
            };
            let revision_changed = previous_catalog.is_some_and(|previous_catalog| {
                previous_catalog
                    .get(&capability)
                    .zip(catalog.get(&capability))
                    .is_some_and(|(previous, current)| previous != current)
            });
            if !catalog.contains_key(&capability) || revision_changed {
                mark_revoked(account);
            }
        }
    }
    projection
        .providers
        .retain(|provider| !provider.accounts.is_empty());

    for previous_provider in &previous.providers {
        for previous_account in &previous_provider.accounts {
            let capability = UsageAccountCapability {
                account_id: previous_account.canonical_account_id.clone(),
                surface_id: previous_provider.provider_id.clone(),
            };
            if catalog.contains_key(&capability)
                || projection.providers.iter().any(|provider| {
                    provider.provider_id == capability.surface_id
                        && provider
                            .accounts
                            .iter()
                            .any(|account| account.canonical_account_id == capability.account_id)
                })
            {
                continue;
            }
            let mut account = previous_account.clone();
            mark_revoked(&mut account);
            if let Some(provider) = projection
                .providers
                .iter_mut()
                .find(|provider| provider.provider_id == capability.surface_id)
            {
                provider.accounts.push(account);
            } else {
                let mut provider = previous_provider.clone();
                provider.accounts = vec![account];
                projection.providers.push(provider);
            }
        }
    }

    projection
        .providers
        .sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    for (provider_rank, provider) in projection.providers.iter_mut().enumerate() {
        provider.rank = u32::try_from(provider_rank).unwrap_or(u32::MAX);
        provider
            .accounts
            .sort_by(|left, right| left.canonical_account_id.cmp(&right.canonical_account_id));
        for (account_rank, account) in provider.accounts.iter_mut().enumerate() {
            account.rank = u32::try_from(account_rank).unwrap_or(u32::MAX);
        }
    }
}

pub(crate) fn mark_revoked(account: &mut UsageAccountV1) {
    account.status_label = Some("removed".to_owned());
    account.lifecycle = UsageLifecycleV1::Unavailable;
    account.freshness.phase = UsageFreshnessPhaseV1::Failed;
    account.freshness.is_stale = true;
    account.windows.clear();
    account.metric_groups.clear();
    account.issues.clear();
}

pub(crate) fn is_revoked_tombstone(account: &UsageAccountV1) -> bool {
    account.status_label.as_deref() == Some("removed")
        && account.lifecycle == UsageLifecycleV1::Unavailable
        && account.freshness.phase == UsageFreshnessPhaseV1::Failed
        && account.freshness.is_stale
}

pub fn catalog_entries(
    catalog: &BTreeMap<UsageAccountCapability, String>,
) -> Vec<UsageCatalogEntry> {
    catalog
        .iter()
        .map(|(capability, revision)| UsageCatalogEntry {
            capability: capability.clone(),
            revision: revision.clone(),
        })
        .collect()
}
