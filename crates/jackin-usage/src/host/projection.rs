// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Surface-neutral canonical usage projection.

use std::collections::{BTreeMap, BTreeSet};

use icu_collator::{Collator, options::CollatorOptions, options::Strength};
use icu_locale::Locale;
use jackin_core::account_key_hash;
use jackin_protocol::control::{
    CountQuotaPeriod, Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity,
    UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV2, UsageCalendarPeriodV2, UsageFreshnessPhaseV2,
    UsageFreshnessV2, UsageGenerationView, UsageIdentityKindV2, UsageIssueRecoverabilityV2,
    UsageIssueScopeV2, UsageIssueV2, UsageLifecycleV2, UsageLimitWindowV2, UsageMembershipStateV2,
    UsageMetricGroupKindV2, UsageMetricGroupV2, UsageMetricPeriodV2, UsageMetricScopeV2,
    UsageMetricValueV2, UsagePercent, UsageProjectionRefreshStateV2, UsageProjectionSchemaV2,
    UsageProjectionV2, UsageProviderV2, UsageQuotaStateV2, UsageUnresolvedV2,
    UsageWindowCategoryV2,
};

use super::accounts::{AccountCatalog, AccountCatalogEntry, CanonicalAccountSubject};
use super::{HostSurfaceId, HostUsageRuntime, ValidatedUsageDiscovery};

pub(super) struct ProjectionMetadata<'a> {
    pub projection_id: &'a str,
    pub generated_at_epoch: i64,
    pub broker_instance_id: &'a str,
    pub broker_generation: u64,
    pub refreshing: bool,
    pub locale: &'a str,
}

/// Typed interactive destination outside the canonical JSON projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageDestination {
    /// All current canonical accounts.
    Overview,
    /// Provider destination allowed only when it owns exactly one account.
    Provider { provider_id: String },
    /// Exact account destination for a multi-account provider.
    Account {
        provider_id: String,
        canonical_account_id: String,
    },
}

/// Result of reconciling adapter selection against one immutable publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUsageDestination {
    pub destination: UsageDestination,
    pub notice: Option<String>,
}

/// Preserve a stable destination or return honestly to Overview when removed.
#[must_use]
pub fn normalize_destination(
    projection: &UsageProjectionV2,
    requested: &UsageDestination,
) -> NormalizedUsageDestination {
    let valid = match requested {
        UsageDestination::Overview => true,
        UsageDestination::Provider { provider_id } => projection
            .providers
            .iter()
            .any(|provider| provider.provider_id == *provider_id && provider.accounts.len() == 1),
        UsageDestination::Account {
            provider_id,
            canonical_account_id,
        } => projection.providers.iter().any(|provider| {
            provider.provider_id == *provider_id
                && provider
                    .accounts
                    .iter()
                    .any(|account| account.canonical_account_id == *canonical_account_id)
        }),
    };
    if valid {
        NormalizedUsageDestination {
            destination: requested.clone(),
            notice: None,
        }
    } else {
        NormalizedUsageDestination {
            destination: UsageDestination::Overview,
            notice: Some("Selected account is no longer available.".to_owned()),
        }
    }
}

impl HostUsageRuntime {
    /// Build the immutable surface-neutral V2 publication from current discovery.
    pub fn canonical_projection(&mut self, locale: &str) -> Result<UsageProjectionV2, String> {
        self.require_open()?;
        let generation = super::broker::publish::next_publication_generation(
            self.canonical_projection_cache
                .as_ref()
                .map_or(0, |projection| projection.broker_generation),
        )
        .map_err(|error| error.message)?;
        let aliases = self
            .discovery
            .as_ref()
            .ok_or_else(|| "usage discovery has not completed".to_owned())?
            .canonical_aliases()
            .map(|(capability_id, identity)| (capability_id.to_owned(), identity.clone()))
            .collect::<Vec<_>>();
        for (capability_id, identity) in aliases {
            let _canonical_id = self
                .canonical_identity_graph
                .resolve_alias(&capability_id, &identity)?;
        }
        let catalog = self.materialize_account_catalog()?;
        let discovery = self
            .discovery
            .as_ref()
            .ok_or_else(|| "usage discovery has not completed".to_owned())?;
        let draft = build_canonical_projection(
            &catalog,
            discovery,
            &self.broker_generations,
            ProjectionMetadata {
                projection_id: "draft",
                generated_at_epoch: 0,
                broker_instance_id: &self.canonical_instance_id,
                broker_generation: 0,
                refreshing: self.broker_refresh_in_progress(),
                locale,
            },
        )?;
        let content = serde_json::to_string(&draft)
            .map_err(|error| format!("canonical usage projection failed: {error}"))?;
        let content_id = account_key_hash("usage-projection-content-v1", &content);
        if self.canonical_content_id.as_deref() == Some(content_id.as_str()) {
            return self
                .canonical_projection_cache
                .clone()
                .ok_or_else(|| "canonical usage projection cache missing".to_owned());
        }
        let projection_id = format!("{}:{generation:020}", self.canonical_instance_id);
        let projection = build_canonical_projection(
            &catalog,
            discovery,
            &self.broker_generations,
            ProjectionMetadata {
                projection_id: &projection_id,
                generated_at_epoch: chrono::Utc::now().timestamp(),
                broker_instance_id: &self.canonical_instance_id,
                broker_generation: generation,
                refreshing: self.broker_refresh_in_progress(),
                locale,
            },
        )?;
        self.canonical_content_id = Some(content_id);
        self.canonical_projection_cache = Some(projection.clone());
        Ok(projection)
    }
}

