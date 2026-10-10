// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::status::push_issue;
use super::validation::scope_session_id;
use super::*;

pub(super) fn quota_window_status(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    window: MonitorQuotaWindow,
    index: usize,
    now_epoch: i64,
) -> MonitorQuotaWindowStatus {
    let mut used_candidates = Vec::<(&MonitorEvidence, i32)>::new();
    let mut reset_candidates = Vec::<(&MonitorEvidence, i64)>::new();
    for evidence in &monitor.evidence {
        if !evidence_is_relevant(evidence, monitor) {
            continue;
        }
        match evidence.value {
            MonitorEvidenceValue::QuotaUsedPercentage {
                window: evidence_window,
                used_percentage_basis_points,
            } if evidence_window == window => {
                if evidence_is_effective_used(evidence, account, index) {
                    used_candidates.push((evidence, used_percentage_basis_points));
                }
            }
            MonitorEvidenceValue::QuotaReset {
                window: evidence_window,
                reset_at_epoch,
            } if evidence_window == window
                && evidence_is_effective_reset(evidence, account, index) =>
            {
                reset_candidates.push((evidence, reset_at_epoch));
            }
            _ => {}
        }
    }
    let used = choose_used_candidate(used_candidates, now_epoch);
    let reset = choose_reset_candidate(reset_candidates, now_epoch);
    let reset_at_epoch = reset.map(|(_, value)| value);
    MonitorQuotaWindowStatus {
        used_percentage_basis_points: used.map(|(_, value)| value),
        used_evidence: used.map(|(evidence, _)| field_evidence(evidence, now_epoch)),
        reset_at_epoch,
        reset_validity: match reset_at_epoch {
            None => MonitorResetValidity::Unknown,
            Some(reset) if reset <= now_epoch => MonitorResetValidity::Due,
            Some(_) => MonitorResetValidity::Future,
        },
        reset_evidence: reset.map(|(evidence, _)| field_evidence(evidence, now_epoch)),
    }
}

pub(super) fn observed_reset_for_used(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    status: &MonitorQuotaWindowStatus,
    index: usize,
) -> Option<i64> {
    let sequence = status.used_evidence.as_ref()?.evidence_sequence;
    let evidence = monitor
        .evidence
        .iter()
        .find(|evidence| evidence.sequence == sequence)?;
    match (evidence.source, account) {
        (MonitorEvidenceSource::Statusline, Some(account)) => evidence
            .session_id
            .as_deref()
            .and_then(|session_id| account.sessions.get(session_id))
            .and_then(|session| session.windows[index].used.as_ref())
            .and_then(|used| used.reset_at_epoch),
        (MonitorEvidenceSource::BrokerProjection, Some(account)) => account.broker_windows[index]
            .used
            .as_ref()
            .and_then(|used| used.reset_at_epoch),
        _ => None,
    }
}

pub(super) fn choose_used_candidate(
    candidates: Vec<(&MonitorEvidence, i32)>,
    now_epoch: i64,
) -> Option<(&MonitorEvidence, i32)> {
    let current = candidates
        .iter()
        .copied()
        .filter(|(evidence, _)| is_current(evidence, now_epoch))
        .max_by_key(|(evidence, value)| (*value, evidence.evidence_received_at_epoch));
    current.or_else(|| {
        candidates
            .into_iter()
            .max_by_key(|(evidence, value)| (*value, evidence.evidence_received_at_epoch))
    })
}

pub(super) fn choose_reset_candidate(
    candidates: Vec<(&MonitorEvidence, i64)>,
    now_epoch: i64,
) -> Option<(&MonitorEvidence, i64)> {
    let current = candidates
        .iter()
        .copied()
        .filter(|(evidence, _)| is_current(evidence, now_epoch))
        .min_by_key(|(evidence, value)| {
            (
                *value,
                std::cmp::Reverse(evidence.evidence_received_at_epoch),
            )
        });
    current.or_else(|| {
        candidates.into_iter().min_by_key(|(evidence, value)| {
            (
                *value,
                std::cmp::Reverse(evidence.evidence_received_at_epoch),
            )
        })
    })
}

