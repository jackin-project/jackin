// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Surface-neutral canonical usage projection.

use std::collections::{BTreeMap, BTreeSet};

use icu_collator::{Collator, options::CollatorOptions, options::Strength};
use icu_locale::Locale;
use jackin_core::account_key_hash;
use jackin_protocol::control::{
    Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageIdentityKindV1, UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1,
    UsageMetricGroupKindV1, UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1,
    UsageMetricValueV1, UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1, UsageUnresolvedV1,
    UsageWindowCategoryV1,
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
    projection: &UsageProjectionV1,
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
                && provider.accounts.len() > 1
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
    /// Build the immutable surface-neutral V1 publication from current discovery.
    pub fn canonical_projection(&mut self, locale: &str) -> Result<UsageProjectionV1, String> {
        self.require_open()?;
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
        let generation = self
            .canonical_projection_cache
            .as_ref()
            .map_or(1, |projection| {
                projection.broker_generation.saturating_add(1)
            });
        let projection_id = format!("{}:{generation:020}", self.canonical_instance_id);
        let projection = build_canonical_projection(
            &catalog,
            discovery,
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
        if entries.is_empty() && unresolved == 0 {
            continue;
        }
        let accounts = entries
            .into_iter()
            .enumerate()
            .map(|(rank, entry)| project_account(entry, rank, metadata.broker_generation))
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
            issues: Vec::new(),
        });
    }

    let mut unresolved = discovery
        .unresolved_capabilities()
        .map(|candidate| UsageUnresolvedV1 {
            provider_id: HostSurfaceId::from_id(&candidate.surface_id).map_or_else(
                || candidate.surface_id.clone(),
                |surface| surface.provider_id().to_owned(),
            ),
            capability_id: candidate.capability_id.clone(),
            configuration_count: u32::try_from(candidate.provenance.len()).unwrap_or(u32::MAX),
            state: UsageLifecycleV1::NeedsLogin,
            issues: Vec::new(),
        })
        .collect::<Vec<_>>();
    unresolved.sort_by(|left, right| {
        provider_rank(&left.provider_id)
            .cmp(&provider_rank(&right.provider_id))
            .then(left.capability_id.cmp(&right.capability_id))
    });
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
        issues: Vec::new(),
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
) -> Result<UsageAccountV1, String> {
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
    Ok(UsageAccountV1 {
        canonical_account_id,
        identity_kind: match entry.identity.subject {
            CanonicalAccountSubject::ProviderId(_) => UsageIdentityKindV1::ProviderAccountId,
            CanonicalAccountSubject::ProviderStableHandle(_)
            | CanonicalAccountSubject::SourceCapability(_) => {
                UsageIdentityKindV1::ProviderStableHandle
            }
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
        issues: Vec::new(),
    })
}

fn project_window(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    rank: usize,
) -> Result<UsageLimitWindowV1, String> {
    let raw_used = money_used_raw_percent(bucket);
    let overage = raw_used.is_some_and(|value| value > 100);
    let (remaining_percent, remaining_raw_percent) = if overage {
        (None, None)
    } else if let Some(value) = bucket.remaining_percent {
        let (raw, clamped) = UsagePercent::split_raw(i32::from(value));
        (Some(clamped), Some(raw))
    } else {
        (None, None)
    };
    let (used_percent, used_raw_percent) = if overage || remaining_percent.is_none() {
        if let Some(raw) = raw_used {
            let (_, clamped) = UsagePercent::split_raw(raw);
            (Some(clamped), Some(raw))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    let value_label = if overage {
        raw_used.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |raw| format!("{raw}% used"),
        )
    } else {
        bucket.remaining_percent.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |value| format!("{value}% left"),
        )
    };
    Ok(UsageLimitWindowV1 {
        window_id: account_key_hash(canonical_account_id, &format!("canonical-window-v1:{rank}")),
        rank: u32::try_from(rank).map_err(|_| "window rank overflow")?,
        category: window_category(bucket.status_slot),
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        reset_at_epoch: bucket.resets_at,
        quota_state: quota_state(bucket),
        pace_label: bucket.pace_label.clone(),
        // No run-out signal exists outside the provider pace composite (which
        // already reaches both surfaces via `pace_label`); the field stays
        // unset rather than deriving a burn-rate estimate no producer stands
        // behind.
        runs_out_label: None,
    })
}

const fn window_category(status_slot: Option<StatusSlot>) -> UsageWindowCategoryV1 {
    match status_slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    }
}

/// Raw used percentage from a monetary bucket, unclamped so over-100% overage
/// survives. One shared [`Money::raw_percent_of`] rule with the capsule
/// bucket presentation, so both surfaces recover the same overage magnitude.
fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
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
) -> Result<Vec<UsageMetricGroupV1>, String> {
    project_groups(view, plan_label, canonical_account_id)
}

