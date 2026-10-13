// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn validate_observation(
    observation: &StatuslineObservation,
    now_epoch: i64,
) -> Result<(), MonitorIssue> {
    if observation.schema_version != USAGE_STATUSLINE_INPUT_SCHEMA_VERSION {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline schema version is unsupported",
            None,
        ));
    }
    validate_identifier(&observation.session_id)?;
    if let Some(model) = observation.model.as_deref()
        && !valid_bounded_text(model, MAX_MODEL_LENGTH)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline model is outside its accepted bounds",
            None,
        ));
    }
    if let Some(version) = observation.claude_code_version.as_deref()
        && !valid_bounded_text(version, 64)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "Claude Code version is outside its accepted bounds",
            None,
        ));
    }
    for (index, window) in [
        observation.rate_limits.five_hour.as_ref(),
        observation.rate_limits.seven_day.as_ref(),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(window) = window else {
            continue;
        };
        let max_future = if index == 0 {
            5 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        } else {
            7 * 24 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        };
        if window
            .used_percentage_basis_points
            .is_some_and(|value| !(0..=10_000).contains(&value))
            || window
                .reset_at_epoch
                .is_some_and(|value| value < 0 || value > now_epoch.saturating_add(max_future))
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "statusline quota value is outside its accepted bounds",
                None,
            ));
        }
    }
    Ok(())
}

pub(super) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub(super) fn valid_source_capability_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_evidence_fingerprint(key: &str, value: &str) -> bool {
    if !valid_bounded_text(key, MAX_ID_LENGTH + 32) || !valid_bounded_text(value, 2_048) {
        return false;
    }
    if key == "spend:account" {
        return true;
    }
    if let Some(session_id) = key.strip_prefix("model:") {
        return valid_identifier(session_id);
    }
    let Some((window, scope)) = key
        .strip_prefix("used:")
        .or_else(|| key.strip_prefix("reset:"))
        .and_then(|rest| rest.split_once(':'))
    else {
        return false;
    };
    matches!(window, "five_hour" | "seven_day") && (scope == "account" || valid_identifier(scope))
}

pub(super) fn parse_counter_id(value: &str, prefix: &str) -> Option<u64> {
    let suffix = value.strip_prefix(prefix)?;
    if !(8..=20).contains(&suffix.len()) || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = suffix.parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

pub(super) fn validate_identifier(value: &str) -> Result<(), MonitorIssue> {
    if valid_identifier(value) {
        Ok(())
    } else {
        Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "identifier is empty or outside its accepted bounds",
            None,
        ))
    }
}

pub(super) fn valid_bounded_text(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_len
        && value.is_ascii()
        && !value.chars().any(char::is_control)
}

pub(super) fn apply_statusline(
    account: &mut AccountObservations,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
) -> (bool, bool) {
    let mut latest_resets = account.latest_reset_epochs;
    let new_session = !account.sessions.contains_key(&observation.session_id);
    let (session_changed, fields_changed, resets) = apply_statusline_session(
        account
            .sessions
            .entry(observation.session_id.clone())
            .or_default(),
        observation,
        now_epoch,
        input_sequence,
        latest_resets,
        new_session,
    );
    let changed = session_changed || new_session;
    let [five_hour_reset, seven_day_reset] = resets;
    latest_resets[0] = max_option(latest_resets[0], five_hour_reset);
    latest_resets[1] = max_option(latest_resets[1], seven_day_reset);
    account.latest_reset_epochs = latest_resets;
    if changed {
        let new_barriers = account
            .sessions
            .get(&observation.session_id)
            .into_iter()
            .flat_map(|session| session.windows.iter().enumerate())
            .filter_map(|(index, window)| {
                let used = window
                    .used
                    .as_ref()
                    .filter(|used| used.input_sequence == input_sequence && used.value >= 9_500)?;
                Some((
                    index,
                    AccountResetBarrier {
                        started_at_epoch: now_epoch,
                        prior_reset_at_epoch: used.reset_at_epoch,
                        prior_used_percentage_basis_points: used.value,
                        input_sequence_before: input_sequence,
                        source: MonitorEvidenceSource::Statusline,
                        session_id: Some(observation.session_id.clone()),
                        pause_reason: if used.value >= 10_000 {
                            MonitorIssueCode::LimitExhausted
                        } else {
                            MonitorIssueCode::LimitGuardReached
                        },
                    },
                ))
            })
            .collect::<Vec<_>>();
        for (index, barrier) in new_barriers {
            latch_account_reset_barrier(account, index, barrier);
        }
    }
    (changed, fields_changed)
}

