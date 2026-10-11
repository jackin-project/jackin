// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::evidence::latch_account_reset_barrier;
use super::validation::valid_identifier;
use super::{
    AccountObservations, AccountResetBarrier, MAX_ACCOUNTS, MAX_FUTURE_SKEW_SECS,
    MONITOR_EVIDENCE_TTL_SECS, MonitorEvidenceSource, MonitorIssue, MonitorIssueCode,
    MonitorProviderReadiness, Observed, ObservedPercentage, ObservedQuotaPair, ObservedWindow,
    ProjectionBindingIdentity, ProviderObservation, StoreState, UsageAccountV1,
    UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricPeriodV1, UsageMetricValueV1,
    UsageProjectionV1, UsageWindowCategoryV1, clear_account_broker_projection, issue,
};

pub(super) fn observe_projection_account(
    state: &mut StoreState,
    account: &UsageAccountV1,
    projection: &UsageProjectionV1,
    local_account_id: &str,
    binding: Option<&ProjectionBindingIdentity>,
    now_epoch: i64,
) -> Result<bool, MonitorIssue> {
    if !valid_identifier(&account.canonical_account_id) || !valid_identifier(local_account_id) {
        return Ok(false);
    }

    // A whole-projection publication can advance because another account
    // refreshed. Stale per-account evidence can still be retained with its
    // source timestamps; it never receives a new evidence time here.
    let proposed_sequence = state.next_input_sequence.saturating_add(1);
    let windows = projection_windows(account, now_epoch, proposed_sequence);
    if !state.accounts.contains_key(local_account_id) && state.accounts.len() >= MAX_ACCOUNTS {
        return Err(issue(
            MonitorIssueCode::MonitorStoreUnavailable,
            "monitor store reached its configured account limit",
            None,
        ));
    }

    let account_state = state
        .accounts
        .entry(local_account_id.to_owned())
        .or_default();
    let mut next_provider_observation = provider_observation(account, projection, binding);
    let identity_changed = account_state
        .provider_observation
        .as_ref()
        .is_some_and(|previous| !same_source_binding(previous, &next_provider_observation));
    if identity_changed {
        // A local label can be rebound to another canonical source. Retained
        // quota evidence and reset barriers belong to the old mapping. The
        // shared reset watermark is recomputed from retained statusline data.
        clear_account_broker_projection(account_state);
    }
    if let Some(previous) = account_state.provider_observation.as_ref() {
        if !identity_changed && projection_is_older(previous, &next_provider_observation) {
            return Ok(false);
        }
        if !identity_changed && same_source_binding(previous, &next_provider_observation) {
            // A provider generation reset changes ordering metadata only.
            // Last-good evidence and quota-reset barriers remain authoritative.
            next_provider_observation.last_good_at_epoch = match (
                next_provider_observation.last_good_at_epoch,
                previous.last_good_at_epoch,
            ) {
                (Some(next), Some(previous)) => Some(next.max(previous)),
                (None, previous) => previous,
                (next, None) => next,
            };
        }
    }
    let mut provider_changed = account_state
        .provider_observation
        .as_ref()
        .is_none_or(|previous| !same_provider_observation(previous, &next_provider_observation));
    if !provider_changed && let Some(previous) = account_state.provider_observation.as_ref() {
        next_provider_observation.broker_generation = previous.broker_generation;
    }
    if provider_changed {
        account_state.provider_observation = Some(next_provider_observation);
    }
    let windows_changed = update_broker_windows(account_state, windows, proposed_sequence);
    if windows_changed {
        if !provider_changed && let Some(previous) = account_state.provider_observation.as_ref() {
            let mut fenced = previous.clone();
            fenced.broker_generation = projection.broker_generation;
            account_state.provider_observation = Some(fenced);
            provider_changed = true;
        }
        account_state.input_sequence = proposed_sequence;
        state.next_input_sequence = proposed_sequence;
        latch_broker_reset_barriers(account_state, proposed_sequence, now_epoch);
    }
    Ok(provider_changed || windows_changed)
}