pub(super) fn evidence_is_effective_used(
    evidence: &MonitorEvidence,
    account: Option<&AccountObservations>,
    index: usize,
) -> bool {
    let Some(account) = account else {
        return true;
    };
    let Some(latest_reset) = account.latest_reset_epochs[index] else {
        return true;
    };
    if evidence.source == MonitorEvidenceSource::BrokerProjection {
        return account.broker_windows[index]
            .used
            .as_ref()
            .is_some_and(|used| {
                used.reset_at_epoch
                    .is_some_and(|reset| reset >= latest_reset)
            });
    }
    let Some(session_id) = evidence.session_id.as_deref() else {
        return true;
    };
    account
        .sessions
        .get(session_id)
        .and_then(|session| session.windows[index].used.as_ref())
        .is_some_and(|used| {
            used.reset_at_epoch
                .is_some_and(|reset| reset >= latest_reset)
        })
}

pub(super) fn evidence_is_effective_reset(
    evidence: &MonitorEvidence,
    account: Option<&AccountObservations>,
    index: usize,
) -> bool {
    let Some(account) = account else {
        return true;
    };
    let Some(latest_reset) = account.latest_reset_epochs[index] else {
        return true;
    };
    if evidence.source == MonitorEvidenceSource::BrokerProjection {
        return account.broker_windows[index]
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value >= latest_reset);
    }
    let Some(session_id) = evidence.session_id.as_deref() else {
        return true;
    };
    account
        .sessions
        .get(session_id)
        .and_then(|session| session.windows[index].reset.as_ref())
        .is_some_and(|reset| reset.value >= latest_reset)
}

pub(super) fn evidence_is_relevant(evidence: &MonitorEvidence, monitor: &DurableMonitor) -> bool {
    match &monitor.config.scope {
        MonitorScope::Session { session_id } => {
            evidence.account_id.is_none() && evidence.session_id.as_ref() == Some(session_id)
        }
        MonitorScope::BoundAccount { session_id, .. } => {
            evidence.account_id == monitor.account_id
                && (session_id
                    .as_ref()
                    .is_none_or(|session| evidence.session_id.as_ref() == Some(session))
                    || (evidence.session_id.is_none()
                        && matches!(&evidence.value, MonitorEvidenceValue::Spend { .. })))
        }
    }
}

pub(super) fn field_evidence(evidence: &MonitorEvidence, now_epoch: i64) -> MonitorFieldEvidence {
    let age = field_age(
        evidence.evidence_at_epoch,
        evidence.evidence_received_at_epoch,
        now_epoch,
    );
    MonitorFieldEvidence {
        evidence_sequence: evidence.sequence,
        evidence_at_epoch: evidence.evidence_at_epoch,
        evidence_received_at_epoch: evidence.evidence_received_at_epoch,
        age_seconds: age,
        freshness: if age <= MONITOR_EVIDENCE_TTL_SECS as u64
            && evidence.evidence_received_at_epoch <= now_epoch
            && evidence
                .evidence_at_epoch
                .is_none_or(|time| time <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS))
        {
            MonitorEvidenceFreshness::Current
        } else {
            MonitorEvidenceFreshness::Stale
        },
    }
}

pub(super) fn field_age(
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    now_epoch: i64,
) -> u64 {
    let nonnegative_age =
        |timestamp| u64::try_from(now_epoch.saturating_sub(timestamp).max(0)).unwrap_or(u64::MAX);
    let received_age = nonnegative_age(received_at_epoch);
    evidence_at_epoch.map_or(received_age, |time| received_age.max(nonnegative_age(time)))
}

pub(super) fn is_current(evidence: &MonitorEvidence, now_epoch: i64) -> bool {
    field_evidence(evidence, now_epoch).freshness == MonitorEvidenceFreshness::Current
}

