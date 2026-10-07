// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! View merging and freshness.

use std::collections::BTreeMap;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageGenerationView, UsageMembershipStateV1, UsageProjectionRefreshStateV1, UsageProjectionV1,
    UsageProviderV1,
};

/// Server-side incremental publisher. Cheap to clone; all state is shared.
use super::{AccountIdentityMetadata, account_for_view};

/// Rebuild provider/account rows from per-account generation views.
///
/// Providers and accounts are rebuilt in settled `(surface_id, account_id)`
/// order with canonical ranks. Projection-level `unresolved`, `issues`, and
/// the catalog revision are preserved untouched.
pub fn merge_views(
    projection: &mut UsageProjectionV1,
    views: &[UsageGenerationView],
    identity_metadata: &BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
) {
    let mut ordered = views.to_vec();
    ordered.sort_by(|left, right| {
        (&left.capability.surface_id, &left.capability.account_id)
            .cmp(&(&right.capability.surface_id, &right.capability.account_id))
    });
    let any_active = ordered.iter().any(|view| view.phase.is_active());
    projection.refresh_state = if any_active {
        UsageProjectionRefreshStateV1::Refreshing
    } else {
        UsageProjectionRefreshStateV1::Idle
    };
    let mut providers: Vec<UsageProviderV1> = Vec::new();
    for view in &ordered {
        let surface_id = view.capability.surface_id.clone();
        if providers
            .last()
            .is_none_or(|provider: &UsageProviderV1| provider.provider_id != surface_id)
        {
            providers.push(UsageProviderV1 {
                provider_id: surface_id.clone(),
                display_name: surface_id.clone(),
                rank: u32::try_from(providers.len()).unwrap_or(u32::MAX),
                membership_state: UsageMembershipStateV1::Current,
                freshness: UsageFreshnessV1 {
                    generation: 0,
                    phase: UsageFreshnessPhaseV1::Failed,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                accounts: Vec::new(),
                issues: Vec::new(),
            });
        }
        let Some(provider) = providers.last_mut() else {
            continue;
        };
        let account = account_for_view(
            view,
            provider.accounts.len(),
            identity_metadata.get(&view.capability),
        );
        if let Some(snapshot) = &view.snapshot
            && !snapshot.account.provider_label.is_empty()
        {
            provider.display_name = snapshot.account.provider_label.clone();
        }
        provider.accounts.push(account);
    }
    for provider in &mut providers {
        let provider_active = ordered
            .iter()
            .filter(|view| view.capability.surface_id == provider.provider_id)
            .any(|view| view.phase.is_active());
        provider.freshness = aggregate_freshness(provider_active, &provider.accounts);
    }
    projection.providers = providers;
}

pub fn aggregate_freshness(any_active: bool, accounts: &[UsageAccountV1]) -> UsageFreshnessV1 {
    let mut freshness = UsageFreshnessV1 {
        generation: 0,
        phase: UsageFreshnessPhaseV1::Failed,
        last_good_at_epoch: None,
        retry_at_epoch: None,
        is_stale: false,
    };
    for account in accounts {
        freshness.generation = freshness.generation.max(account.freshness.generation);
        freshness.last_good_at_epoch = freshness
            .last_good_at_epoch
            .max(account.freshness.last_good_at_epoch);
        freshness.retry_at_epoch = [freshness.retry_at_epoch, account.freshness.retry_at_epoch]
            .into_iter()
            .flatten()
            .min();
        freshness.is_stale |= account.freshness.is_stale;
    }
    freshness.phase = if any_active {
        UsageFreshnessPhaseV1::Refreshing
    } else if accounts
        .iter()
        .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Failed)
    {
        UsageFreshnessPhaseV1::Failed
    } else if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV1::Stale)
    {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    freshness
}