fn provider_observation(
    account: &UsageAccountV1,
    projection: &UsageProjectionV1,
    binding: Option<&ProjectionBindingIdentity>,
) -> ProviderObservation {
    let (issue_code, issue_retry_at) = [
        "rate_limited",
        "unauthorized",
        "provider_timeout",
        "provider_unavailable",
        "needs_secret",
    ]
    .into_iter()
    .find_map(|code| {
        account
            .issues
            .iter()
            .find(|item| item.code == code)
            .map(|item| {
                let code = match item.code.as_str() {
                    "rate_limited" => MonitorIssueCode::ProviderRateLimited,
                    "unauthorized" => MonitorIssueCode::ProviderUnauthorized,
                    "provider_timeout" => MonitorIssueCode::ProviderTimeout,
                    "provider_unavailable" => MonitorIssueCode::ProviderUnavailable,
                    "needs_secret" => MonitorIssueCode::ProviderNeedsSecret,
                    _ => unreachable!("the selected provider issue code is supported"),
                };
                (code, item.retry_at_epoch)
            })
    })
    .map_or((None, None), |(code, retry_at)| (Some(code), retry_at));
    let readiness = if issue_code == Some(MonitorIssueCode::ProviderRateLimited) {
        MonitorProviderReadiness::RateLimited
    } else {
        match account.freshness.phase {
            UsageFreshnessPhaseV1::Current if !account.freshness.is_stale => {
                MonitorProviderReadiness::Current
            }
            UsageFreshnessPhaseV1::Refreshing => MonitorProviderReadiness::Refreshing,
            UsageFreshnessPhaseV1::Stale => MonitorProviderReadiness::Stale,
            UsageFreshnessPhaseV1::Current => MonitorProviderReadiness::Stale,
            UsageFreshnessPhaseV1::Failed if account.freshness.last_good_at_epoch.is_some() => {
                MonitorProviderReadiness::Stale
            }
            UsageFreshnessPhaseV1::Failed => MonitorProviderReadiness::Unknown,
        }
    };
    ProviderObservation {
        generation: account.freshness.generation,
        broker_instance_id: projection.broker_instance_id.clone(),
        broker_generation: projection.broker_generation,
        source_account_id: account.canonical_account_id.clone(),
        binding_id: binding.map(|binding| binding.binding_id.clone()),
        binding_revision: binding.map(|binding| binding.binding_revision),
        readiness,
        last_good_at_epoch: account.freshness.last_good_at_epoch,
        retry_at_epoch: issue_retry_at.or(account.freshness.retry_at_epoch),
        issue_code,
    }
}

fn same_source_binding(previous: &ProviderObservation, next: &ProviderObservation) -> bool {
    previous.source_account_id == next.source_account_id
        && previous.binding_id == next.binding_id
        && previous.binding_revision == next.binding_revision
}

fn projection_is_older(previous: &ProviderObservation, next: &ProviderObservation) -> bool {
    if !same_source_binding(previous, next)
        || previous.broker_instance_id != next.broker_instance_id
    {
        return false;
    }
    if next.generation > previous.generation {
        return false;
    }
    if next.generation < previous.generation {
        // A newer broker publication may represent a provider catalog reset
        // that legitimately restarted this account's generation counter.
        return next.broker_generation <= previous.broker_generation;
    }
    next.broker_generation < previous.broker_generation
}

fn same_provider_observation(previous: &ProviderObservation, next: &ProviderObservation) -> bool {
    previous.generation == next.generation
        && previous.broker_instance_id == next.broker_instance_id
        && previous.source_account_id == next.source_account_id
        && previous.binding_id == next.binding_id
        && previous.binding_revision == next.binding_revision
        && previous.readiness == next.readiness
        && previous.last_good_at_epoch == next.last_good_at_epoch
        && previous.retry_at_epoch == next.retry_at_epoch
        && previous.issue_code == next.issue_code
}