pub(super) fn build_canonical_projection(
    catalog: &AccountCatalog,
    discovery: &ValidatedUsageDiscovery,
    broker_generations: &BTreeMap<UsageAccountCapability, UsageGenerationView>,
    metadata: ProjectionMetadata<'_>,
) -> Result<UsageProjectionV2, String> {
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
    let accepted_catalog = super::broker::usage_catalog_entries(discovery)
        .into_iter()
        .map(|entry| (entry.capability.clone(), entry))
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
            .map(|diagnostic| discovery_issue(diagnostic.issue, UsageIssueScopeV2::Provider))
            .collect::<Vec<_>>();
        if entries.is_empty() && unresolved == 0 && issues.is_empty() {
            continue;
        }
        let accounts = entries
            .into_iter()
            .enumerate()
            .map(|(rank, entry)| {
                let refresh_capabilities = discovery
                    .bindings
                    .iter()
                    .filter(|binding| binding.identity.as_ref() == Some(&entry.identity))
                    .map(|binding| {
                        super::broker::capability_for_binding(
                            binding,
                            discovery.config_generation.as_deref(),
                        )
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let states = refresh_capabilities
                    .iter()
                    .filter_map(|route| broker_generations.get(route))
                    .filter(|state| {
                        let Some(accepted) = accepted_catalog.get(&state.capability) else {
                            return false;
                        };
                        state.snapshot.as_ref().is_none_or(|snapshot| {
                            snapshot.canonical_identity == accepted.canonical_identity
                                && snapshot.account_identity.as_ref().is_some_and(|route| {
                                    route.account_id == accepted.capability.account_id
                                        && route.surface_id == accepted.capability.surface_id
                                        && (!matches!(
                                            entry.identity.subject,
                                            CanonicalAccountSubject::SourceCapability(_)
                                        ) || route.source_revision.as_deref()
                                            == Some(accepted.revision.as_str()))
                                })
                        })
                    })
                    .collect::<Vec<_>>();
                let broker_state = states.iter().copied().max_by_key(|state| {
                    (
                        state
                            .snapshot
                            .as_ref()
                            .is_some_and(|view| view_is_usable(view.status)),
                        state
                            .snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.fetched_at_epoch),
                        &state.capability,
                    )
                });
                let mut observation = entry.clone();
                if let Some(state) = broker_state
                    && let Some(view) = &state.snapshot
                {
                    observation.view = view.clone();
                    observation.view.canonical_identity = Some(entry.identity.protocol_identity());
                    if view_is_usable(observation.view.status)
                        && accepted_catalog
                            .get(&state.capability)
                            .is_some_and(|accepted| {
                                view.account_identity.as_ref().is_none_or(|route| {
                                    route.source_revision.as_deref()
                                        != Some(accepted.revision.as_str())
                                })
                            })
                    {
                        observation.view.status = UsageSnapshotStatus::Stale;
                    }
                    if let Some(error) = &state.error {
                        observation.view.last_error = Some(error.message.clone());
                        if view_is_usable(observation.view.status) {
                            observation.view.status = UsageSnapshotStatus::Stale;
                        }
                    }
                    observation.username = observation.view.account.username.clone();
                    observation.plan_label = observation.view.account.plan_label.clone();
                    observation.fetched_at_epoch = observation.view.fetched_at_epoch;
                }
                let mut account = project_account(&observation, rank, metadata.broker_generation)?;
                account.provenance_count = refresh_capabilities
                    .iter()
                    .filter_map(|route| accepted_catalog.get(route))
                    .map(|accepted| accepted.provenance_count)
                    .next()
                    .ok_or_else(|| {
                        "canonical account has no accepted source provenance".to_owned()
                    })?;
                account.refresh_capabilities = refresh_capabilities;
                if let Some(state) = broker_state {
                    apply_generation_metadata(&mut account, state);
                }
                for state in &states {
                    if let Some(error) = &state.error {
                        let issue = UsageIssueV2 {
                            code: super::broker::publish::issue_code(error.kind),
                            scope: UsageIssueScopeV2::Account,
                            recoverability: super::broker::publish::issue_recoverability(
                                error.kind,
                            ),
                            message: error.message.clone(),
                            retry_at_epoch: state.retry_at_epoch,
                        };
                        if !account.issues.contains(&issue) {
                            account.issues.push(issue);
                        }
                    }
                }
                if states.iter().any(|state| state.phase.is_active()) {
                    account.freshness.phase = UsageFreshnessPhaseV2::Refreshing;
                }
                account.freshness.retry_at_epoch =
                    states.iter().filter_map(|state| state.retry_at_epoch).min();
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
        providers.push(UsageProviderV2 {
            provider_id: surface.provider_id().to_owned(),
            display_name: surface.label().to_owned(),
            rank: u32::try_from(providers.len()).map_err(|_| "provider rank overflow")?,
            membership_state: UsageMembershipStateV2::Current,
            freshness,
            accounts,
            issues,
        });
    }

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
                    broker_generations.get(&super::broker::capability_for_binding(
                        binding,
                        discovery.config_generation.as_deref(),
                    ))
                });
            let issues = broker_state
                .and_then(|state| {
                    state.error.as_ref().map(|error| UsageIssueV2 {
                        code: super::broker::publish::issue_code(error.kind),
                        scope: UsageIssueScopeV2::Provider,
                        recoverability: super::broker::publish::issue_recoverability(error.kind),
                        message: error.message.clone(),
                        retry_at_epoch: state.retry_at_epoch,
                    })
                })
                .into_iter()
                .collect();
            UsageUnresolvedV2 {
                provider_id: HostSurfaceId::from_id(&candidate.surface_id).map_or_else(
                    || candidate.surface_id.clone(),
                    |surface| surface.provider_id().to_owned(),
                ),
                capability_id: candidate.capability_id.clone(),
                configuration_count: u32::try_from(candidate.provenance.len()).unwrap_or(u32::MAX),
                state: broker_state
                    .and_then(|state| state.error.as_ref())
                    .map_or_else(
                        || {
                            if discovery.candidate_is_deferred(candidate) {
                                UsageLifecycleV2::Unavailable
                            } else {
                                UsageLifecycleV2::NeedsLogin
                            }
                        },
                        |error| failure_lifecycle(error.kind),
                    ),
                issues,
            }
        })
        .collect::<Vec<_>>();
    unresolved.sort_by(|left, right| {
        provider_rank(&left.provider_id)
            .cmp(&provider_rank(&right.provider_id))
            .then(left.capability_id.cmp(&right.capability_id))
    });
    let projection = UsageProjectionV2 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: metadata.projection_id.to_owned(),
        generated_at_epoch: metadata.generated_at_epoch,
        discovery_revision: discovery
            .config_generation
            .clone()
            .unwrap_or_else(|| "empty".to_owned()),
        broker_instance_id: metadata.broker_instance_id.to_owned(),
        broker_generation: metadata.broker_generation,
        refresh_state: if metadata.refreshing {
            UsageProjectionRefreshStateV2::Refreshing
        } else {
            UsageProjectionRefreshStateV2::Idle
        },
        providers,
        unresolved,
        unresolved_grants: Vec::new(),
        issues: discovery
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.surface_id.is_none())
            .map(|diagnostic| discovery_issue(diagnostic.issue, UsageIssueScopeV2::Projection))
            .collect(),
    };
    projection.validate()?;
    Ok(projection)
}