fn apply_statusline_session(
    session: &mut SessionObservation,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
    watermarks: [Option<i64>; 2],
    new_session: bool,
) -> (bool, bool, [Option<i64>; 2]) {
    let mut changed = new_session;
    let mut fields_changed = false;
    if let Some(model) = observation.model.as_ref() {
        let model_changed = update_observed(
            &mut session.model,
            model.clone(),
            None,
            now_epoch,
            input_sequence,
            observation.claude_code_version.as_deref(),
        );
        changed |= model_changed;
        fields_changed |= model_changed;
    }
    let (five_hour_changed, five_hour_reset) = apply_session_window(
        &mut session.windows[0],
        observation.rate_limits.five_hour.as_ref(),
        watermarks[0],
        now_epoch,
        input_sequence,
        observation.claude_code_version.as_deref(),
    );
    let (seven_day_changed, seven_day_reset) = apply_session_window(
        &mut session.windows[1],
        observation.rate_limits.seven_day.as_ref(),
        watermarks[1],
        now_epoch,
        input_sequence,
        observation.claude_code_version.as_deref(),
    );
    changed |= five_hour_changed || seven_day_changed;
    fields_changed |= five_hour_changed || seven_day_changed;
    if fields_changed {
        session.last_observation_received_at_epoch = Some(now_epoch);
    }
    if let Some(version) = observation.claude_code_version.as_ref()
        && session.claude_code_version.as_ref() != Some(version)
    {
        session.claude_code_version = Some(version.clone());
        changed = true;
    }
    if session.last_callback_received_at_epoch != Some(now_epoch) {
        session.last_callback_received_at_epoch = Some(now_epoch);
        changed = true;
    }
    (changed, fields_changed, [five_hour_reset, seven_day_reset])
}

pub(super) fn apply_unbound_statusline(
    session: &mut SessionObservation,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
    new_session: bool,
) -> (bool, bool) {
    let watermarks = std::array::from_fn(|index| {
        session.windows[index]
            .reset
            .as_ref()
            .map(|reset| reset.value)
    });
    let (changed, fields_changed, _) = apply_statusline_session(
        session,
        observation,
        now_epoch,
        input_sequence,
        watermarks,
        new_session,
    );
    (changed, fields_changed)
}

pub(super) fn latch_account_reset_barrier(
    account: &mut AccountObservations,
    index: usize,
    incoming: AccountResetBarrier,
) {
    let Some(slot) = account.reset_barriers.get_mut(index) else {
        return;
    };
    let Some(existing) = slot.as_ref() else {
        *slot = Some(incoming);
        return;
    };
    let reset_advanced = incoming.prior_reset_at_epoch.is_some_and(|reset| {
        existing
            .prior_reset_at_epoch
            .is_none_or(|previous| reset > previous)
    });
    if incoming.prior_used_percentage_basis_points > existing.prior_used_percentage_basis_points
        || reset_advanced
    {
        let mut incoming = incoming;
        incoming.prior_reset_at_epoch =
            max_option(existing.prior_reset_at_epoch, incoming.prior_reset_at_epoch);
        *slot = Some(incoming);
    }
}

fn account_reset_barrier_satisfied(
    account: &AccountObservations,
    index: usize,
    barrier: &AccountResetBarrier,
    now_epoch: i64,
) -> bool {
    let required_at = barrier
        .prior_reset_at_epoch
        .unwrap_or(barrier.started_at_epoch)
        .saturating_add(MONITOR_RESET_GRACE_SECS)
        .max(barrier.started_at_epoch);
    if now_epoch < required_at {
        return false;
    }
    let pair_satisfies = |window: &ObservedWindow| {
        let (Some(used), Some(reset), Some(pair)) = (
            window.used.as_ref(),
            window.reset.as_ref(),
            window.paired.as_ref(),
        ) else {
            return false;
        };
        let reset_advanced = barrier
            .prior_reset_at_epoch
            .is_none_or(|previous| pair.reset_at_epoch > previous);
        pair.input_sequence > barrier.input_sequence_before
            && pair.received_at_epoch >= required_at
            && pair.received_at_epoch <= now_epoch
            && pair
                .evidence_at_epoch
                .is_none_or(|evidence_at| evidence_at >= required_at)
            && field_age(used.evidence_at_epoch, used.received_at_epoch, now_epoch)
                <= MONITOR_EVIDENCE_TTL_SECS as u64
            && field_age(reset.evidence_at_epoch, reset.received_at_epoch, now_epoch)
                <= MONITOR_EVIDENCE_TTL_SECS as u64
            && used.value == pair.used_percentage_basis_points
            && reset.value == pair.reset_at_epoch
            && used.reset_at_epoch == Some(reset.value)
            && pair.used_percentage_basis_points < barrier.prior_used_percentage_basis_points
            && reset_advanced
    };

    // The quota guard belongs to the account, so any source may confirm the
    // reset. Each candidate is one stored full-window observation, preserving
    // the source/session pairing and preventing fields from separate callbacks
    // from being combined to release the guard.
    account
        .sessions
        .values()
        .filter_map(|session| session.windows.get(index))
        .chain(account.broker_windows.get(index))
        .any(pair_satisfies)
}