pub(super) fn latch_broker_reset_barriers(
    account: &mut AccountObservations,
    input_sequence: u64,
    now_epoch: i64,
) {
    for index in 0..account.broker_windows.len() {
        let barrier = account.broker_windows[index]
            .used
            .as_ref()
            .filter(|used| used.input_sequence == input_sequence && used.value >= 9_500)
            .map(|used| AccountResetBarrier {
                started_at_epoch: now_epoch,
                prior_reset_at_epoch: used.reset_at_epoch,
                prior_used_percentage_basis_points: used.value,
                input_sequence_before: input_sequence,
                source: MonitorEvidenceSource::BrokerProjection,
                session_id: None,
                pause_reason: if used.value >= 10_000 {
                    MonitorIssueCode::LimitExhausted
                } else {
                    MonitorIssueCode::LimitGuardReached
                },
            });
        if let Some(barrier) = barrier {
            latch_account_reset_barrier(account, index, barrier);
        }
    }
}

pub(super) fn projection_windows(
    account: &UsageAccountV1,
    received_at_epoch: i64,
    input_sequence: u64,
) -> [Option<ObservedWindow>; 2] {
    let mut windows = [None, None];
    let fallback_evidence_at = account
        .freshness
        .last_good_at_epoch
        .filter(|time| *time <= received_at_epoch);
    for (index, category) in [
        UsageWindowCategoryV1::Session,
        UsageWindowCategoryV1::LongRange,
    ]
    .into_iter()
    .enumerate()
    {
        let max_reset_ahead_secs = if category == UsageWindowCategoryV1::Session {
            5 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        } else {
            7 * 24 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        };
        let candidates = account
            .windows
            .iter()
            .filter(|window| window.category == category)
            .filter_map(|window| {
                if window.reset_at_epoch.is_some_and(|reset| {
                    reset < 0 || reset > received_at_epoch.saturating_add(max_reset_ahead_secs)
                }) {
                    return None;
                }
                let raw_used = window.used_raw_percent.or_else(|| {
                    window
                        .remaining_raw_percent
                        .map(|remaining| 100i32.saturating_sub(remaining))
                });
                let raw_used = raw_used.filter(|used| *used >= 0);
                let used = raw_used.map(|value| value.min(100));
                let used_at = raw_used.and_then(|value| {
                    metric_group_time(
                        account,
                        index,
                        Some(value),
                        window.reset_at_epoch,
                        received_at_epoch,
                    )
                    .unwrap_or(fallback_evidence_at)
                });
                let reset_at = window.reset_at_epoch.and_then(|reset| {
                    metric_group_time(account, index, raw_used, Some(reset), received_at_epoch)
                        .unwrap_or(fallback_evidence_at)
                });
                (used.is_some() || window.reset_at_epoch.is_some())
                    .then_some((window, used, used_at, reset_at))
            })
            .collect::<Vec<_>>();
        let used = candidates
            .iter()
            .filter_map(|(window, used, evidence_at, _)| {
                used.map(|used| (*window, used, *evidence_at))
            })
            .max_by_key(|(_, used, evidence_at)| (*used, *evidence_at));
        let reset = candidates
            .iter()
            .filter_map(|(window, _, _, evidence_at)| {
                window.reset_at_epoch.map(|reset| (reset, *evidence_at))
            })
            .min_by_key(|(reset, _)| *reset);
        let (used_value, used_reset, used_evidence_at) = used
            .map_or((None, None, None), |(window, used, evidence_at)| {
                (Some(used), window.reset_at_epoch, evidence_at)
            });
        let (reset_value, reset_evidence_at) = reset
            .map_or((None, None), |(reset, evidence_at)| {
                (Some(reset), evidence_at)
            });
        if used_value.is_none() && reset_value.is_none() {
            continue;
        }
        let paired = match (
            used_value,
            used_reset,
            used_evidence_at,
            reset_value,
            reset_evidence_at,
        ) {
            (Some(used), Some(used_reset), Some(used_at), Some(reset), Some(reset_at))
                if used_reset == reset =>
            {
                Some(ObservedQuotaPair {
                    used_percentage_basis_points: used.saturating_mul(100),
                    reset_at_epoch: reset,
                    evidence_at_epoch: Some(used_at.min(reset_at)),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                })
            }
            _ => None,
        };
        windows[index] = Some(ObservedWindow {
            used: used_value
                .zip(used_evidence_at)
                .map(|(value, evidence_at_epoch)| ObservedPercentage {
                    value: value.saturating_mul(100),
                    reset_at_epoch: used_reset,
                    evidence_at_epoch: Some(evidence_at_epoch),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                }),
            reset: reset_value
                .zip(reset_evidence_at)
                .map(|(value, evidence_at_epoch)| Observed {
                    value,
                    evidence_at_epoch: Some(evidence_at_epoch),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                }),
            paired,
        });
    }
    windows
}