fn provider_rank(provider_id: &str) -> usize {
    HostSurfaceId::ALL
        .iter()
        .position(|surface| surface.provider_id() == provider_id)
        .unwrap_or(usize::MAX)
}

fn project_account(
    entry: &AccountCatalogEntry,
    rank: usize,
    generation: u64,
) -> Result<UsageAccountV2, String> {
    let canonical_account_id = entry.identity.canonical_id_v1();
    let lifecycle = lifecycle(entry.view.status, entry.view.confidence);
    let freshness = freshness(entry.view.status, entry.view.fetched_at_epoch, generation);
    let windows = entry
        .view
        .buckets
        .iter()
        .enumerate()
        .map(|(window_rank, bucket)| project_window(&canonical_account_id, bucket, window_rank))
        .collect::<Result<Vec<_>, _>>()?;
    let metric_groups = project_groups(
        &entry.view,
        entry.plan_label.as_deref(),
        &canonical_account_id,
    )?;
    Ok(UsageAccountV2 {
        canonical_account_id,
        refresh_capabilities: Vec::new(),
        username: entry.username.clone(),
        auth_origin: entry.view.account.credential_origin.clone(),
        identity_kind: match entry.identity.subject {
            CanonicalAccountSubject::ProviderId(_) => UsageIdentityKindV2::ProviderAccountId,
            CanonicalAccountSubject::ProviderStableHandle(_) => {
                UsageIdentityKindV2::ProviderStableHandle
            }
            CanonicalAccountSubject::SourceCapability(_) => UsageIdentityKindV2::SourceCapability,
        },
        rank: u32::try_from(rank).map_err(|_| "account rank overflow")?,
        display_label: entry.account_label.clone(),
        plan_label: entry.plan_label.clone(),
        status_label: Some(status_label(entry.view.status).to_owned()),
        lifecycle,
        freshness,
        provenance_count: u32::try_from(entry.discovery_provenance.len()).unwrap_or(u32::MAX),
        windows,
        metric_groups,
        // No credential-expiry signal exists in current provider views; the
        // field stays unset rather than borrowing a quota reset timestamp.
        credential_expires_at_epoch: None,
        issues: view_issues(&entry.view),
    })
}

