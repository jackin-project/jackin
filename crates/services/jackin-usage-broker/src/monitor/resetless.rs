// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricPeriodV1,
    UsageMetricValueV1, UsageWindowCategoryV1,
};
use jackin_protocol::usage_monitor::{MonitorEvidence, MonitorEvidenceValue};
use serde::{Deserialize, Serialize};

use super::{
    AccountObservations, MAX_FUTURE_SKEW_SECS, MONITOR_EVIDENCE_TTL_SECS, Observed,
    ObservedPercentage, ObservedQuotaPair, ObservedWindow, quota::monitor_basis_points,
    quota::monitor_source_used_points, quota::monitor_window_used_points,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairSource {
    BrokerProjection,
    Statusline,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ResetAdmissionFence {
    Unknown {
        after_epoch: i64,
    },
    #[default]
    KnownNone,
    Known {
        reset_at_epoch: i64,
        source_after_epoch: i64,
        receipt_after_epoch: i64,
        input_sequence: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResetlessCandidate {
    pub(super) value: i32,
    pub(super) source_at_epoch: i64,
}

pub(super) fn strict_resetless_candidates(
    account: &jackin_protocol::usage_broker::UsageAccountV1,
    received_at_epoch: i64,
    now_epoch: i64,
) -> [Option<ResetlessCandidate>; 2] {
    std::array::from_fn(|index| {
        strict_resetless_candidate(account, index, received_at_epoch, now_epoch)
    })
}

fn strict_resetless_candidate(
    account: &jackin_protocol::usage_broker::UsageAccountV1,
    index: usize,
    received_at_epoch: i64,
    now_epoch: i64,
) -> Option<ResetlessCandidate> {
    let category = if index == 0 {
        UsageWindowCategoryV1::Session
    } else {
        UsageWindowCategoryV1::LongRange
    };
    let principal_windows = account
        .windows
        .iter()
        .filter(|window| {
            window.category == category
                && window.reset_at_epoch.is_none()
                && monitor_window_used_points(window).is_some()
        })
        .collect::<Vec<_>>();
    if principal_windows.len() != 1 {
        return None;
    }
    let window = principal_windows[0];
    let raw_used = monitor_window_used_points(window)?;
    let value = monitor_basis_points(raw_used)?;
    let period_matches = |period: UsageMetricPeriodV1| match period {
        UsageMetricPeriodV1::Rolling { window_secs } => {
            (index == 0 && window_secs <= 86_400) || (index == 1 && window_secs > 86_400)
        }
        UsageMetricPeriodV1::Calendar { granularity } => {
            (index == 0 && granularity == UsageCalendarPeriodV1::Daily)
                || (index == 1
                    && matches!(
                        granularity,
                        UsageCalendarPeriodV1::Weekly | UsageCalendarPeriodV1::Monthly
                    ))
        }
        UsageMetricPeriodV1::ProviderDefined => index == 0,
        UsageMetricPeriodV1::Unknown => false,
    };
    let principal_groups = account
        .metric_groups
        .iter()
        .filter(|group| {
            if group.kind != UsageMetricGroupKindV1::Window
                || group.label != window.label
                || group.scope.service.is_some()
                || group.scope.model.is_some()
                || group.scope.pool.is_some()
                || group.scope.key_id.is_some()
            {
                return false;
            }
            let UsageMetricValueV1::Window { period, .. } = &group.value else {
                return false;
            };
            period_matches(*period)
        })
        .collect::<Vec<_>>();
    if principal_groups.len() != 1 {
        return None;
    }
    let group = principal_groups[0];
    if group.is_stale
        || group.phase != UsageFreshnessPhaseV1::Current
        || group.reset_at_epoch.is_some()
    {
        return None;
    }
    let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        money_used_percent,
        ..
    } = &group.value
    else {
        return None;
    };
    let group_used = monitor_source_used_points(
        *used_percent,
        *used_percent_origin,
        *remaining_percent,
        *remaining_percent_origin,
        *money_used_percent,
    )?;
    if group_used != raw_used {
        return None;
    }
    let source_at_epoch = group
        .observed_at_epoch
        .or(group.last_success_at_epoch)
        .unwrap_or(group.fetched_at_epoch);
    valid_source_receipt(source_at_epoch, received_at_epoch, now_epoch).then_some(
        ResetlessCandidate {
            value,
            source_at_epoch,
        },
    )
}

pub(super) fn used_only_is_admissible(
    account: &AccountObservations,
    index: usize,
    candidate: &ObservedPercentage,
    input_sequence: u64,
    now_epoch: i64,
) -> bool {
    let Some(window) = account.broker_windows.get(index) else {
        return false;
    };
    if !valid_candidate(candidate, input_sequence, now_epoch)
        || input_sequence <= account.input_sequence
        || !valid_broker_window(window, account.input_sequence, now_epoch)
    {
        return false;
    }
    let fence = account.reset_admission_fences[index].clone();
    if !fence_is_well_formed(
        &fence,
        account.latest_reset_epochs[index],
        account.input_sequence,
        now_epoch,
    ) {
        return false;
    }
    match fence {
        ResetAdmissionFence::Unknown { .. } => return false,
        ResetAdmissionFence::KnownNone => {
            if account.latest_reset_epochs[index].is_some() {
                return false;
            }
        }
        ResetAdmissionFence::Known {
            source_after_epoch,
            receipt_after_epoch,
            input_sequence: fence_sequence,
            ..
        } => {
            if candidate.input_sequence <= fence_sequence
                || candidate
                    .evidence_at_epoch
                    .is_none_or(|source| source <= source_after_epoch)
                || candidate.received_at_epoch <= receipt_after_epoch
            {
                return false;
            }
        }
    }
    for prior_sequence in broker_window_sequences(window) {
        if input_sequence <= prior_sequence {
            return false;
        }
    }
    for (source, received) in broker_window_times(window) {
        if candidate.received_at_epoch <= received
            || candidate
                .evidence_at_epoch
                .is_none_or(|next_source| next_source <= source)
        {
            return false;
        }
    }
    for session in account.sessions.values() {
        let Some(prior_window) = session.windows.get(index) else {
            return false;
        };
        if !valid_statusline_observed_window(prior_window, account.input_sequence, now_epoch) {
            return false;
        }
        for (source, received) in broker_window_times(prior_window) {
            if candidate.received_at_epoch <= received
                || candidate
                    .evidence_at_epoch
                    .is_none_or(|next_source| next_source <= source)
            {
                return false;
            }
        }
    }
    true
}

pub(super) fn advance_reset_admission_fence(
    account: &mut AccountObservations,
    index: usize,
    reset_at_epoch: i64,
    reset: &Observed<i64>,
    now_epoch: i64,
) {
    let Some(latest_reset) = account.latest_reset_epochs.get(index).copied().flatten() else {
        return;
    };
    if reset_at_epoch != latest_reset || reset_at_epoch < 0 {
        return;
    }
    let receipt = reset.received_at_epoch;
    let source = reset.evidence_at_epoch.unwrap_or(receipt);
    let previous = account.reset_admission_fences[index].clone();
    let prior_cutoff = match &previous {
        ResetAdmissionFence::Unknown { after_epoch } => *after_epoch,
        ResetAdmissionFence::KnownNone => 0,
        ResetAdmissionFence::Known {
            source_after_epoch,
            receipt_after_epoch,
            ..
        } => (*source_after_epoch).max(*receipt_after_epoch),
    };
    let times_are_fresh = valid_source_receipt(source, receipt, now_epoch)
        && reset.input_sequence > account.input_sequence;
    let fence = match previous {
        ResetAdmissionFence::Unknown { after_epoch } => {
            if times_are_fresh && source > after_epoch && receipt > after_epoch {
                ResetAdmissionFence::Known {
                    reset_at_epoch,
                    source_after_epoch: source.max(receipt),
                    receipt_after_epoch: receipt,
                    input_sequence: reset.input_sequence,
                }
            } else {
                ResetAdmissionFence::Unknown {
                    after_epoch: after_epoch.max(receipt).max(source),
                }
            }
        }
        ResetAdmissionFence::KnownNone => {
            if times_are_fresh {
                ResetAdmissionFence::Known {
                    reset_at_epoch,
                    source_after_epoch: source.max(receipt),
                    receipt_after_epoch: receipt,
                    input_sequence: reset.input_sequence,
                }
            } else {
                ResetAdmissionFence::Unknown {
                    after_epoch: now_epoch.max(prior_cutoff),
                }
            }
        }
        ResetAdmissionFence::Known {
            source_after_epoch,
            receipt_after_epoch,
            input_sequence,
            ..
        } => {
            if times_are_fresh {
                ResetAdmissionFence::Known {
                    reset_at_epoch,
                    source_after_epoch: source_after_epoch.max(source).max(receipt),
                    receipt_after_epoch: receipt_after_epoch.max(receipt),
                    input_sequence: input_sequence.max(reset.input_sequence),
                }
            } else {
                ResetAdmissionFence::Unknown {
                    after_epoch: now_epoch
                        .max(source_after_epoch)
                        .max(receipt_after_epoch)
                        .max(receipt)
                        .max(source),
                }
            }
        }
    };
    account.reset_admission_fences[index] = fence;
    if account.used_only_admission_sequences[index].is_some_and(|sequence| {
        !broker_orphan_is_admissible(account, index, now_epoch) || sequence >= reset.input_sequence
    }) {
        account.used_only_admission_sequences[index] = None;
    }
}

pub(super) fn invalidate_reset_admission_fence(
    account: &mut AccountObservations,
    index: usize,
    now_epoch: i64,
) {
    let old_cutoff = match &account.reset_admission_fences[index] {
        ResetAdmissionFence::Unknown { after_epoch } => *after_epoch,
        ResetAdmissionFence::KnownNone => 0,
        ResetAdmissionFence::Known {
            source_after_epoch,
            receipt_after_epoch,
            ..
        } => (*source_after_epoch).max(*receipt_after_epoch),
    };
    account.reset_admission_fences[index] = ResetAdmissionFence::Unknown {
        after_epoch: now_epoch.max(old_cutoff),
    };
    account.used_only_admission_sequences[index] = None;
}

pub(super) fn broker_orphan_is_admissible(
    account: &AccountObservations,
    index: usize,
    now_epoch: i64,
) -> bool {
    let Some(window) = account.broker_windows.get(index) else {
        return false;
    };
    let (Some(used), Some(fence)) = (
        window.used.as_ref(),
        account.reset_admission_fences.get(index),
    ) else {
        return false;
    };
    if used.reset_at_epoch.is_some()
        || used.evidence_at_epoch.is_none()
        || used.input_sequence == 0
        || account.used_only_admission_sequences[index] != Some(used.input_sequence)
        || !valid_observed_percentage(used, account.input_sequence, now_epoch)
        || window.paired.is_some()
        || !valid_reset_fence(
            fence,
            account.latest_reset_epochs[index],
            account.input_sequence,
            now_epoch,
        )
        || window.reset.as_ref().is_some_and(|reset| {
            reset.value < 0
                || !valid_optional_source_receipt(
                    reset.evidence_at_epoch,
                    reset.received_at_epoch,
                    reset.input_sequence,
                    account.input_sequence,
                    now_epoch,
                )
        })
    {
        return false;
    }
    let retained_reset_is_before_used = window.reset.as_ref().is_none_or(|reset| {
        reset.value <= account.latest_reset_epochs[index].unwrap_or(-1)
            && reset.input_sequence < used.input_sequence
            && reset.received_at_epoch < used.received_at_epoch
            && reset.evidence_at_epoch.unwrap_or(reset.received_at_epoch)
                < used.evidence_at_epoch.unwrap_or(used.received_at_epoch)
    });
    if !retained_reset_is_before_used {
        return false;
    }
    match fence {
        ResetAdmissionFence::KnownNone => {
            account.latest_reset_epochs[index].is_none() && window.reset.is_none()
        }
        ResetAdmissionFence::Known {
            reset_at_epoch,
            source_after_epoch,
            receipt_after_epoch,
            input_sequence,
        } => {
            account.latest_reset_epochs[index] == Some(*reset_at_epoch)
                && used.input_sequence > *input_sequence
                && used
                    .evidence_at_epoch
                    .is_some_and(|source| source > *source_after_epoch)
                && used.received_at_epoch > *receipt_after_epoch
        }
        ResetAdmissionFence::Unknown { .. } => false,
    }
}

pub(super) fn evidence_matches_used(
    evidence: &MonitorEvidence,
    account_id: Option<&str>,
    session_id: Option<&str>,
    used: &ObservedPercentage,
    window: super::MonitorQuotaWindow,
) -> bool {
    evidence.account_id.as_deref() == account_id
        && evidence.session_id.as_deref() == session_id
        && evidence.evidence_at_epoch == used.evidence_at_epoch
        && evidence.evidence_received_at_epoch == used.received_at_epoch
        && evidence.claude_code_version == used.claude_code_version
        && matches!(
            &evidence.value,
            MonitorEvidenceValue::QuotaUsedPercentage {
                window: found,
                used_percentage_basis_points,
            } if *found == window && *used_percentage_basis_points == used.value
        )
}

pub(super) fn evidence_matches_reset(
    evidence: &MonitorEvidence,
    account_id: Option<&str>,
    session_id: Option<&str>,
    reset: &Observed<i64>,
    window: super::MonitorQuotaWindow,
) -> bool {
    evidence.account_id.as_deref() == account_id
        && evidence.session_id.as_deref() == session_id
        && evidence.evidence_at_epoch == reset.evidence_at_epoch
        && evidence.evidence_received_at_epoch == reset.received_at_epoch
        && evidence.claude_code_version == reset.claude_code_version
        && matches!(
            &evidence.value,
            MonitorEvidenceValue::QuotaReset {
                window: found,
                reset_at_epoch,
            } if *found == window && *reset_at_epoch == reset.value
        )
}

pub(super) fn exact_observed_pair(window: &ObservedWindow) -> Option<&ObservedQuotaPair> {
    let (Some(used), Some(reset), Some(pair)) = (
        window.used.as_ref(),
        window.reset.as_ref(),
        window.paired.as_ref(),
    ) else {
        return None;
    };
    let pair_evidence_at = match (
        used.evidence_at_epoch,
        reset.evidence_at_epoch,
        pair.source_times,
    ) {
        (Some(used_source), Some(reset_source), [Some(pair_used), Some(pair_reset)])
            if pair_used == used_source && pair_reset == reset_source =>
        {
            Some(used_source.min(reset_source))
        }
        (None, None, [None, None]) => None,
        _ => return None,
    };
    (pair.input_sequence > 0
        && used.reset_at_epoch == Some(reset.value)
        && pair.used_percentage_basis_points == used.value
        && pair.reset_at_epoch == reset.value
        && pair.input_sequence == used.input_sequence
        && pair.input_sequence == reset.input_sequence
        && pair.received_at_epoch == used.received_at_epoch
        && pair.received_at_epoch == reset.received_at_epoch
        && pair.evidence_at_epoch == pair_evidence_at
        && pair.claude_code_version == used.claude_code_version
        && pair.claude_code_version == reset.claude_code_version)
        .then_some(pair)
}

pub(super) fn exact_observed_pair_for_source(
    window: &ObservedWindow,
    source: PairSource,
) -> Option<&ObservedQuotaPair> {
    let pair = exact_observed_pair(window)?;
    match source {
        PairSource::BrokerProjection if matches!(pair.source_times, [Some(_), Some(_)]) => {
            Some(pair)
        }
        PairSource::Statusline if pair.source_times == [None, None] => Some(pair),
        _ => None,
    }
}

pub(super) fn pair_from_observations(
    used: &ObservedPercentage,
    reset: &Observed<i64>,
    pair_source: PairSource,
) -> Option<ObservedQuotaPair> {
    if used.reset_at_epoch != Some(reset.value)
        || used.input_sequence == 0
        || used.input_sequence != reset.input_sequence
        || used.received_at_epoch != reset.received_at_epoch
        || used.claude_code_version != reset.claude_code_version
    {
        return None;
    }
    let (source_times, evidence_at_epoch) = match (used.evidence_at_epoch, reset.evidence_at_epoch)
    {
        (Some(used_source), Some(reset_source)) => (
            [Some(used_source), Some(reset_source)],
            Some(used_source.min(reset_source)),
        ),
        (None, None) => ([None, None], None),
        _ => return None,
    };
    if !match pair_source {
        PairSource::BrokerProjection => matches!(source_times, [Some(_), Some(_)]),
        PairSource::Statusline => source_times == [None, None],
    } {
        return None;
    }
    Some(ObservedQuotaPair {
        used_percentage_basis_points: used.value,
        reset_at_epoch: reset.value,
        evidence_at_epoch,
        source_times,
        received_at_epoch: used.received_at_epoch,
        input_sequence: used.input_sequence,
        claude_code_version: used.claude_code_version.clone(),
    })
}

pub(super) fn valid_observed_percentage(
    used: &ObservedPercentage,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    (0..=10_000).contains(&used.value)
        && valid_optional_source_receipt(
            used.evidence_at_epoch,
            used.received_at_epoch,
            used.input_sequence,
            max_sequence,
            now_epoch,
        )
        && used.reset_at_epoch.is_none_or(|reset| reset >= 0)
}

pub(super) fn valid_reset_fence(
    fence: &ResetAdmissionFence,
    latest_reset: Option<i64>,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    fence_is_well_formed(fence, latest_reset, max_sequence, now_epoch)
}

fn fence_is_well_formed(
    fence: &ResetAdmissionFence,
    latest_reset: Option<i64>,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    match fence {
        ResetAdmissionFence::Unknown { after_epoch } => {
            *after_epoch >= 0 && *after_epoch <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        }
        ResetAdmissionFence::KnownNone => latest_reset.is_none(),
        ResetAdmissionFence::Known {
            reset_at_epoch,
            source_after_epoch,
            receipt_after_epoch,
            input_sequence,
        } => {
            latest_reset == Some(*reset_at_epoch)
                && *reset_at_epoch >= 0
                && *source_after_epoch >= *receipt_after_epoch
                && *source_after_epoch <= receipt_after_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
                && *receipt_after_epoch >= 0
                && *receipt_after_epoch <= now_epoch
                && *source_after_epoch <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
                && *input_sequence > 0
                && *input_sequence <= max_sequence
        }
    }
}

fn valid_broker_window(window: &ObservedWindow, max_sequence: u64, now_epoch: i64) -> bool {
    valid_observed_window_for_source(
        window,
        max_sequence,
        now_epoch,
        PairSource::BrokerProjection,
    )
}

fn valid_observed_window_for_source(
    window: &ObservedWindow,
    max_sequence: u64,
    now_epoch: i64,
    pair_source: PairSource,
) -> bool {
    window
        .used
        .as_ref()
        .is_none_or(|used| valid_observed_percentage(used, max_sequence, now_epoch))
        && window.reset.as_ref().is_none_or(|reset| {
            reset.value >= 0
                && valid_optional_source_receipt(
                    reset.evidence_at_epoch,
                    reset.received_at_epoch,
                    reset.input_sequence,
                    max_sequence,
                    now_epoch,
                )
        })
        && window.paired.as_ref().is_none_or(|pair| {
            (0..=10_000).contains(&pair.used_percentage_basis_points)
                && pair.reset_at_epoch >= 0
                && valid_optional_source_receipt(
                    pair.evidence_at_epoch,
                    pair.received_at_epoch,
                    pair.input_sequence,
                    max_sequence,
                    now_epoch,
                )
                && exact_observed_pair_for_source(window, pair_source).is_some()
        })
}

pub(super) fn valid_broker_observed_window(
    window: &ObservedWindow,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    valid_broker_window(window, max_sequence, now_epoch)
}

pub(super) fn valid_statusline_observed_window(
    window: &ObservedWindow,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    valid_observed_window_for_source(window, max_sequence, now_epoch, PairSource::Statusline)
}

fn valid_candidate(candidate: &ObservedPercentage, sequence: u64, now_epoch: i64) -> bool {
    candidate.input_sequence == sequence
        && candidate.reset_at_epoch.is_none()
        && candidate.received_at_epoch == now_epoch
        && valid_observed_percentage(candidate, sequence, now_epoch)
}

pub(super) fn valid_optional_source_receipt(
    source_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    max_sequence: u64,
    now_epoch: i64,
) -> bool {
    received_at_epoch >= 0
        && received_at_epoch <= now_epoch
        && input_sequence <= max_sequence
        && source_at_epoch.is_none_or(|source| {
            source >= 0
                && source <= received_at_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
                && source <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        })
}

fn valid_source_receipt(source_at_epoch: i64, received_at_epoch: i64, now_epoch: i64) -> bool {
    if source_at_epoch < 0
        || received_at_epoch < 0
        || received_at_epoch > now_epoch
        || source_at_epoch > received_at_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        || source_at_epoch > now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
    {
        return false;
    }
    now_epoch.saturating_sub(source_at_epoch).max(0) <= MONITOR_EVIDENCE_TTL_SECS
        && now_epoch.saturating_sub(received_at_epoch).max(0) <= MONITOR_EVIDENCE_TTL_SECS
}

fn broker_window_sequences(window: &ObservedWindow) -> Vec<u64> {
    window
        .used
        .iter()
        .map(|used| used.input_sequence)
        .chain(window.reset.iter().map(|reset| reset.input_sequence))
        .chain(window.paired.iter().map(|pair| pair.input_sequence))
        .collect()
}

fn broker_window_times(window: &ObservedWindow) -> Vec<(i64, i64)> {
    window
        .used
        .iter()
        .map(|used| {
            (
                used.evidence_at_epoch.unwrap_or(used.received_at_epoch),
                used.received_at_epoch,
            )
        })
        .chain(window.reset.iter().map(|reset| {
            (
                reset.evidence_at_epoch.unwrap_or(reset.received_at_epoch),
                reset.received_at_epoch,
            )
        }))
        .chain(window.paired.iter().map(|pair| {
            (
                pair.evidence_at_epoch.unwrap_or(pair.received_at_epoch),
                pair.received_at_epoch,
            )
        }))
        .collect()
}