pub(super) fn advance_account_reset_barriers(
    account: &mut AccountObservations,
    now_epoch: i64,
) -> bool {
    let mut changed = false;
    for index in 0..account.reset_barriers.len() {
        let Some(barrier) = account.reset_barriers[index].as_ref() else {
            continue;
        };
        if account_reset_barrier_satisfied(account, index, barrier, now_epoch) {
            account.reset_barriers[index] = None;
            changed = true;
        }
    }
    changed
}

fn apply_session_window(
    stored: &mut ObservedWindow,
    input: Option<&StatuslineQuotaWindow>,
    watermark: Option<i64>,
    now_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<&str>,
) -> (bool, Option<i64>) {
    let Some(input) = input else {
        return (false, None);
    };
    if input
        .reset_at_epoch
        .is_some_and(|reset| watermark.is_some_and(|latest| reset < latest))
    {
        // A callback from an older overlapping session cannot replace the
        // current reset or lower its utilization.
        return (false, None);
    }
    let mut changed = false;
    let mut effective_reset = stored.reset.as_ref().map(|reset| reset.value);
    if let Some(candidate_reset) = input.reset_at_epoch {
        if stored
            .reset
            .as_ref()
            .is_none_or(|reset| candidate_reset > reset.value)
        {
            stored.reset = Some(Observed {
                value: candidate_reset,
                evidence_at_epoch: None,
                received_at_epoch: now_epoch,
                input_sequence,
                claude_code_version: claude_code_version.map(str::to_owned),
            });
            effective_reset = Some(candidate_reset);
            changed = true;
        } else if stored
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value == candidate_reset)
        {
            effective_reset = Some(candidate_reset);
        }
    }
    if let Some(candidate_used) = input.used_percentage_basis_points {
        let current = stored.used.as_ref();
        let same_window = current.is_some_and(|used| used.reset_at_epoch == effective_reset);
        let reset_advanced = effective_reset.is_some_and(|reset| {
            current.is_some_and(|used| used.reset_at_epoch.is_none_or(|old| reset > old))
        });
        let watermark_allows_used =
            watermark.is_none_or(|latest| effective_reset.is_some_and(|reset| reset >= latest));
        let should_accept = current.is_none()
            || reset_advanced
            || current.is_some_and(|used| same_window && candidate_used > used.value);
        if watermark_allows_used && should_accept {
            stored.used = Some(ObservedPercentage {
                value: candidate_used,
                reset_at_epoch: effective_reset,
                evidence_at_epoch: None,
                received_at_epoch: now_epoch,
                input_sequence,
                claude_code_version: claude_code_version.map(str::to_owned),
            });
            changed = true;
        }
    }
    if let (Some(candidate_used), Some(candidate_reset)) =
        (input.used_percentage_basis_points, input.reset_at_epoch)
        && stored.used.as_ref().is_some_and(|used| {
            used.value == candidate_used && used.reset_at_epoch == Some(candidate_reset)
        })
        && stored
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value == candidate_reset)
        && stored.paired.as_ref().is_none_or(|pair| {
            pair.used_percentage_basis_points != candidate_used
                || pair.reset_at_epoch != candidate_reset
        })
    {
        stored.paired = Some(ObservedQuotaPair {
            used_percentage_basis_points: candidate_used,
            reset_at_epoch: candidate_reset,
            evidence_at_epoch: None,
            received_at_epoch: now_epoch,
            input_sequence,
            claude_code_version: claude_code_version.map(str::to_owned),
        });
        changed = true;
    }
    (changed, stored.reset.as_ref().map(|reset| reset.value))
}