fn discovery_issue(
    issue: super::discovery::UsageDiscoveryIssue,
    scope: UsageIssueScopeV2,
) -> UsageIssueV2 {
    use super::discovery::UsageDiscoveryIssue;
    let recoverability = match issue {
        UsageDiscoveryIssue::ConfigVersionUnsupported => UsageIssueRecoverabilityV2::Unsupported,
        UsageDiscoveryIssue::ConfigTransientConflict
        | UsageDiscoveryIssue::CredentialUnavailable => UsageIssueRecoverabilityV2::Retryable,
        _ => UsageIssueRecoverabilityV2::ActionRequired,
    };
    UsageIssueV2 {
        code: issue.id().to_owned(),
        scope,
        recoverability,
        message: issue.display_message().to_owned(),
        retry_at_epoch: None,
    }
}

/// Typed broker state supplies recovery policy; display text never controls it.
fn apply_generation_metadata(account: &mut UsageAccountV2, state: &UsageGenerationView) {
    account.freshness.generation = state.generation;
    account.freshness.retry_at_epoch = state.retry_at_epoch;
    account.freshness.last_good_at_epoch = state
        .snapshot
        .as_ref()
        .filter(|snapshot| view_is_usable(snapshot.status))
        .map(|snapshot| snapshot.fetched_at_epoch)
        .or(account.freshness.last_good_at_epoch);
    account.freshness.is_stale = account.freshness.is_stale
        || (state.error.is_some() || state.snapshot.is_none())
            && account.freshness.last_good_at_epoch.is_some()
        || state
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.status == UsageSnapshotStatus::Stale);
    account.freshness.phase = if state.phase.is_active() {
        UsageFreshnessPhaseV2::Refreshing
    } else if account.freshness.is_stale {
        UsageFreshnessPhaseV2::Stale
    } else if state.error.is_some() || state.snapshot.is_none() {
        UsageFreshnessPhaseV2::Failed
    } else {
        account.freshness.phase
    };
    if account.freshness.is_stale {
        for group in &mut account.metric_groups {
            if group.last_success_at_epoch.is_some() {
                group.is_stale = true;
                group.phase = UsageFreshnessPhaseV2::Stale;
            }
        }
    }
    if let Some(error) = &state.error {
        if state.snapshot.is_none() {
            account.lifecycle = failure_lifecycle(error.kind);
        }
        account.issues = vec![UsageIssueV2 {
            code: super::broker::publish::issue_code(error.kind),
            scope: UsageIssueScopeV2::Account,
            recoverability: super::broker::publish::issue_recoverability(error.kind),
            message: error.message.clone(),
            retry_at_epoch: state.retry_at_epoch,
        }];
    }
}

pub(in crate::host) fn failure_lifecycle(
    kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind,
) -> UsageLifecycleV2 {
    use jackin_protocol::usage_broker::UsageCoordinationErrorKind;
    match kind {
        UsageCoordinationErrorKind::NeedsSecret => UsageLifecycleV2::NeedsSecret,
        UsageCoordinationErrorKind::Unauthorized => UsageLifecycleV2::NeedsLogin,
        UsageCoordinationErrorKind::ProtocolMismatch => UsageLifecycleV2::Unsupported,
        UsageCoordinationErrorKind::Unavailable
        | UsageCoordinationErrorKind::ProviderUnavailable => UsageLifecycleV2::Unavailable,
        _ => UsageLifecycleV2::Error,
    }
}

fn view_issues(view: &jackin_protocol::control::FocusedUsageView) -> Vec<UsageIssueV2> {
    let Some(message) = view
        .last_error
        .as_ref()
        .filter(|message| !message.trim().is_empty())
    else {
        return Vec::new();
    };
    let (code, recoverability) = match view.status {
        UsageSnapshotStatus::NeedsLogin => {
            ("needs_login", UsageIssueRecoverabilityV2::ActionRequired)
        }
        UsageSnapshotStatus::NeedsSecret => {
            ("needs_secret", UsageIssueRecoverabilityV2::ActionRequired)
        }
        UsageSnapshotStatus::Unsupported => {
            ("unsupported", UsageIssueRecoverabilityV2::Unsupported)
        }
        UsageSnapshotStatus::Fresh
        | UsageSnapshotStatus::Stale
        | UsageSnapshotStatus::Unavailable
        | UsageSnapshotStatus::Error => (
            "provider_unavailable",
            UsageIssueRecoverabilityV2::Retryable,
        ),
    };
    vec![UsageIssueV2 {
        code: code.to_owned(),
        scope: UsageIssueScopeV2::Account,
        recoverability,
        message: message.clone(),
        retry_at_epoch: None,
    }]
}