pub(super) fn model_status(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    unbound_session: Option<&SessionObservation>,
    now_epoch: i64,
) -> (Option<String>, Option<MonitorFieldEvidence>, bool, bool) {
    let sessions = match &monitor.config.scope {
        MonitorScope::Session { session_id } => unbound_session
            .map(|session| vec![(session_id.as_str(), session)])
            .unwrap_or_default(),
        MonitorScope::BoundAccount {
            session_id: Some(session_id),
            ..
        } => account
            .and_then(|account| account.sessions.get(session_id))
            .map(|session| vec![(session_id.as_str(), session)])
            .unwrap_or_default(),
        MonitorScope::BoundAccount {
            session_id: None, ..
        } => account
            .map(|account| {
                account
                    .sessions
                    .iter()
                    .map(|(session_id, session)| (session_id.as_str(), session))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    if sessions.is_empty() {
        return (None, None, true, false);
    }

    let mut latest: Option<(&MonitorEvidence, &str)> = None;
    let mut unknown = false;
    let mut mismatch = false;
    for (session_id, session) in sessions {
        let Some(model) = session.model.as_ref() else {
            unknown = true;
            continue;
        };
        let Some(evidence) = monitor.evidence.iter().find(|evidence| {
            evidence.source == MonitorEvidenceSource::Statusline
                && evidence.session_id.as_deref() == Some(session_id)
                && matches!(evidence.value, MonitorEvidenceValue::Model { .. })
        }) else {
            unknown = true;
            continue;
        };
        // Model identity is a descriptor for the current session context, not
        // quota evidence. Keep its age visible in status, but do not require a
        // new callback every quota-evidence TTL while the session is active.
        let context_active = session_is_active(session, now_epoch);
        if context_active {
            mismatch |= monitor
                .config
                .expected_model
                .as_ref()
                .is_some_and(|expected| expected != &model.value);
        } else {
            unknown = true;
        }
        if latest.is_none_or(|(current, _)| {
            evidence.evidence_received_at_epoch > current.evidence_received_at_epoch
        }) {
            latest = Some((evidence, model.value.as_str()));
        }
    }
    let metadata = latest.map(|(evidence, _)| field_evidence(evidence, now_epoch));
    let model = latest.map(|(_, value)| value.to_owned());
    (model, metadata, unknown, mismatch)
}

pub(super) fn session_is_active(session: &SessionObservation, now_epoch: i64) -> bool {
    session
        .last_observation_received_at_epoch
        .is_some_and(|received| {
            received <= now_epoch && now_epoch.saturating_sub(received) <= MONITOR_EVIDENCE_TTL_SECS
        })
}

pub(super) fn prune_inactive_sessions(
    state: &mut StoreState,
    account_id: &str,
    incoming_session_id: Option<&str>,
    now_epoch: i64,
) {
    let mut protected = Vec::<String>::new();
    if let Some(account) = state.accounts.get(account_id) {
        protected.extend(
            account
                .reset_barriers
                .iter()
                .flatten()
                .filter_map(|barrier| barrier.session_id.clone()),
        );
    }
    for monitor in state.monitors.values().filter(|monitor| {
        monitor.stopped_at_epoch.is_none() && monitor.account_id.as_deref() == Some(account_id)
    }) {
        if let Some(session_id) = scope_session_id(&monitor.config.scope) {
            protected.push(session_id.to_owned());
        }
        for barrier in monitor.reset_barriers.iter().flatten() {
            let Some(dependency_sequence) = barrier.dependency_evidence_sequence else {
                continue;
            };
            let dependency = monitor.evidence.iter().find(|evidence| {
                evidence.sequence == dependency_sequence
                    && evidence.source == MonitorEvidenceSource::Statusline
                    && evidence.account_id.as_deref() == Some(account_id)
                    && matches!(
                        &evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage { .. }
                    )
            });
            if let Some(session_id) = dependency.and_then(|evidence| evidence.session_id.as_ref()) {
                protected.push(session_id.clone());
            }
        }
    }
    if let Some(account) = state.accounts.get_mut(account_id) {
        let removed = account
            .sessions
            .iter()
            .filter_map(|(session_id, session)| {
                (!session_is_active(session, now_epoch)
                    && !protected.contains(session_id)
                    && Some(session_id.as_str()) != incoming_session_id)
                    .then_some(session_id.clone())
            })
            .collect::<Vec<_>>();
        account.sessions.retain(|session_id, session| {
            session_is_active(session, now_epoch)
                || protected.contains(session_id)
                || Some(session_id.as_str()) == incoming_session_id
        });
        if !removed.is_empty() {
            for monitor in state
                .monitors
                .values_mut()
                .filter(|monitor| monitor.account_id.as_deref() == Some(account_id))
            {
                prune_monitor_session_evidence(monitor, &removed);
            }
        }
    }
}

pub(super) fn prune_inactive_unbound_sessions(
    state: &mut StoreState,
    incoming_session_id: &str,
    now_epoch: i64,
) {
    let protected = state
        .monitors
        .values()
        .filter(|monitor| monitor.stopped_at_epoch.is_none())
        .filter_map(|monitor| match &monitor.config.scope {
            MonitorScope::Session { session_id } => Some(session_id.clone()),
            MonitorScope::BoundAccount { .. } => None,
        })
        .collect::<Vec<_>>();
    let removed = state
        .unbound_sessions
        .iter()
        .filter_map(|(session_id, session)| {
            (!session_is_active(session, now_epoch)
                && !protected.contains(session_id)
                && session_id != incoming_session_id)
                .then_some(session_id.clone())
        })
        .collect::<Vec<_>>();
    state.unbound_sessions.retain(|session_id, session| {
        session_is_active(session, now_epoch)
            || protected.contains(session_id)
            || session_id == incoming_session_id
    });
    if !removed.is_empty() {
        for monitor in state.monitors.values_mut() {
            if matches!(monitor.config.scope, MonitorScope::Session { .. }) {
                prune_monitor_session_evidence(monitor, &removed);
            }
        }
    }
}

pub(super) fn prune_monitor_session_evidence(monitor: &mut DurableMonitor, removed: &[String]) {
    monitor
        .evidence_fingerprints
        .retain(|key, _| !fingerprint_key_references_removed_session(key, removed));
    if monitor.stopped_at_epoch.is_none() {
        monitor
            .evidence
            .retain(|evidence| !evidence_references_removed_session(evidence, removed));
    }
}

pub(super) fn fingerprint_key_references_removed_session(key: &str, removed: &[String]) -> bool {
    if let Some(session_id) = key.strip_prefix("model:") {
        return removed.iter().any(|removed| removed == session_id);
    }
    let Some((_, session_id)) = key
        .strip_prefix("used:")
        .or_else(|| key.strip_prefix("reset:"))
        .and_then(|rest| rest.rsplit_once(':'))
    else {
        return false;
    };
    session_id != "account" && removed.iter().any(|removed| removed == session_id)
}

pub(super) fn evidence_references_removed_session(
    evidence: &MonitorEvidence,
    removed: &[String],
) -> bool {
    evidence
        .session_id
        .as_ref()
        .is_some_and(|session_id| removed.contains(session_id))
}

pub(super) fn update_reset_barrier(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    status: &MonitorQuotaWindowStatus,
    now_epoch: i64,
    issues: &mut Vec<MonitorIssue>,
) -> bool {
    if let Some(barrier) = monitor.reset_barriers[index].as_ref() {
        let satisfied = reset_barrier_satisfied(monitor, account, index, barrier, now_epoch);
        if satisfied {
            monitor.reset_barriers[index] = None;
        } else {
            let due = barrier
                .prior_reset_at_epoch
                .map(|reset| reset.saturating_add(MONITOR_RESET_GRACE_SECS))
                .is_some_and(|due| now_epoch >= due);
            if due {
                push_issue(
                    issues,
                    issue(
                        MonitorIssueCode::ResetDueUnverified,
                        "the old reset elapsed; waiting for fresh lower usage and an advanced reset",
                        status.reset_at_epoch,
                    ),
                );
            }
            return false;
        }
        return true;
    }
    let Some(reset_at_epoch) = status.reset_at_epoch else {
        return false;
    };
    let due_at = reset_at_epoch.saturating_add(MONITOR_RESET_GRACE_SECS);
    if now_epoch < due_at {
        return false;
    }
    let used = status.used_percentage_basis_points.unwrap_or(10_000);
    monitor.reset_barriers[index] = Some(ResetBarrier {
        started_at_epoch: now_epoch,
        prior_reset_at_epoch: Some(reset_at_epoch),
        prior_used_percentage_basis_points: used,
        evidence_sequence_before: monitor.next_evidence_sequence,
        dependency_evidence_sequence: status
            .used_evidence
            .as_ref()
            .map(|evidence| evidence.evidence_sequence),
        pause_reason: MonitorIssueCode::ResetDueUnverified,
    });
    push_issue(
        issues,
        issue(
            MonitorIssueCode::ResetDueUnverified,
            "the old reset elapsed; waiting for fresh lower usage and an advanced reset",
            Some(due_at),
        ),
    );
    true
}

pub(super) fn reset_barrier_satisfied(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    barrier: &ResetBarrier,
    now_epoch: i64,
) -> bool {
    let window = if index == 0 {
        MonitorQuotaWindow::FiveHour
    } else {
        MonitorQuotaWindow::SevenDay
    };
    let Some(prior_reset) = barrier.prior_reset_at_epoch else {
        return false;
    };
    let due_at = prior_reset.saturating_add(MONITOR_RESET_GRACE_SECS);
    let required_at = due_at.max(barrier.started_at_epoch);
    if now_epoch < required_at {
        return false;
    }
    let mut resets = Vec::<(&MonitorEvidence, i64)>::new();
    let mut usages = Vec::<(&MonitorEvidence, i32)>::new();
    for evidence in &monitor.evidence {
        if evidence.sequence <= barrier.evidence_sequence_before
            || !is_current(evidence, now_epoch)
            || !evidence_is_relevant(evidence, monitor)
            || evidence.evidence_received_at_epoch < required_at
            || evidence
                .evidence_at_epoch
                .is_some_and(|observed| observed < required_at)
        {
            continue;
        }
        match evidence.value {
            MonitorEvidenceValue::QuotaReset {
                window: found,
                reset_at_epoch,
            } if found == window && reset_at_epoch > prior_reset => {
                resets.push((evidence, reset_at_epoch));
            }
            MonitorEvidenceValue::QuotaUsedPercentage {
                window: found,
                used_percentage_basis_points,
            } if found == window
                && used_percentage_basis_points < barrier.prior_used_percentage_basis_points =>
            {
                usages.push((evidence, used_percentage_basis_points));
            }
            _ => {}
        }
    }
    usages.into_iter().any(|(evidence, _)| {
        let stored_reset = match (evidence.source, account) {
            (MonitorEvidenceSource::Statusline, Some(account)) => evidence
                .session_id
                .as_deref()
                .and_then(|session_id| account.sessions.get(session_id))
                .and_then(|session| session.windows[index].used.as_ref())
                .and_then(|used| used.reset_at_epoch),
            (MonitorEvidenceSource::BrokerProjection, Some(account)) => account.broker_windows
                [index]
                .used
                .as_ref()
                .and_then(|used| used.reset_at_epoch),
            _ => None,
        };
        stored_reset.is_some_and(|reset| {
            reset > prior_reset
                && resets.iter().any(|(reset_evidence, new_reset)| {
                    reset_evidence.source == evidence.source
                        && reset_evidence.session_id == evidence.session_id
                        && *new_reset == reset
                })
        })
    })
}