/// `Some(None)` means a typed group identifies this field but is stale.
pub(super) fn metric_group_time(
    account: &UsageAccountV1,
    window_index: usize,
    used_raw_percent: Option<i32>,
    reset_at_epoch: Option<i64>,
    now_epoch: i64,
) -> Option<Option<i64>> {
    let period_matches = |period: UsageMetricPeriodV1| match period {
        UsageMetricPeriodV1::Rolling { window_secs } => {
            (window_index == 0 && window_secs <= 86_400)
                || (window_index == 1 && window_secs > 86_400)
        }
        UsageMetricPeriodV1::Calendar { granularity } => {
            (window_index == 0
                && granularity == jackin_protocol::usage_broker::UsageCalendarPeriodV1::Daily)
                || (window_index == 1
                    && matches!(
                        granularity,
                        jackin_protocol::usage_broker::UsageCalendarPeriodV1::Weekly
                            | jackin_protocol::usage_broker::UsageCalendarPeriodV1::Monthly
                    ))
        }
        // Claude's session window has no published duration. Its typed group
        // still identifies independent per-window freshness and is authoritative.
        UsageMetricPeriodV1::ProviderDefined => window_index == 0,
        UsageMetricPeriodV1::Unknown => false,
    };
    let window_groups = account
        .metric_groups
        .iter()
        .filter(|group| group.kind == UsageMetricGroupKindV1::Window)
        .filter(|group| match &group.value {
            UsageMetricValueV1::Window { period, .. } => period_matches(*period),
            _ => false,
        })
        .collect::<Vec<_>>();
    if window_groups.is_empty() {
        return None;
    }
    let groups = window_groups
        .into_iter()
        .filter(|group| {
            let UsageMetricValueV1::Window {
                used_raw_percent: group_used,
                remaining_raw_percent: group_remaining,
                ..
            } = &group.value
            else {
                return false;
            };
            let group_used = (*group_used)
                .or_else(|| group_remaining.map(|remaining| 100i32.saturating_sub(remaining)));
            (used_raw_percent.is_none() || group_used == used_raw_percent)
                && (reset_at_epoch.is_none() || group.reset_at_epoch == reset_at_epoch)
        })
        .collect::<Vec<_>>();
    if groups.is_empty() {
        // Typed provider evidence for this quota category is authoritative.
        // A different metric group cannot lend it a fresh timestamp.
        return Some(None);
    }
    let current = groups
        .iter()
        .filter(|group| group_evidence_time(account, group).is_some())
        .filter(|group| {
            group_evidence_time(account, group)
                .is_some_and(|time| time <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS))
        })
        .max_by_key(|group| group_evidence_time(account, group));
    current.map_or(Some(None), |group| {
        group_evidence_time(account, group).map(Some)
    })
}