pub(in crate::host) fn project_window(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    rank: usize,
) -> Result<UsageLimitWindowV2, String> {
    bucket.validate_count_representation()?;
    if !money_representation_valid(bucket) {
        return Err(format!(
            "quota bucket {} has invalid monetary quantities",
            bucket.label
        ));
    }
    let raw_used = bucket
        .count_quota
        .is_none()
        .then(|| money_used_raw_percent(bucket))
        .flatten();
    let overage = bucket.remaining_money.is_none()
        && matches!((bucket.used_money.as_ref(), bucket.limit_money.as_ref()),
            (Some(used), Some(limit)) if used.exact_cmp(limit) == Some(std::cmp::Ordering::Greater));
    let typed_money = bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.remaining_money.is_some();
    let money_geometry = typed_money.then(|| money_window_geometry(bucket));
    let (remaining_percent, remaining_raw_percent) =
        if let Some((remaining, raw, _, _)) = money_geometry {
            (remaining, raw)
        } else if overage {
            (None, None)
        } else if let Some(value) = bucket
            .count_quota
            .as_ref()
            .map_or(bucket.remaining_percent, |count| count.remaining_percent())
        {
            let (raw, clamped) = UsagePercent::split_raw(i32::from(value));
            (Some(clamped), Some(raw))
        } else {
            (None, None)
        };
    let (used_percent, used_raw_percent) = if let Some((_, _, used, raw)) = money_geometry {
        (used, raw)
    } else if overage || remaining_percent.is_none() {
        if let Some(raw) = raw_used {
            let (_, clamped) = UsagePercent::split_raw(raw);
            (Some(clamped), Some(raw))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    let value_label = if let Some(count) = &bucket.count_quota {
        crate::usage::usage_count_quota_summary(count)
    } else if overage {
        raw_used.map_or_else(
            || crate::usage::usage_money_quota_summary(bucket),
            |raw| format!("{raw}% used"),
        )
    } else {
        remaining_percent.map_or_else(
            || {
                bucket.remaining_money.as_ref().map_or_else(
                    || bucket.used_label.clone().unwrap_or_default(),
                    |remaining| format!("{remaining} left"),
                )
            },
            |value| {
                format!(
                    "{}% left",
                    remaining_raw_percent.unwrap_or(i32::from(value.get()))
                )
            },
        )
    };
    Ok(UsageLimitWindowV2 {
        window_id: account_key_hash(canonical_account_id, &format!("canonical-window-v1:{rank}")),
        rank: u32::try_from(rank).map_err(|_| "window rank overflow")?,
        category: bucket.count_quota.as_ref().map_or_else(
            || window_category(bucket.status_slot),
            |count| match count.period {
                CountQuotaPeriod::UtcDaily => UsageWindowCategoryV2::LongRange,
                CountQuotaPeriod::Unknown => UsageWindowCategoryV2::Other,
            },
        ),
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        reset_at_epoch: bucket.resets_at,
        count_quota: bucket.count_quota.clone(),
        quota_state: quota_state(bucket),
        pace_label: bucket.pace_label.clone(),
        // No run-out signal exists outside the provider pace composite (which
        // already reaches both surfaces via `pace_label`); the field stays
        // unset rather than deriving a burn-rate estimate no producer stands
        // behind.
        runs_out_label: None,
    })
}

const fn window_category(status_slot: Option<StatusSlot>) -> UsageWindowCategoryV2 {
    match status_slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV2::LongRange,
        Some(StatusSlot::Session) => UsageWindowCategoryV2::Session,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV2::Other,
    }
}

/// Raw used percentage from a monetary bucket, unclamped so over-100% overage
/// survives. One shared [`Money::raw_percent_of`] rule with the capsule
/// bucket presentation, so both surfaces recover the same overage magnitude.
fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    if !money_representation_valid(bucket) {
        return None;
    }
    bucket
        .used_money
        .as_ref()?
        .raw_percent_of(bucket.limit_money.as_ref()?)
}