pub(super) fn max_option(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn update_observed<T: Clone + PartialEq>(
    stored: &mut Option<Observed<T>>,
    value: T,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<&str>,
) -> bool {
    if stored
        .as_ref()
        .is_some_and(|current| current.value == value)
    {
        return false;
    }
    *stored = Some(Observed {
        value,
        evidence_at_epoch,
        received_at_epoch,
        input_sequence,
        claude_code_version: claude_code_version.map(str::to_owned),
    });
    true
}

pub(super) fn sync_monitor_from_account(
    accounts: &BTreeMap<String, AccountObservations>,
    account_id: &str,
    monitor: &mut DurableMonitor,
    now_epoch: i64,
) {
    let Some(account) = accounts.get(account_id) else {
        return;
    };
    for (session_id, session) in &account.sessions {
        if scope_session_id(&monitor.config.scope).is_some_and(|expected| expected != session_id) {
            continue;
        }
        if let Some(model) = &session.model {
            upsert_evidence(
                monitor,
                EvidenceInput {
                    account_id: Some(account_id),
                    session_id: Some(session_id),
                    source: MonitorEvidenceSource::Statusline,
                    evidence_at_epoch: model.evidence_at_epoch,
                    received_at_epoch: model.received_at_epoch,
                    claude_code_version: model.claude_code_version.as_deref(),
                    value: MonitorEvidenceValue::Model {
                        model: model.value.clone(),
                    },
                    fingerprint_key: format!("model:{session_id}"),
                },
            );
        }
        for index in 0..2 {
            sync_window(
                monitor,
                Some(account_id),
                Some(session_id),
                MonitorEvidenceSource::Statusline,
                index,
                &session.windows[index],
            );
        }
    }
    for index in 0..2 {
        sync_window(
            monitor,
            Some(account_id),
            None,
            MonitorEvidenceSource::BrokerProjection,
            index,
            &account.broker_windows[index],
        );
    }
    if let Some(record) = &account.spend.latest_record {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id: Some(account_id),
                session_id: None,
                source: MonitorEvidenceSource::Operator,
                evidence_at_epoch: record.evidence_at_epoch,
                received_at_epoch: record.evidence_received_at_epoch,
                claude_code_version: None,
                value: MonitorEvidenceValue::Spend {
                    amount: record.amount.clone(),
                    billing_period_start_epoch: record.billing_period_start_epoch,
                    billing_period_end_epoch: record.billing_period_end_epoch,
                    verification: record.verification,
                },
                fingerprint_key: "spend:account".to_owned(),
            },
        );
    }
    if monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR {
        monitor
            .evidence
            .drain(0..monitor.evidence.len() - MAX_EVIDENCE_PER_MONITOR);
    }
    let _ = now_epoch;
}

pub(super) fn sync_monitor_from_session(
    account_id: Option<&str>,
    session_id: &str,
    session: &SessionObservation,
    monitor: &mut DurableMonitor,
) {
    if let Some(model) = &session.model {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id: Some(session_id),
                source: MonitorEvidenceSource::Statusline,
                evidence_at_epoch: model.evidence_at_epoch,
                received_at_epoch: model.received_at_epoch,
                claude_code_version: model.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::Model {
                    model: model.value.clone(),
                },
                fingerprint_key: format!("model:{session_id}"),
            },
        );
    }
    for index in 0..2 {
        sync_window(
            monitor,
            account_id,
            Some(session_id),
            MonitorEvidenceSource::Statusline,
            index,
            &session.windows[index],
        );
    }
    if monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR {
        monitor
            .evidence
            .drain(0..monitor.evidence.len() - MAX_EVIDENCE_PER_MONITOR);
    }
}

fn sync_window(
    monitor: &mut DurableMonitor,
    account_id: Option<&str>,
    session_id: Option<&str>,
    source: MonitorEvidenceSource,
    index: usize,
    window: &ObservedWindow,
) {
    let name = if index == 0 { "five_hour" } else { "seven_day" };
    let kind = if index == 0 {
        MonitorQuotaWindow::FiveHour
    } else {
        MonitorQuotaWindow::SevenDay
    };
    if let Some(used) = &window.used {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id,
                source,
                evidence_at_epoch: used.evidence_at_epoch,
                received_at_epoch: used.received_at_epoch,
                claude_code_version: used.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::QuotaUsedPercentage {
                    window: kind,
                    used_percentage_basis_points: used.value,
                },
                fingerprint_key: format!("used:{name}:{}", session_id.unwrap_or("account")),
            },
        );
    }
    if let Some(reset) = &window.reset {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id,
                source,
                evidence_at_epoch: reset.evidence_at_epoch,
                received_at_epoch: reset.received_at_epoch,
                claude_code_version: reset.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::QuotaReset {
                    window: kind,
                    reset_at_epoch: reset.value,
                },
                fingerprint_key: format!("reset:{name}:{}", session_id.unwrap_or("account")),
            },
        );
    }
}