fn group_evidence_time(
    account: &UsageAccountV1,
    group: &jackin_protocol::usage_broker::UsageMetricGroupV1,
) -> Option<i64> {
    if group.is_stale || group.phase != UsageFreshnessPhaseV1::Current {
        group
            .last_success_at_epoch
            .or(account.freshness.last_good_at_epoch)
    } else {
        Some(
            group
                .observed_at_epoch
                .or(group.last_success_at_epoch)
                .unwrap_or(group.fetched_at_epoch),
        )
    }
}

pub(super) fn update_broker_windows(
    account: &mut AccountObservations,
    input: [Option<ObservedWindow>; 2],
    input_sequence: u64,
) -> bool {
    let mut changed = false;
    for (index, maybe_window) in input.into_iter().enumerate() {
        let Some(window) = maybe_window else {
            continue;
        };
        if window.reset.as_ref().is_some_and(|reset| {
            account.latest_reset_epochs[index].is_some_and(|latest| reset.value < latest)
        }) {
            continue;
        }
        if let Some(reset) = window.reset.as_ref().filter(|reset| {
            account.latest_reset_epochs[index].is_none_or(|latest| reset.value > latest)
        }) {
            account.latest_reset_epochs[index] = Some(reset.value);
            changed = true;
        }
        let current = &mut account.broker_windows[index];
        changed |= update_broker_window(current, &window, input_sequence);
    }
    changed
}

pub(super) fn update_broker_window(
    current: &mut ObservedWindow,
    next: &ObservedWindow,
    input_sequence: u64,
) -> bool {
    let mut changed = false;
    if let Some(next_reset) = &next.reset {
        let mut next_reset = next_reset.clone();
        next_reset.input_sequence = input_sequence;
        match current.reset.as_ref() {
            None => {
                current.reset = Some(next_reset);
                changed = true;
            }
            Some(previous) if next_reset.value > previous.value => {
                current.reset = Some(next_reset);
                changed = true;
            }
            Some(previous)
                if next_reset.value == previous.value
                    && source_observation_is_newer(&next_reset, previous) =>
            {
                current.reset = Some(next_reset);
                changed = true;
            }
            _ => {}
        }
    }
    if let Some(next_used) = &next.used {
        let mut next_used = next_used.clone();
        next_used.input_sequence = input_sequence;
        let next_reset = next_used.reset_at_epoch;
        let current_used = current.used.as_ref();
        let reset_is_current = current
            .reset
            .as_ref()
            .is_none_or(|reset| next_reset.is_some_and(|candidate| candidate >= reset.value));
        if reset_is_current {
            match current_used {
                None => {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous) if next_reset > previous.reset_at_epoch => {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous)
                    if next_reset == previous.reset_at_epoch
                        && next_used.value > previous.value =>
                {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous)
                    if next_reset == previous.reset_at_epoch
                        && next_used.value == previous.value
                        && next_used.evidence_at_epoch > previous.evidence_at_epoch =>
                {
                    current.used = Some(next_used);
                    changed = true;
                }
                _ => {}
            }
        }
    }
    if let Some(next_pair) = &next.paired {
        let pair_is_newer = current.paired.as_ref().is_none_or(|previous| {
            next_pair.used_percentage_basis_points != previous.used_percentage_basis_points
                || next_pair.reset_at_epoch != previous.reset_at_epoch
                || next_pair.evidence_at_epoch > previous.evidence_at_epoch
        });
        if pair_is_newer {
            let mut next_pair = next_pair.clone();
            next_pair.input_sequence = input_sequence;
            current.paired = Some(next_pair);
            changed = true;
        }
    }
    changed
}

pub(super) fn source_observation_is_newer<T>(next: &Observed<T>, current: &Observed<T>) -> bool {
    next.evidence_at_epoch > current.evidence_at_epoch
}