/// Build typed metric groups from the existing provider view.
///
/// One window group mirrors each quota bucket; monetary buckets additionally
/// yield a spend-cap group carrying the structured [`Money`] amounts; a
/// provider plan label yields a plan group. Groups reuse the view's fetched
/// timestamp for their own observed/fetched/last-success epochs: current views
/// report transport completion only, so observation time equals fetch time and
/// last success is set exactly when the view holds usable data. Scope labels,
/// balances, token totals, and rate limits stay unset until provider
/// collectors supply them; nothing is inferred.
pub(crate) fn metric_groups_for_view(
    canonical_account_id: &str,
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
) -> Result<Vec<UsageMetricGroupV2>, String> {
    project_groups(view, plan_label, canonical_account_id)
}

fn project_groups(
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
    canonical_account_id: &str,
) -> Result<Vec<UsageMetricGroupV2>, String> {
    let mut groups = Vec::new();
    for bucket in &view.buckets {
        let rank = groups.len();
        groups.push(project_window_group(
            canonical_account_id,
            bucket,
            view.status,
            view.fetched_at_epoch,
            rank,
        )?);
        if bucket.used_money.is_some()
            || bucket.limit_money.is_some()
            || bucket.remaining_money.is_some()
        {
            let rank = groups.len();
            groups.push(project_spend_group(
                canonical_account_id,
                bucket,
                view.status,
                view.fetched_at_epoch,
                rank,
            )?);
        }
    }
    if let Some(plan_label) = plan_label {
        let rank = groups.len();
        groups.push(project_plan_group(
            canonical_account_id,
            view.status,
            view.fetched_at_epoch,
            plan_label,
            rank,
        )?);
    }
    for (group_rank, group) in groups.iter().enumerate() {
        group.validate(group_rank)?;
    }
    Ok(groups)
}

fn group_id(canonical_account_id: &str, rank: usize) -> String {
    account_key_hash(canonical_account_id, &format!("canonical-group-v1:{rank}"))
}

fn group_rank(rank: usize) -> Result<u32, String> {
    u32::try_from(rank).map_err(|_| "metric group rank overflow".to_owned())
}

/// Per-group timestamps from a view that reports transport completion only.
fn group_epochs(view_fetched_at: i64, usable: bool) -> (Option<i64>, Option<i64>) {
    let observed = Some(view_fetched_at);
    let last_success = usable.then_some(view_fetched_at);
    (observed, last_success)
}