fn project_groups(
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
    canonical_account_id: &str,
) -> Result<Vec<UsageMetricGroupV1>, String> {
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
        if bucket.used_money.is_some() || bucket.limit_money.is_some() {
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
) -> Result<UsageMetricGroupV1, String> {
    let window = project_window(canonical_account_id, bucket, rank)?;
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::Window,
        label: bucket.label.clone(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        quota_state: window.quota_state,
        value: UsageMetricValueV1::Window {
            remaining_percent: window.remaining_percent,
            remaining_raw_percent: window.remaining_raw_percent,
            used_percent: window.used_percent,
            used_raw_percent: window.used_raw_percent,
            period: group_period(bucket.status_slot),
            unit: None,
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
) -> Result<UsageMetricGroupV1, String> {
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    let quota_state = spend_quota_state(bucket);
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: format!("{} spend", bucket.label),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        quota_state,
        value: UsageMetricValueV1::SpendCap {
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
) -> Result<UsageMetricGroupV1, String> {
    let phase = group_phase(view_status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(view_status));
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::Plan,
        label: "Plan".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        // Plan metadata carries no quota notion.
        quota_state: UsageQuotaStateV1::NotApplicable,
        value: UsageMetricValueV1::Plan {
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
) -> UsageFreshnessPhaseV1 {
    if view_status == UsageSnapshotStatus::Stale {
        return UsageFreshnessPhaseV1::Stale;
    }
    match bucket_status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV1::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV1::Stale,
        _ => UsageFreshnessPhaseV1::Failed,
    }
}

fn group_period(status_slot: Option<StatusSlot>) -> UsageMetricPeriodV1 {
    match status_slot {
        Some(StatusSlot::Session) => UsageMetricPeriodV1::ProviderDefined,
        Some(StatusSlot::Daily) => UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        },
        Some(StatusSlot::Weekly) => UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        },
        Some(StatusSlot::Spend) | None => UsageMetricPeriodV1::Unknown,
    }
}

/// Remaining spend from a monetary bucket. Checked subtraction on compatible
/// denominations only; over-spend clamps at zero here because a Money amount
/// cannot express negative remaining, while the over-100% raw percent on the
/// sibling window payload preserves the overage magnitude.
fn spend_remaining(bucket: &QuotaBucketView) -> Option<Money> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    if used.currency != limit.currency || used.exponent != limit.exponent {
        return None;
    }
    let remaining = limit.amount_minor.saturating_sub(used.amount_minor).max(0);
    Some(Money::new(
        remaining,
        limit.currency.clone(),
        limit.exponent,
    ))
}

/// Quota state for a spend-cap group from its money ratio. A missing or
/// unusable cap is [`UsageQuotaStateV1::Unknown`], never fabricated credit;
/// an uncapped tracker is [`UsageQuotaStateV1::NotApplicable`].
fn spend_quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
                (Some(used), Some(limit)) => spend_ratio_state(used, limit),
                // Spend without a cap is uncapped tracking; a cap without
                // spend leaves the ratio unknown.
                (Some(_), None) => UsageQuotaStateV1::NotApplicable,
                _ => UsageQuotaStateV1::Unknown,
            }
        }
    }
}

/// Quota state from a spend/cap money ratio with checked math.
fn spend_ratio_state(used: &Money, limit: &Money) -> UsageQuotaStateV1 {
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return UsageQuotaStateV1::Unknown;
    }
    if used.amount_minor >= limit.amount_minor {
        UsageQuotaStateV1::Exhausted
    } else if used.amount_minor.saturating_mul(100) / limit.amount_minor >= 80 {
        UsageQuotaStateV1::Warning
    } else {
        UsageQuotaStateV1::Available
    }
}

fn lifecycle(status: UsageSnapshotStatus, confidence: UsageConfidence) -> UsageLifecycleV1 {
    if confidence == UsageConfidence::PresenceOnly {
        return UsageLifecycleV1::AgentUninitialized;
    }
    match status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => UsageLifecycleV1::Available,
        UsageSnapshotStatus::NeedsLogin => UsageLifecycleV1::NeedsLogin,
        UsageSnapshotStatus::NeedsSecret => UsageLifecycleV1::NeedsSecret,
        UsageSnapshotStatus::Unsupported => UsageLifecycleV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageLifecycleV1::Unavailable,
        UsageSnapshotStatus::Error => UsageLifecycleV1::Error,
    }
}

fn freshness(status: UsageSnapshotStatus, last_good: i64, generation: u64) -> UsageFreshnessV1 {
    let phase = match status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV1::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV1::Stale,
        _ => UsageFreshnessPhaseV1::Failed,
    };
    UsageFreshnessV1 {
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

fn provider_freshness(accounts: &[UsageAccountV1], generation: u64) -> UsageFreshnessV1 {
    let phase = if accounts.iter().any(|account| account.freshness.is_stale) {
        UsageFreshnessPhaseV1::Stale
    } else if accounts.is_empty()
        || accounts
            .iter()
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Failed)
    {
        UsageFreshnessPhaseV1::Failed
    } else {
        UsageFreshnessPhaseV1::Current
    };
    UsageFreshnessV1 {
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
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
    }
}

/// Quota state for one bucket with no collapsing: missing permission stays
/// [`UsageQuotaStateV1::NoPermission`] (never [`UsageQuotaStateV1::Unsupported`]),
/// and a fresh bucket with no usable quantity stays
/// [`UsageQuotaStateV1::Unknown`] (never a fabricated `0%` bar or an
/// [`UsageQuotaStateV1::Available`] claim).
fn quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV1::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV1::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV1::Available
                        } else {
                            UsageQuotaStateV1::Unknown
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
    match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
        (Some(used), Some(limit)) => {
            used.currency == limit.currency
                && used.exponent == limit.exponent
                && limit.amount_minor > 0
                && used.amount_minor >= limit.amount_minor
        }
        _ => false,
    }
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
