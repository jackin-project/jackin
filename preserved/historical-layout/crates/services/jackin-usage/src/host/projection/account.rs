// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account and window projection.

use jackin_core::account_key_hash;
use jackin_protocol::control::{QuotaBucketView, StatusSlot, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageGenerationView, UsageIdentityKindV1,
    UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageLimitWindowV1, UsagePercent, UsageWindowCategoryV1,
};

use super::super::accounts::{AccountCatalogEntry, CanonicalAccountSubject};

use super::{freshness, lifecycle, project_groups, quota_state, status_label, view_is_usable};

pub(crate) fn project_account(
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
        issues: view_issues(&entry.view),
    })
}

pub(crate) fn discovery_issue(
    issue: super::super::discovery::UsageDiscoveryIssue,
    scope: UsageIssueScopeV1,
) -> UsageIssueV1 {
    use super::super::discovery::UsageDiscoveryIssue;
    let recoverability = match issue {
        UsageDiscoveryIssue::ConfigVersionUnsupported => UsageIssueRecoverabilityV1::Unsupported,
        UsageDiscoveryIssue::ConfigTransientConflict => UsageIssueRecoverabilityV1::Retryable,
        _ => UsageIssueRecoverabilityV1::ActionRequired,
    };
    UsageIssueV1 {
        code: issue.id().to_owned(),
        scope,
        recoverability,
        message: issue.display_message().to_owned(),
        retry_at_epoch: None,
    }
}

/// Typed broker state supplies recovery policy; display text never controls it.
pub(crate) fn apply_generation_metadata(account: &mut UsageAccountV1, state: &UsageGenerationView) {
    account.freshness.generation = state.generation;
    account.freshness.retry_at_epoch = state.retry_at_epoch;
    account.freshness.last_good_at_epoch = state
        .snapshot
        .as_ref()
        .filter(|snapshot| view_is_usable(snapshot.status))
        .map(|snapshot| snapshot.fetched_at_epoch);
    account.freshness.is_stale = state.error.is_some()
        && account.freshness.last_good_at_epoch.is_some()
        || state
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.status == UsageSnapshotStatus::Stale);
    account.freshness.phase = if state.phase.is_active() {
        UsageFreshnessPhaseV1::Refreshing
    } else if account.freshness.is_stale {
        UsageFreshnessPhaseV1::Stale
    } else if state.error.is_some() || state.snapshot.is_none() {
        UsageFreshnessPhaseV1::Failed
    } else {
        account.freshness.phase
    };
    if let Some(error) = &state.error {
        if state.snapshot.is_none() {
            account.lifecycle = failure_lifecycle(error.kind);
        }
        account.issues = vec![UsageIssueV1 {
            code: super::super::broker::publish::issue_code(error.kind),
            scope: UsageIssueScopeV1::Account,
            recoverability: super::super::broker::publish::issue_recoverability(error.kind),
            message: error.message.clone(),
            retry_at_epoch: state.retry_at_epoch,
        }];
    }
}

pub(in crate::host) fn failure_lifecycle(
    kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind,
) -> UsageLifecycleV1 {
    use jackin_protocol::usage_broker::UsageCoordinationErrorKind;
    match kind {
        UsageCoordinationErrorKind::NeedsSecret => UsageLifecycleV1::NeedsSecret,
        UsageCoordinationErrorKind::Unauthorized => UsageLifecycleV1::NeedsLogin,
        UsageCoordinationErrorKind::ProtocolMismatch => UsageLifecycleV1::Unsupported,
        UsageCoordinationErrorKind::Unavailable
        | UsageCoordinationErrorKind::ProviderUnavailable => UsageLifecycleV1::Unavailable,
        _ => UsageLifecycleV1::Error,
    }
}

pub(crate) fn view_issues(view: &jackin_protocol::control::FocusedUsageView) -> Vec<UsageIssueV1> {
    let Some(message) = view
        .last_error
        .as_ref()
        .filter(|message| !message.trim().is_empty())
    else {
        return Vec::new();
    };
    let (code, recoverability) = match view.status {
        UsageSnapshotStatus::NeedsLogin => {
            ("needs_login", UsageIssueRecoverabilityV1::ActionRequired)
        }
        UsageSnapshotStatus::NeedsSecret => {
            ("needs_secret", UsageIssueRecoverabilityV1::ActionRequired)
        }
        UsageSnapshotStatus::Unsupported => {
            ("unsupported", UsageIssueRecoverabilityV1::Unsupported)
        }
        UsageSnapshotStatus::Fresh
        | UsageSnapshotStatus::Stale
        | UsageSnapshotStatus::Unavailable
        | UsageSnapshotStatus::Error => (
            "provider_unavailable",
            UsageIssueRecoverabilityV1::Retryable,
        ),
    };
    vec![UsageIssueV1 {
        code: code.to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability,
        message: message.clone(),
        retry_at_epoch: None,
    }]
}

pub(crate) fn project_window(
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

pub(crate) const fn window_category(status_slot: Option<StatusSlot>) -> UsageWindowCategoryV1 {
    match status_slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    }
}

/// Raw used percentage from a monetary bucket, unclamped so over-100% overage
/// survives. One shared [`Money::raw_percent_of`] rule with the capsule
/// bucket presentation, so both surfaces recover the same overage magnitude.
pub(crate) fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    bucket
        .used_money
        .as_ref()?
        .raw_percent_of(bucket.limit_money.as_ref()?)
}