fn project_window_group(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    rank: usize,
) -> Result<UsageMetricGroupV2, String> {
    let window = project_window(canonical_account_id, bucket, rank)?;
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    Ok(UsageMetricGroupV2 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV2::Window,
        label: bucket.label.clone(),
        scope: UsageMetricScopeV2::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV2::Stale,
        quota_state: window.quota_state,
        value: UsageMetricValueV2::Window {
            remaining_percent: window.remaining_percent,
            remaining_raw_percent: window.remaining_raw_percent,
            used_percent: window.used_percent,
            used_raw_percent: window.used_raw_percent,
            period: bucket.count_quota.as_ref().map_or_else(
                || group_period(bucket.status_slot),
                |count| match count.period {
                    CountQuotaPeriod::UtcDaily => UsageMetricPeriodV2::Calendar {
                        granularity: UsageCalendarPeriodV2::Daily,
                    },
                    CountQuotaPeriod::Unknown => UsageMetricPeriodV2::Unknown,
                },
            ),
            unit: bucket.count_quota.as_ref().map(|_| "requests".to_owned()),
            count_quota: bucket.count_quota.clone(),
        },
        reset_at_epoch: bucket.resets_at,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

fn project_spend_group(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    rank: usize,
) -> Result<UsageMetricGroupV2, String> {
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    let quota_state = spend_quota_state(bucket);
    Ok(UsageMetricGroupV2 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV2::SpendCap,
        label: format!("{} spend", bucket.label),
        scope: UsageMetricScopeV2::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV2::Stale,
        quota_state,
        value: UsageMetricValueV2::SpendCap {
            cap: bucket.limit_money.clone(),
            spent: bucket.used_money.clone(),
            remaining: spend_remaining(bucket),
        },
        reset_at_epoch: bucket.resets_at,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

fn project_plan_group(
    canonical_account_id: &str,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    plan_label: &str,
    rank: usize,
) -> Result<UsageMetricGroupV2, String> {
    let phase = group_phase(view_status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(view_status));
    Ok(UsageMetricGroupV2 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV2::Plan,
        label: "Plan".to_owned(),
        scope: UsageMetricScopeV2::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV2::Stale,
        // Plan metadata carries no quota notion.
        quota_state: UsageQuotaStateV2::NotApplicable,
        value: UsageMetricValueV2::Plan {
            plan_label: Some(plan_label.to_owned()),
            tier: None,
        },
        reset_at_epoch: None,
        // No renewal signal exists in current provider views.
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

fn view_is_usable(status: UsageSnapshotStatus) -> bool {
    matches!(
        status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

fn group_phase(
    bucket_status: UsageSnapshotStatus,
    view_status: UsageSnapshotStatus,
) -> UsageFreshnessPhaseV2 {
    if view_status == UsageSnapshotStatus::Stale {
        return UsageFreshnessPhaseV2::Stale;
    }
    match bucket_status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV2::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV2::Stale,
        _ => UsageFreshnessPhaseV2::Failed,
    }
}

fn group_period(status_slot: Option<StatusSlot>) -> UsageMetricPeriodV2 {
    match status_slot {
        Some(StatusSlot::Session) => UsageMetricPeriodV2::ProviderDefined,
        Some(StatusSlot::Daily) => UsageMetricPeriodV2::Calendar {
            granularity: UsageCalendarPeriodV2::Daily,
        },
        Some(StatusSlot::Weekly) => UsageMetricPeriodV2::Calendar {
            granularity: UsageCalendarPeriodV2::Weekly,
        },
        Some(StatusSlot::Spend) | None => UsageMetricPeriodV2::Unknown,
    }
}

/// Provider remaining wins. Otherwise exact checked subtraction preserves debt.
fn spend_remaining(bucket: &QuotaBucketView) -> Option<Money> {
    if !money_representation_valid(bucket) {
        return None;
    }
    bucket.remaining_money.clone().or_else(|| {
        bucket
            .limit_money
            .as_ref()?
            .checked_sub(bucket.used_money.as_ref()?)
    })
}

fn money_representation_valid(bucket: &QuotaBucketView) -> bool {
    if bucket
        .used_money
        .as_ref()
        .is_some_and(|used| used.amount_minor < 0)
        || bucket
            .limit_money
            .as_ref()
            .is_some_and(|limit| limit.amount_minor < 0)
    {
        return false;
    }
    let mut currency = None;
    for money in [
        &bucket.used_money,
        &bucket.limit_money,
        &bucket.remaining_money,
    ]
    .into_iter()
    .flatten()
    {
        if currency.is_some_and(|currency| currency != money.currency.as_str()) {
            return false;
        }
        currency = Some(money.currency.as_str());
    }
    true
}

/// Exact quantities own state. Ratios supply geometry only.
fn money_window_geometry(
    bucket: &QuotaBucketView,
) -> (
    Option<UsagePercent>,
    Option<i32>,
    Option<UsagePercent>,
    Option<i32>,
) {
    if !money_representation_valid(bucket) {
        return (None, None, None, None);
    }
    let limit = bucket.limit_money.as_ref();
    if limit.is_some_and(|limit| limit.amount_minor == 0) {
        return (Some(UsagePercent::clamp_raw(0)), None, None, None);
    }
    if bucket.remaining_money.is_none()
        && matches!((bucket.used_money.as_ref(), limit),
            (Some(used), Some(limit)) if used.exact_cmp(limit) == Some(std::cmp::Ordering::Greater))
    {
        return (
            None,
            None,
            Some(UsagePercent::clamp_raw(100)),
            money_used_raw_percent(bucket),
        );
    }
    let Some(remaining) = spend_remaining(bucket) else {
        return (None, None, None, None);
    };
    let raw = limit.and_then(|limit| remaining.raw_percent_of(limit));
    let geometry = limit
        .and_then(|limit| remaining.remaining_percent_of(limit))
        .map(|percent| UsagePercent::clamp_raw(i32::from(percent)))
        .or_else(|| (remaining.amount_minor <= 0).then(|| UsagePercent::clamp_raw(0)));
    (geometry, raw, None, None)
}

/// Quota state for a spend-cap group from its money ratio. A missing or
/// unusable cap is [`UsageQuotaStateV2::Unknown`], never fabricated credit;
/// a missing cap never proves unlimited credit.
fn spend_quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV2 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV2::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV2::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV2::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV2::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if !money_representation_valid(bucket) {
                return UsageQuotaStateV2::Unknown;
            }
            if bucket
                .limit_money
                .as_ref()
                .is_some_and(|limit| limit.amount_minor == 0)
            {
                return UsageQuotaStateV2::Exhausted;
            }
            if let Some(remaining) = &bucket.remaining_money {
                if remaining.amount_minor <= 0 {
                    return UsageQuotaStateV2::Exhausted;
                }
                if let Some(limit) = &bucket.limit_money {
                    return if remaining
                        .percent_cmp(limit, 20)
                        .is_some_and(|ordering| ordering != std::cmp::Ordering::Greater)
                    {
                        UsageQuotaStateV2::Warning
                    } else {
                        UsageQuotaStateV2::Available
                    };
                }
                return UsageQuotaStateV2::Available;
            }
            match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
                (Some(used), Some(limit)) => spend_ratio_state(used, limit),
                _ => UsageQuotaStateV2::Unknown,
            }
        }
    }
}

/// Quota state from a spend/cap money ratio with checked math.
fn spend_ratio_state(used: &Money, limit: &Money) -> UsageQuotaStateV2 {
    if used.currency != limit.currency || limit.amount_minor < 0 || used.amount_minor < 0 {
        return UsageQuotaStateV2::Unknown;
    }
    if used
        .exact_cmp(limit)
        .is_some_and(|ordering| ordering != std::cmp::Ordering::Less)
    {
        UsageQuotaStateV2::Exhausted
    } else if used
        .percent_cmp(limit, 80)
        .is_some_and(|ordering| ordering != std::cmp::Ordering::Less)
    {
        UsageQuotaStateV2::Warning
    } else {
        UsageQuotaStateV2::Available
    }
}

pub(in crate::host) fn lifecycle(
    status: UsageSnapshotStatus,
    confidence: UsageConfidence,
) -> UsageLifecycleV2 {
    if confidence == UsageConfidence::PresenceOnly {
        return UsageLifecycleV2::AgentUninitialized;
    }
    match status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => UsageLifecycleV2::Available,
        UsageSnapshotStatus::NeedsLogin => UsageLifecycleV2::NeedsLogin,
        UsageSnapshotStatus::NeedsSecret => UsageLifecycleV2::NeedsSecret,
        UsageSnapshotStatus::Unsupported => UsageLifecycleV2::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageLifecycleV2::Unavailable,
        UsageSnapshotStatus::Error => UsageLifecycleV2::Error,
    }
}

fn freshness(status: UsageSnapshotStatus, last_good: i64, generation: u64) -> UsageFreshnessV2 {
    let phase = match status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV2::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV2::Stale,
        _ => UsageFreshnessPhaseV2::Failed,
    };
    UsageFreshnessV2 {
        generation,
        phase,
        last_good_at_epoch: matches!(
            status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        )
        .then_some(last_good),
        retry_at_epoch: None,
        is_stale: status == UsageSnapshotStatus::Stale,
    }
}

fn provider_freshness(accounts: &[UsageAccountV2], generation: u64) -> UsageFreshnessV2 {
    let is_stale = accounts.iter().any(|account| account.freshness.is_stale);
    let phase = if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV2::Refreshing)
    {
        UsageFreshnessPhaseV2::Refreshing
    } else if is_stale {
        UsageFreshnessPhaseV2::Stale
    } else if accounts.is_empty()
        || accounts
            .iter()
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV2::Failed)
    {
        UsageFreshnessPhaseV2::Failed
    } else {
        UsageFreshnessPhaseV2::Current
    };
    UsageFreshnessV2 {
        generation,
        phase,
        last_good_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.last_good_at_epoch)
            .max(),
        retry_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.retry_at_epoch)
            .min(),
        is_stale,
    }
}

