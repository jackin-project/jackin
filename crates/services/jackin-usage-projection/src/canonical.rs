// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Canonical projection.

use std::collections::{BTreeMap, BTreeSet};

use icu_collator::{Collator, options::CollatorOptions, options::Strength};
use icu_locale::Locale;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageGenerationView, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageMembershipStateV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProjectionV1, UsageProviderV1, UsageUnresolvedV1,
};

use crate::{
    ProjectionMetadata, apply_generation_metadata, discovery_issue, failure_lifecycle,
    project_account, provider_freshness,
};
use jackin_usage_discovery::ValidatedUsageDiscovery;
use jackin_usage_host_accounts::AccountCatalog;
use jackin_usage_host_presentation::HostSurfaceId;

pub fn build_canonical_projection(
    catalog: &AccountCatalog,
    discovery: &ValidatedUsageDiscovery,
    broker_generations: &BTreeMap<UsageAccountCapability, UsageGenerationView>,
    metadata: ProjectionMetadata<'_>,
) -> Result<UsageProjectionV1, String> {
    let locale = metadata
        .locale
        .parse::<Locale>()
        .or_else(|_| "und".parse())
        .map_err(|error| format!("usage account locale unavailable: {error}"))?;
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    let collator = Collator::try_new(locale.into(), options)
        .map_err(|error| format!("usage account collation unavailable: {error}"))?;

    let members = discovery
        .accounts
        .iter()
        .map(|account| {
            (
                (account.identity.surface, account.account_key.as_str()),
                account,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut providers = Vec::new();
    for surface in HostSurfaceId::ALL.iter().copied() {
        let mut entries = catalog
            .entries_for_surface(surface)
            .into_iter()
            .filter(|entry| members.contains_key(&(surface, entry.account_key.as_str())))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            collator
                .compare(&left.account_label, &right.account_label)
                .then_with(|| {
                    left.identity
                        .canonical_id_v1()
                        .cmp(&right.identity.canonical_id_v1())
                })
        });

        let unresolved = discovery
            .unresolved_capabilities()
            .filter(|candidate| candidate.surface_id == surface.id())
            .count();
        let issues = discovery
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.surface_id.as_deref() == Some(surface.id()))
            .map(|diagnostic| discovery_issue(diagnostic.issue, UsageIssueScopeV1::Provider))
            .collect::<Vec<_>>();
        if entries.is_empty() && unresolved == 0 && issues.is_empty() {
            continue;
        }
        let accounts = entries
            .into_iter()
            .enumerate()
            .map(|(rank, entry)| {
                let mut account = project_account(entry, rank, metadata.broker_generation)?;
                let broker_state = discovery
                    .bindings
                    .iter()
                    .find(|binding| binding.identity.as_ref() == Some(&entry.identity))
                    .and_then(|binding| {
                        broker_generations.get(&jackin_usage_discovery::capability_for_binding(
                            binding,
                            discovery.config_generation.as_deref(),
                        ))
                    });
                if let Some(state) = broker_state {
                    apply_generation_metadata(&mut account, state);
                }
                Ok::<_, String>(account)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut canonical_ids = BTreeSet::new();
        if let Some(collision) = accounts
            .iter()
            .find(|account| !canonical_ids.insert(account.canonical_account_id.as_str()))
        {
            return Err(format!(
                "canonical account identity collision for {}",
                collision.canonical_account_id
            ));
        }
        let freshness = provider_freshness(&accounts, metadata.broker_generation);
        providers.push(UsageProviderV1 {
            provider_id: surface.provider_id().to_owned(),
            display_name: surface.label().to_owned(),
            rank: u32::try_from(providers.len()).map_err(|_| "provider rank overflow")?,
            membership_state: UsageMembershipStateV1::Current,
            freshness,
            accounts,
            issues,
        });
    }

    let unresolved = project_unresolved_capabilities(discovery, broker_generations);
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: metadata.projection_id.to_owned(),
        generated_at_epoch: metadata.generated_at_epoch,
        discovery_revision: discovery
            .config_generation
            .clone()
            .unwrap_or_else(|| "empty".to_owned()),
        broker_instance_id: metadata.broker_instance_id.to_owned(),
        broker_generation: metadata.broker_generation,
        refresh_state: if metadata.refreshing {
            UsageProjectionRefreshStateV1::Refreshing
        } else {
            UsageProjectionRefreshStateV1::Idle
        },
        providers,
        unresolved,
        issues: discovery
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.surface_id.is_none())
            .map(|diagnostic| discovery_issue(diagnostic.issue, UsageIssueScopeV1::Projection))
            .collect(),
    };
    projection.validate()?;
    Ok(projection)
}

pub(crate) fn project_unresolved_capabilities(
    discovery: &ValidatedUsageDiscovery,
    broker_generations: &BTreeMap<UsageAccountCapability, UsageGenerationView>,
) -> Vec<UsageUnresolvedV1> {
    let mut unresolved = discovery
        .unresolved_capabilities()
        .map(|candidate| {
            let broker_state = discovery
                .bindings
                .iter()
                .find(|binding| {
                    binding.capability_id == candidate.capability_id && binding.identity.is_none()
                })
                .and_then(|binding| {
                    broker_generations.get(&jackin_usage_discovery::capability_for_binding(
                        binding,
                        discovery.config_generation.as_deref(),
                    ))
                });
            let issues = broker_state
                .and_then(|state| {
                    state.error.as_ref().map(|error| UsageIssueV1 {
                        code: jackin_usage_broker_publish::issue_code(error.kind),
                        scope: UsageIssueScopeV1::Provider,
                        recoverability: jackin_usage_broker_publish::issue_recoverability(
                            error.kind,
                        ),
                        message: error.message.clone(),
                        retry_at_epoch: state.retry_at_epoch,
                    })
                })
                .into_iter()
                .collect();
            UsageUnresolvedV1 {
                provider_id: HostSurfaceId::from_id(&candidate.surface_id).map_or_else(
                    || candidate.surface_id.clone(),
                    |surface| surface.provider_id().to_owned(),
                ),
                capability_id: candidate.capability_id.clone(),
                configuration_count: u32::try_from(candidate.provenance.len()).unwrap_or(u32::MAX),
                state: broker_state
                    .and_then(|state| state.error.as_ref())
                    .map_or(UsageLifecycleV1::NeedsLogin, |error| {
                        failure_lifecycle(error.kind)
                    }),
                issues,
            }
        })
        .collect::<Vec<_>>();
    unresolved.sort_by(|left, right| {
        provider_rank(&left.provider_id)
            .cmp(&provider_rank(&right.provider_id))
            .then(left.capability_id.cmp(&right.capability_id))
    });
    unresolved
}

pub(crate) fn provider_rank(provider_id: &str) -> usize {
    HostSurfaceId::ALL
        .iter()
        .position(|surface| surface.provider_id() == provider_id)
        .unwrap_or(usize::MAX)
}