/// Quota state for one bucket with no collapsing: missing permission stays
/// [`UsageQuotaStateV2::NoPermission`] (never [`UsageQuotaStateV2::Unsupported`]),
/// and a fresh bucket with no usable quantity stays
/// [`UsageQuotaStateV2::Unknown`] (never a fabricated `0%` bar or an
/// [`UsageQuotaStateV2::Available`] claim).
pub(in crate::host) fn quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV2 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV2::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV2::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV2::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV2::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if let Some(count) = &bucket.count_quota {
                return match count.remaining {
                    Some(0) => UsageQuotaStateV2::Exhausted,
                    Some(_) => match bucket.severity {
                        UsageSeverity::Normal => UsageQuotaStateV2::Available,
                        UsageSeverity::Warn | UsageSeverity::Danger => UsageQuotaStateV2::Warning,
                    },
                    None => UsageQuotaStateV2::Unknown,
                };
            }
            if bucket.used_money.is_some()
                || bucket.limit_money.is_some()
                || bucket.remaining_money.is_some()
            {
                return spend_quota_state(bucket);
            }
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV2::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV2::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV2::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV2::Available
                        } else {
                            UsageQuotaStateV2::Unknown
                        }
                    }
                }
            }
        }
    }
}

/// Whether a monetary bucket reports spend at or over its cap on a compatible
/// denomination. Incompatible or unusable money never reads as exhausted.
fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
    spend_quota_state(bucket) == UsageQuotaStateV2::Exhausted
}

/// Whether a bucket carries any usable quantity: a percent, a money amount,
/// or a provider quantity label. Buckets with none of these are unknown, not
/// available.
fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.used_label.is_some()
        || bucket.limit_label.is_some()
}

const fn status_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "Available",
        UsageSnapshotStatus::Stale => "Stale",
        UsageSnapshotStatus::NeedsLogin => "Needs login",
        UsageSnapshotStatus::NeedsSecret => "Needs secret",
        UsageSnapshotStatus::Unsupported => "Unsupported",
        UsageSnapshotStatus::Unavailable => "Unavailable",
        UsageSnapshotStatus::Error => "Error",
    }
}

#[cfg(test)]
mod tests;
