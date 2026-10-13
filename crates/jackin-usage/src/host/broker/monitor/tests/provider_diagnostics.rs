// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn rate_limited_provider_preserves_retry_and_fresh_statusline_quota() {
    let (_directory, store) = open_store();
    let (binding, monitor_id) = start_approved_projection_observer(
        &store,
        "operator-local-rate-limited-diagnostics",
        "acct-provider-freshness",
        NOW,
    );
    store
        .observe_projection(&provider_projection(NOW, 1, 1), NOW)
        .expect("publish initial provider success");

    let retry_at_epoch = NOW + 900;
    let mut failed = provider_projection(NOW + 10, 2, 2);
    let account = &mut failed.providers[0].accounts[0];
    account.freshness = UsageFreshnessV1 {
        generation: 2,
        phase: UsageFreshnessPhaseV1::Failed,
        last_good_at_epoch: Some(NOW),
        retry_at_epoch: Some(retry_at_epoch),
        is_stale: true,
    };
    account.issues = vec![UsageIssueV1 {
        code: "rate_limited".to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "rate limited".to_owned(),
        retry_at_epoch: Some(retry_at_epoch),
    }];
    for group in &mut account.metric_groups {
        group.phase = UsageFreshnessPhaseV1::Failed;
        group.is_stale = true;
        group.observed_at_epoch = None;
        group.fetched_at_epoch = NOW + 10;
        group.last_success_at_epoch = Some(NOW);
    }
    store
        .observe_projection(&failed, NOW + 10)
        .expect("publish rate-limited last-good state");

    let expired_at = NOW + MONITOR_EVIDENCE_TTL_SECS + 1;
    let expired = descriptor_status(&store, &monitor_id, expired_at);
    assert_eq!(
        expired.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(expired.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_eq!(expired.five_hour.used_percentage_basis_points, Some(2_000));
    assert_eq!(expired.seven_day.used_percentage_basis_points, Some(3_000));
    for window in [&expired.five_hour, &expired.seven_day] {
        let used = window.used_evidence.as_ref().expect("retained quota value");
        assert_eq!(used.evidence_at_epoch, Some(NOW));
        assert_eq!(used.evidence_received_at_epoch, NOW);
        assert_eq!(used.age_seconds, 301);
        assert_eq!(used.freshness, MonitorEvidenceFreshness::Stale);
    }
    let rate_issue = expired
        .issues
        .iter()
        .find(|issue| issue.code == MonitorIssueCode::ProviderRateLimited)
        .expect("provider rate-limit issue is visible");
    assert_eq!(rate_issue.retry_at_epoch, Some(retry_at_epoch));

    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: observation_with_windows(
                    "fresh-statusline-after-provider-failure",
                    Some(42),
                    Some(NOW + 3_600),
                    Some(34),
                    Some(NOW + 86_400),
                ),
            },
            expired_at + 1,
        )
        .expect("ingest independent fresh statusline quota");
    let recovered = descriptor_status(&store, &monitor_id, expired_at + 1);
    assert_eq!(
        recovered.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(recovered.readiness.quota, MonitorQuotaReadiness::Ready);
    assert!(recovered.issues.iter().any(|issue| {
        issue.code == MonitorIssueCode::ProviderRateLimited
            && issue.retry_at_epoch == Some(retry_at_epoch)
    }));
    assert!(
        !recovered
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaStale)
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "This regression checks accepted used-only updates and all preserved reset/readiness invariants together."
)]
fn current_session_zero_without_reset_updates_only_used_and_keeps_reset_unverified() {
    for initial_session_used in [54, 95] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-used-only-{initial_session_used}");
        let (binding, observer_id) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id = start_provider_dispatch_guard(
            &store,
            &binding,
            &format!("used-only-{initial_session_used}"),
            NOW + 1,
        );

        let initial_source_at = NOW + 4;
        let session_reset_at = NOW + 3_600;
        let weekly_reset_at = NOW + 86_400;
        let initial = projection_with_quota_fields(
            initial_source_at,
            1,
            1,
            initial_session_used,
            Some(session_reset_at),
            92,
            Some(weekly_reset_at),
        );
        store
            .observe_projection(&initial, initial_source_at)
            .expect("publish initial paired Session and Weekly quota");

        let before = {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(
                account.broker_windows[0]
                    .used
                    .as_ref()
                    .expect("initial Session used value")
                    .value,
                initial_session_used * 100
            );
            assert!(account.broker_windows[0].paired.is_some());
            assert_eq!(
                account.broker_windows[1]
                    .used
                    .as_ref()
                    .expect("initial Weekly used value")
                    .value,
                9_200
            );
            (
                (*account).clone(),
                serde_json::to_value(&account.reset_barriers)
                    .expect("serialize account reset barrier snapshot"),
                serde_json::to_value(&state.monitors[&guard_id].reset_barriers)
                    .expect("serialize monitor reset barrier snapshot"),
            )
        };
        if initial_session_used == 95 {
            assert!(
                before.0.reset_barriers[0].is_some(),
                "the threshold sample establishes a durable account reset barrier"
            );
        }

        let first_session_source_at = initial_source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
        let first_session_received_at = first_session_source_at + 1;
        let first_used_only = projection_with_quota_fields(
            first_session_source_at,
            2,
            2,
            0,
            None,
            92,
            Some(weekly_reset_at),
        );
        store
            .observe_projection(&first_used_only, first_session_received_at)
            .expect("accept a fresh zero-usage Session field without a reset");
        let first_used_sequence = {
            let state = store.lock();
            let session = &state.accounts[&account_id].broker_windows[0];
            let first_used = session.used.as_ref().expect("first Session update");
            assert_eq!(first_used.value, 0);
            assert_eq!(first_used.evidence_at_epoch, Some(first_session_source_at));
            assert_eq!(first_used.received_at_epoch, first_session_received_at);
            assert_eq!(first_used.reset_at_epoch, None);
            assert!(session.paired.is_none());
            first_used.input_sequence
        };

        let session_source_at = first_session_source_at + 1;
        let session_received_at = session_source_at + 1;
        let repeated_used_only = projection_with_quota_fields(
            session_source_at,
            3,
            3,
            0,
            None,
            92,
            Some(weekly_reset_at),
        );
        store
            .observe_projection(&repeated_used_only, session_received_at)
            .expect("accept a newer same-value Session observation");

        let state = store.lock();
        let account = &state.accounts[&account_id];
        let session = &account.broker_windows[0];
        let used = session
            .used
            .as_ref()
            .expect("fresh Session usage is retained");
        assert_eq!(used.value, 0);
        assert_eq!(used.reset_at_epoch, None);
        assert_eq!(used.evidence_at_epoch, Some(session_source_at));
        assert_eq!(used.received_at_epoch, session_received_at);
        assert!(used.input_sequence > first_used_sequence);
        assert!(session.paired.is_none());
        let reset_fields = |reset: Option<&Observed<i64>>| {
            reset.map(|reset| {
                (
                    reset.value,
                    reset.evidence_at_epoch,
                    reset.received_at_epoch,
                    reset.input_sequence,
                    reset.claude_code_version.clone(),
                )
            })
        };
        assert_eq!(
            reset_fields(session.reset.as_ref()),
            reset_fields(before.0.broker_windows[0].reset.as_ref()),
            "missing reset evidence cannot refresh or erase the previous reset"
        );
        assert_eq!(
            account.latest_reset_epochs, before.0.latest_reset_epochs,
            "the used-only update leaves account reset maxima unchanged"
        );
        assert_eq!(
            account.statusline_reset_watermarks,
            before.0.statusline_reset_watermarks
        );
        assert_eq!(
            account.legacy_reset_admission_after_epochs,
            before.0.legacy_reset_admission_after_epochs
        );
        assert_eq!(
            serde_json::to_value(&account.reset_barriers)
                .expect("serialize account reset barrier snapshot"),
            before.1
        );
        assert_eq!(
            serde_json::to_value(&state.monitors[&guard_id].reset_barriers)
                .expect("serialize monitor reset barrier snapshot"),
            before.2,
            "the lower used value cannot satisfy or clear an existing monitor barrier"
        );

        let weekly = &account.broker_windows[1];
        assert_eq!(weekly.used.as_ref().map(|field| field.value), Some(9_200));
        assert_eq!(
            weekly.reset.as_ref().map(|field| field.value),
            Some(weekly_reset_at)
        );
        assert!(weekly.paired.is_some());
        drop(state);

        let status = descriptor_status(&store, &observer_id, session_received_at);
        assert_eq!(status.readiness.provider, MonitorProviderReadiness::Current);
        assert_eq!(status.five_hour.used_percentage_basis_points, Some(0));
        let current_used = status
            .five_hour
            .used_evidence
            .as_ref()
            .expect("current zero-usage evidence is published");
        assert_eq!(current_used.evidence_at_epoch, Some(session_source_at));
        assert_eq!(current_used.evidence_received_at_epoch, session_received_at);
        assert_eq!(current_used.freshness, MonitorEvidenceFreshness::Current);
        assert_eq!(status.five_hour.reset_at_epoch, Some(session_reset_at));
        assert_eq!(
            status.five_hour.reset_validity,
            MonitorResetValidity::Future,
            "the retained future reset keeps its original meaning"
        );
        let old_reset = status
            .five_hour
            .reset_evidence
            .as_ref()
            .expect("the old reset is retained for diagnosis");
        assert_eq!(old_reset.evidence_at_epoch, Some(initial_source_at));
        assert_eq!(old_reset.evidence_received_at_epoch, initial_source_at);
        assert_eq!(old_reset.age_seconds, 303);
        assert_eq!(old_reset.freshness, MonitorEvidenceFreshness::Stale);
        assert_ne!(status.readiness.quota, MonitorQuotaReadiness::Ready);
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::MissingReset)
        );
        assert!(!status.runnable);
        assert_eq!(status.seven_day.used_percentage_basis_points, Some(9_200));
        assert_eq!(status.seven_day.reset_at_epoch, Some(weekly_reset_at));
        assert_eq!(
            status
                .seven_day
                .used_evidence
                .as_ref()
                .expect("the sibling Weekly value remains visible")
                .freshness,
            MonitorEvidenceFreshness::Current
        );
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "This checks barrier creation, rebasing, and non-release across high and low used-only samples."
)]
fn high_session_used_only_without_reset_latches_and_low_sample_cannot_clear_barrier() {
    for initial_session_used in [54, 95] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-high-used-only-{initial_session_used}");
        let (binding, observer_id) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id = start_provider_dispatch_guard(
            &store,
            &binding,
            &format!("high-used-only-{initial_session_used}"),
            NOW + 1,
        );
        let source_at = NOW + 4;
        let session_reset_at = NOW + 3_600;
        let weekly_reset_at = NOW + 86_400;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    source_at,
                    1,
                    1,
                    initial_session_used,
                    Some(session_reset_at),
                    92,
                    Some(weekly_reset_at),
                ),
                source_at,
            )
            .expect("publish the initial complete Session observation");

        let (has_account_barrier, has_monitor_barrier, prior_account_barrier_reset) = {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            let account_barrier = account.reset_barriers[0].as_ref();
            let monitor_barrier = state.monitors[&guard_id].reset_barriers[0].as_ref();
            if initial_session_used == 54 {
                assert!(account_barrier.is_none());
                assert!(monitor_barrier.is_none());
            } else {
                assert!(account_barrier.is_some());
                assert!(monitor_barrier.is_some());
            }
            (
                account_barrier.is_some(),
                monitor_barrier.is_some(),
                account_barrier.and_then(|barrier| barrier.prior_reset_at_epoch),
            )
        };
        let expected_prior_reset = prior_account_barrier_reset
            .into_iter()
            .chain(Some(session_reset_at))
            .max();

        let used_only_source_at = source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
        let used_only_received_at = used_only_source_at + 1;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    used_only_source_at,
                    2,
                    2,
                    96,
                    None,
                    92,
                    Some(weekly_reset_at),
                ),
                used_only_received_at,
            )
            .expect("accept current high Session usage without a reset");

        let (after_high_account_barriers, after_high_monitor_barriers) = {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            let session = &account.broker_windows[0];
            let used = session
                .used
                .as_ref()
                .expect("high used-only value is retained");
            assert_eq!(used.value, 9_600);
            assert_eq!(used.reset_at_epoch, None);
            assert_eq!(used.evidence_at_epoch, Some(used_only_source_at));
            assert_eq!(used.received_at_epoch, used_only_received_at);
            assert!(session.paired.is_none());

            let account_barrier = account.reset_barriers[0]
                .as_ref()
                .expect("high usage without a reset latches an account barrier");
            assert_eq!(account_barrier.prior_reset_at_epoch, expected_prior_reset);
            assert_eq!(account_barrier.prior_used_percentage_basis_points, 9_600);
            let monitor_barrier = state.monitors[&guard_id].reset_barriers[0]
                .as_ref()
                .expect("high usage remains paused for the active observer");
            assert_eq!(
                monitor_barrier.prior_reset_at_epoch, expected_prior_reset,
                "the selected used-only orphan anchors the monitor barrier to the historical reset"
            );
            assert_eq!(monitor_barrier.prior_used_percentage_basis_points, 9_600);
            if initial_session_used == 54 {
                assert!(!has_account_barrier);
                assert!(!has_monitor_barrier);
            }

            (
                serde_json::to_value(&account.reset_barriers)
                    .expect("serialize account reset barrier snapshot"),
                serde_json::to_value(&state.monitors[&guard_id].reset_barriers)
                    .expect("serialize monitor reset barrier snapshot"),
            )
        };

        let low_used_source_at = used_only_source_at + 1;
        let low_used_received_at = low_used_source_at + 1;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    low_used_source_at,
                    3,
                    3,
                    0,
                    None,
                    92,
                    Some(weekly_reset_at),
                ),
                low_used_received_at,
            )
            .expect("accept newer low usage while retaining reset barriers");

        let state = store.lock();
        let account = &state.accounts[&account_id];
        assert_eq!(
            account.broker_windows[0]
                .used
                .as_ref()
                .map(|field| field.value),
            Some(0)
        );
        assert!(account.broker_windows[0].paired.is_none());
        assert_eq!(
            serde_json::to_value(&account.reset_barriers)
                .expect("serialize account reset barrier snapshot"),
            after_high_account_barriers
        );
        assert_eq!(
            serde_json::to_value(&state.monitors[&guard_id].reset_barriers)
                .expect("serialize monitor reset barrier snapshot"),
            after_high_monitor_barriers
        );
        drop(state);

        let status = descriptor_status(&store, &observer_id, low_used_received_at);
        assert_eq!(status.five_hour.used_percentage_basis_points, Some(0));
        assert_eq!(status.five_hour.reset_at_epoch, Some(session_reset_at));
        assert_eq!(status.readiness.provider, MonitorProviderReadiness::Current);
        assert_ne!(status.readiness.quota, MonitorQuotaReadiness::Ready);
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::MissingReset)
        );
        assert!(!status.runnable);

        let old_reset_source_at = session_reset_at + MONITOR_RESET_GRACE_SECS + 1;
        let old_reset_received_at = old_reset_source_at + 1;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    old_reset_source_at,
                    4,
                    4,
                    20,
                    Some(session_reset_at),
                    92,
                    Some(weekly_reset_at),
                ),
                old_reset_received_at,
            )
            .expect("publish a fresh low-use pair with the old reset after grace");
        {
            let state = store.lock();
            assert!(
                state.accounts[&account_id].reset_barriers[0].is_some(),
                "a complete pair with the old reset cannot release the account barrier"
            );
            assert!(
                state.monitors[&guard_id].reset_barriers[0].is_some(),
                "a complete pair with the old reset cannot release the observer barrier"
            );
        }

        let recovery_source_at = old_reset_received_at + 1;
        let recovery_received_at = recovery_source_at + 1;
        let advanced_reset_at = session_reset_at + 3_600;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    recovery_source_at,
                    5,
                    5,
                    20,
                    Some(advanced_reset_at),
                    92,
                    Some(weekly_reset_at),
                ),
                recovery_received_at,
            )
            .expect("publish a fresh complete pair after reset grace with a newer reset");
        {
            let state = store.lock();
            assert!(
                state.accounts[&account_id].reset_barriers[0].is_none(),
                "the fresh complete pair releases the account barrier"
            );
            assert!(
                state.monitors[&guard_id].reset_barriers[0].is_none(),
                "the fresh complete pair releases the observer barrier"
            );
            let session = &state.accounts[&account_id].broker_windows[0];
            assert!(session.paired.is_some());
            assert_eq!(session.used.as_ref().map(|field| field.value), Some(2_000));
            assert_eq!(
                session.reset.as_ref().map(|field| field.value),
                Some(advanced_reset_at)
            );
        }
    }
}

#[test]
fn statusline_high_without_reset_keeps_provider_reset_fence_until_advanced_pair() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-statusline-high-without-reset";
    let (binding, monitor_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let guard_id = start_provider_dispatch_guard(&store, &binding, "statusline-high", NOW + 1);
    let prior_reset = NOW + 3_600;
    let weekly_reset = NOW + 86_400;
    store
        .observe_projection(
            &projection_with_quota_fields(
                NOW + 2,
                1,
                1,
                54,
                Some(prior_reset),
                92,
                Some(weekly_reset),
            ),
            NOW + 2,
        )
        .expect("publish provider reset history before statusline high usage");

    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(&binding),
                observation: observation_with_windows(
                    "statusline-high-no-reset",
                    Some(9_500),
                    None,
                    None,
                    None,
                ),
            },
            NOW + 3,
        )
        .expect("ingest high statusline usage without a reset field");
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert!(matches!(
            account.statusline_reset_watermarks[0],
            StatuslineResetWatermark::KnownNone
        ));
        assert_eq!(account.latest_reset_epochs[0], Some(prior_reset));
        let barrier = account.reset_barriers[0]
            .as_ref()
            .expect("high statusline usage latches an account barrier");
        assert_eq!(barrier.source, MonitorEvidenceSource::Statusline);
        assert_eq!(barrier.prior_reset_at_epoch, Some(prior_reset));
        assert_eq!(barrier.prior_used_percentage_basis_points, 9_500);
        assert!(
            state.monitors[&guard_id].reset_barriers[0].is_none(),
            "without an effective reset proof, the monitor-local barrier remains unset"
        );
    }
    let status = descriptor_status(&store, &monitor_id, NOW + 3);
    assert!(
        !status.runnable,
        "the durable account barrier pauses dispatch"
    );
    assert!(status.issues.iter().any(|issue| {
        matches!(
            issue.code,
            MonitorIssueCode::LimitGuardReached | MonitorIssueCode::ResetDueUnverified
        )
    }));

    let old_reset_source_at = prior_reset + MONITOR_RESET_GRACE_SECS + 1;
    let old_reset_received_at = old_reset_source_at + 1;
    store
        .observe_projection(
            &projection_with_quota_fields(
                old_reset_source_at,
                2,
                2,
                20,
                Some(prior_reset),
                92,
                Some(weekly_reset),
            ),
            old_reset_received_at,
        )
        .expect("accept a fresh pair with the unchanged historical reset");
    {
        let state = store.lock();
        assert!(state.accounts[account_id].reset_barriers[0].is_some());
        let barrier = state.monitors[&guard_id].reset_barriers[0]
            .as_ref()
            .expect("the elapsed old reset creates a monitor-local due barrier");
        assert_eq!(barrier.prior_reset_at_epoch, Some(prior_reset));
    }
    let blocked_status = descriptor_status(&store, &monitor_id, old_reset_received_at);
    assert!(!blocked_status.runnable);
    assert!(
        blocked_status
            .issues
            .iter()
            .any(|issue| { issue.code == MonitorIssueCode::ResetDueUnverified })
    );

    let advanced_source_at = old_reset_received_at + 1;
    let advanced_received_at = advanced_source_at + 1;
    let advanced_reset = prior_reset + 3_600;
    store
        .observe_projection(
            &projection_with_quota_fields(
                advanced_source_at,
                3,
                3,
                10,
                Some(advanced_reset),
                92,
                Some(weekly_reset),
            ),
            advanced_received_at,
        )
        .expect("accept a fresh lower-use pair with a reset advanced after grace");
    {
        let state = store.lock();
        assert!(state.accounts[account_id].reset_barriers[0].is_none());
        assert!(state.monitors[&guard_id].reset_barriers[0].is_none());
    }
    let status = descriptor_status(&store, &monitor_id, advanced_received_at);
    assert_eq!(status.five_hour.used_percentage_basis_points, Some(1_000));
    assert_eq!(status.five_hour.reset_at_epoch, Some(advanced_reset));
    assert!(!status.issues.iter().any(|issue| {
        matches!(
            issue.code,
            MonitorIssueCode::LimitGuardReached
                | MonitorIssueCode::LimitExhausted
                | MonitorIssueCode::ResetDueUnverified
        )
    }));
}

#[test]
fn session_used_only_rejects_stale_ambiguous_scoped_and_old_evidence() {
    for rejected_case in [
        "stale-group",
        "ambiguous-group",
        "scoped-group",
        "old-source",
        "old-receipt",
    ] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-used-only-rejected-{rejected_case}");
        let (_binding, observer_id) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let initial_source_at = NOW + 4;
        let session_reset_at = NOW + 3_600;
        let weekly_reset_at = NOW + 86_400;
        store
            .observe_projection(
                &projection_with_quota_fields(
                    initial_source_at,
                    1,
                    1,
                    54,
                    Some(session_reset_at),
                    92,
                    Some(weekly_reset_at),
                ),
                initial_source_at,
            )
            .expect("publish the previous paired Session field");
        let before = broker_window_snapshot(&store.lock().accounts[&account_id].broker_windows[0]);

        let (source_at, received_at) = match rejected_case {
            "old-source" => (initial_source_at, initial_source_at + 2),
            "old-receipt" => (initial_source_at + 10, initial_source_at),
            _ => (initial_source_at + 20, initial_source_at + 21),
        };
        let mut candidate =
            projection_with_quota_fields(source_at, 2, 2, 0, None, 92, Some(weekly_reset_at));
        let account = &mut candidate.providers[0].accounts[0];
        match rejected_case {
            "stale-group" => {
                let session_group = account
                    .metric_groups
                    .iter_mut()
                    .find(|group| group.label == "Session")
                    .expect("principal Session metric group");
                session_group.phase = UsageFreshnessPhaseV1::Stale;
                session_group.is_stale = true;
                session_group.observed_at_epoch = None;
                session_group.last_success_at_epoch = Some(initial_source_at);
            }
            "ambiguous-group" => {
                let session_group = account
                    .metric_groups
                    .iter()
                    .find(|group| group.label == "Session")
                    .expect("principal Session metric group");
                let mut duplicate = session_group.clone();
                duplicate.group_id = "ambiguous-session-used-only".to_owned();
                duplicate.rank += 1;
                account.metric_groups.push(duplicate);
            }
            "scoped-group" => {
                let session_group = account
                    .metric_groups
                    .iter_mut()
                    .find(|group| group.label == "Session")
                    .expect("principal Session metric group");
                session_group.scope.service = Some("anthropic".to_owned());
            }
            "old-source" | "old-receipt" => {}
            other => panic!("unexpected used-only rejection case: {other}"),
        }
        if let Err(error) = store.observe_projection(&candidate, received_at) {
            assert_eq!(
                rejected_case, "ambiguous-group",
                "only a malformed ambiguous DTO may fail whole-projection validation: {error:?}"
            );
            assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
        }
        assert_eq!(
            broker_window_snapshot(&store.lock().accounts[&account_id].broker_windows[0]),
            before,
            "{rejected_case} evidence cannot replace the prior paired Session fields"
        );
        let status = descriptor_status(&store, &observer_id, received_at);
        assert_eq!(status.five_hour.used_percentage_basis_points, Some(5_400));
        assert_eq!(status.five_hour.reset_at_epoch, Some(session_reset_at));
    }
}

#[test]
fn current_used_only_after_receipt_only_reset_requires_strictly_new_times() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-receipt-only-reset";
    let (binding, observer_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let reset_received_at = NOW + 4;
    let session_reset_at = reset_received_at + 3_600;
    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(&binding),
                observation: observation_with_windows(
                    "statusline-reset-without-source-time",
                    None,
                    Some(session_reset_at),
                    None,
                    None,
                ),
            },
            reset_received_at,
        )
        .expect("ingest statusline reset with local receipt time");

    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let reset = account.sessions["statusline-reset-without-source-time"].windows[0]
            .reset
            .as_ref()
            .expect("statusline reset is retained");
        assert_eq!(reset.value, session_reset_at);
        assert_eq!(reset.evidence_at_epoch, None);
        assert_eq!(reset.received_at_epoch, reset_received_at);
    }

    let at_fence = projection_with_quota_fields(
        reset_received_at,
        1,
        1,
        0,
        None,
        92,
        Some(reset_received_at + 86_400),
    );
    store
        .observe_projection(&at_fence, reset_received_at + 1)
        .expect("a sample at the reset receipt fence is ignored safely");
    assert!(
        store.lock().accounts[account_id].broker_windows[0]
            .used
            .is_none()
    );

    let used_source_at = reset_received_at + 1;
    let used_received_at = reset_received_at + 2;
    let after_fence = projection_with_quota_fields(
        used_source_at,
        2,
        2,
        0,
        None,
        92,
        Some(reset_received_at + 86_400),
    );
    store
        .observe_projection(&after_fence, used_received_at)
        .expect("a sample strictly after both reset receipt and source fence is admitted");

    let state = store.lock();
    let broker_session = &state.accounts[account_id].broker_windows[0];
    let broker_used = broker_session
        .used
        .as_ref()
        .expect("fresh provider used-only field is retained");
    assert_eq!(broker_used.value, 0);
    assert_eq!(broker_used.reset_at_epoch, None);
    assert_eq!(broker_used.evidence_at_epoch, Some(used_source_at));
    assert_eq!(broker_used.received_at_epoch, used_received_at);
    assert!(broker_session.paired.is_none());
    drop(state);

    let status = descriptor_status(&store, &observer_id, used_received_at);
    assert_eq!(status.readiness.provider, MonitorProviderReadiness::Current);
    assert_eq!(status.five_hour.used_percentage_basis_points, Some(0));
    let used = status
        .five_hour
        .used_evidence
        .as_ref()
        .expect("provider used-only evidence is visible");
    assert!(status.evidence.iter().any(|evidence| {
        evidence.sequence == used.evidence_sequence
            && evidence.source == MonitorEvidenceSource::BrokerProjection
            && matches!(
                &evidence.value,
                MonitorEvidenceValue::QuotaUsedPercentage {
                    window: MonitorQuotaWindow::FiveHour,
                    used_percentage_basis_points: 0,
                }
            )
    }));
    assert_eq!(used.evidence_at_epoch, Some(used_source_at));
    assert_eq!(used.evidence_received_at_epoch, used_received_at);
    assert_eq!(used.freshness, MonitorEvidenceFreshness::Current);
    assert_eq!(status.five_hour.reset_at_epoch, Some(session_reset_at));
    let reset = status
        .five_hour
        .reset_evidence
        .as_ref()
        .expect("receipt-only reset remains visible");
    assert!(status.evidence.iter().any(|evidence| {
        evidence.sequence == reset.evidence_sequence
            && evidence.source == MonitorEvidenceSource::Statusline
            && matches!(
                &evidence.value,
                MonitorEvidenceValue::QuotaReset {
                    window: MonitorQuotaWindow::FiveHour,
                    reset_at_epoch,
                } if *reset_at_epoch == session_reset_at
            )
    }));
    assert_eq!(reset.evidence_at_epoch, None);
    assert_eq!(reset.evidence_received_at_epoch, reset_received_at);
    assert_eq!(reset.age_seconds, 2);
    assert_eq!(reset.freshness, MonitorEvidenceFreshness::Current);
    assert_eq!(
        status.five_hour.reset_validity,
        MonitorResetValidity::Future
    );
    assert_ne!(status.readiness.quota, MonitorQuotaReadiness::Ready);
    assert!(
        status
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::MissingReset)
    );
    assert!(!status.runnable);
}

#[test]
fn used_only_source_time_must_follow_prior_local_receipt_when_old_source_time_is_missing() {
    let prior_received_at = NOW + 10;
    let mut account = AccountObservations {
        input_sequence: 7,
        ..AccountObservations::default()
    };
    account.broker_windows[0].used = Some(ObservedPercentage {
        value: 5_400,
        reset_at_epoch: None,
        evidence_at_epoch: None,
        received_at_epoch: prior_received_at,
        input_sequence: 7,
        claude_code_version: None,
    });
    let before = broker_window_snapshot(&account.broker_windows[0]);

    let candidate_at_prior_receipt = ObservedWindow {
        used: Some(ObservedPercentage {
            value: 0,
            reset_at_epoch: None,
            evidence_at_epoch: Some(prior_received_at),
            received_at_epoch: prior_received_at + 1,
            input_sequence: 8,
            claude_code_version: None,
        }),
        reset: None,
        paired: None,
    };
    assert!(!projection::update_broker_windows(
        &mut account,
        [Some(candidate_at_prior_receipt), None],
        [false, false],
        [true, false],
        8,
    ));
    assert_eq!(broker_window_snapshot(&account.broker_windows[0]), before);

    let candidate_after_prior_receipt = ObservedWindow {
        used: Some(ObservedPercentage {
            value: 0,
            reset_at_epoch: None,
            evidence_at_epoch: Some(prior_received_at + 1),
            received_at_epoch: prior_received_at + 2,
            input_sequence: 8,
            claude_code_version: None,
        }),
        reset: None,
        paired: None,
    };
    assert!(projection::update_broker_windows(
        &mut account,
        [Some(candidate_after_prior_receipt), None],
        [false, false],
        [true, false],
        8,
    ));
    let accepted = account.broker_windows[0]
        .used
        .as_ref()
        .expect("strictly later used-only evidence is accepted");
    assert_eq!(accepted.value, 0);
    assert_eq!(accepted.evidence_at_epoch, Some(prior_received_at + 1));
    assert_eq!(accepted.received_at_epoch, prior_received_at + 2);
    assert_eq!(accepted.reset_at_epoch, None);
    assert!(account.broker_windows[0].reset.is_none());
    assert!(account.broker_windows[0].paired.is_none());
    assert_eq!(account.latest_reset_epochs[0], None);
}

#[expect(
    clippy::too_many_lines,
    reason = "The fixture builds a complete V4 policy, spend, quota, and event history for migration verification."
)]
fn v4_history_fixture() -> (tempfile::TempDir, serde_json::Value, String, String, String) {
    let (directory, store) = open_store();
    let account_id = "acct-v4-provider-history";
    let goal_id = "goal-v4-provider-history";
    seed_zero_sgd_spend(&store, account_id, NOW);
    let (binding, _) = start_approved_projection_observer(&store, account_id, account_id, NOW);
    let policy = approve_strict_policy(
        &store,
        &binding,
        goal_id,
        &Money::new(5_000, "SGD", 2),
        NOW + 1,
    );
    let monitor_id = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: binding_scope(&binding),
                    goal_id: Some(goal_id.to_owned()),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "v4-strict-history-monitor".to_owned(),
            },
            NOW + 2,
        )
        .expect("start strict history monitor")
    {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started reply, got {other:?}"),
    };
    let statusline_session_reset = NOW + 1_800;
    let statusline_weekly_reset = NOW + 80_000;
    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(&binding),
                observation: observation_with_windows(
                    "v4-statusline-reset-watermark",
                    None,
                    Some(statusline_session_reset),
                    None,
                    Some(statusline_weekly_reset),
                ),
            },
            NOW + 3,
        )
        .expect("retain statusline reset watermark alongside provider data");
    record_spend(
        &store,
        spend_input(account_id, NOW - 100, NOW + 100_000, 123, NOW + 4, "SGD"),
        NOW + 4,
    );
    let mut projection = provider_projection(NOW + 5, 1, 1);
    projection.providers[0].accounts[0].canonical_account_id = account_id.to_owned();
    for window in &mut projection.providers[0].accounts[0].windows {
        window.used_percent = Some(UsagePercent::clamp_raw(100));
        window.used_raw_percent = Some(100);
    }
    for group in &mut projection.providers[0].accounts[0].metric_groups {
        if let UsageMetricValueV1::Window {
            used_percent,
            used_raw_percent,
            ..
        } = &mut group.value
        {
            *used_percent = Some(UsagePercent::clamp_raw(100));
            *used_raw_percent = Some(100);
        }
    }
    store
        .observe_projection(&projection, NOW + 5)
        .expect("publish provider quota under the legacy unscoped account key");

    let before_migration = store.lock();
    assert!(
        before_migration.accounts[account_id]
            .reset_barriers
            .iter()
            .flatten()
            .any(|barrier| barrier.source == MonitorEvidenceSource::BrokerProjection)
    );
    assert!(
        before_migration.monitors[&monitor_id]
            .evidence
            .iter()
            .any(|evidence| {
                evidence.source == MonitorEvidenceSource::BrokerProjection
                    && matches!(
                        evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage {
                            used_percentage_basis_points: 10_000,
                            ..
                        }
                    )
            })
    );
    drop(before_migration);

    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V4 fixture");
    snapshot["schema_version"] = serde_json::json!(4);
    for account in snapshot["accounts"]
        .as_object_mut()
        .expect("persisted accounts")
        .values_mut()
    {
        let account = account.as_object_mut().expect("persisted account");
        account.remove("provider_observation");
        account.remove("statusline_reset_watermarks");
        account.remove("legacy_reset_admission_after_epochs");
    }
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        let fingerprints = monitor["evidence_fingerprints"]
            .as_object_mut()
            .expect("persisted evidence fingerprints");
        let current_fingerprints = std::mem::take(fingerprints);
        for (key, value) in current_fingerprints {
            let parts = key.split(':').collect::<Vec<_>>();
            let legacy_key = match parts.as_slice() {
                [source, field @ ("used" | "reset"), window, "account"]
                    if matches!(
                        *source,
                        "broker_projection"
                            | "statusline"
                            | "provider_spend"
                            | "operator"
                            | "local_session_log"
                    ) =>
                {
                    Some(format!("{field}:{window}:account"))
                }
                [
                    source,
                    field @ ("used" | "reset"),
                    window,
                    "session",
                    session_id,
                ] if matches!(
                    *source,
                    "broker_projection"
                        | "statusline"
                        | "provider_spend"
                        | "operator"
                        | "local_session_log"
                ) =>
                {
                    Some(format!("{field}:{window}:{session_id}"))
                }
                _ => None,
            };
            fingerprints.insert(legacy_key.unwrap_or(key), value);
        }
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            assert!(
                event["status"]["readiness"]
                    .as_object_mut()
                    .expect("V5 event readiness")
                    .remove("provider")
                    .is_some(),
                "V4 event status does not contain provider readiness"
            );
            event["status"]["schema_version"] = serde_json::json!(4);
        }
    }
    assert_ne!(
        snapshot["accounts"][account_id]["broker_windows"][0]["used"],
        serde_json::Value::Null,
        "fixture includes legacy provider quota without source attribution"
    );
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);
    (
        directory,
        snapshot,
        account_id.to_owned(),
        goal_id.to_owned(),
        monitor_id,
    )
}

#[test]
fn v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota() {
    let (directory, legacy, account_id, goal_id, monitor_id) = v4_history_fixture();
    let migrated = MonitorStore::open(directory.path()).expect("migrate V4 store");
    let state = migrated.lock();
    let snapshot = serde_json::to_value(&*state).expect("serialize migrated store");

    assert_eq!(snapshot["schema_version"], serde_json::json!(6));
    assert_eq!(
        snapshot["goals"][goal_id.as_str()],
        legacy["goals"][goal_id.as_str()]
    );
    assert_eq!(
        snapshot["policy_records"][goal_id.as_str()],
        legacy["policy_records"][goal_id.as_str()]
    );
    assert_eq!(
        snapshot["accounts"][account_id.as_str()]["spend"],
        legacy["accounts"][account_id.as_str()]["spend"]
    );

    let mut expected_monitor = legacy["monitors"][monitor_id.as_str()].clone();
    assert_eq!(
        expected_monitor["config"]["goal_id"],
        serde_json::json!(goal_id)
    );
    assert_ne!(expected_monitor["latest_decision"], serde_json::Value::Null);
    assert_ne!(expected_monitor["spend_state"], serde_json::Value::Null);
    assert!(
        !expected_monitor["events"]
            .as_array()
            .expect("legacy events")
            .is_empty()
    );
    for event in expected_monitor["events"]
        .as_array_mut()
        .expect("legacy events")
    {
        event["status"]["schema_version"] = serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION);
        event["status"]["readiness"]["provider"] = serde_json::json!("unknown");
    }
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["config"],
        expected_monitor["config"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["latest_decision"],
        expected_monitor["latest_decision"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["spend_state"],
        expected_monitor["spend_state"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["events"],
        expected_monitor["events"]
    );

    let account = &state.accounts[account_id.as_str()];
    assert!(account.provider_observation.is_none());
    assert!(account.broker_windows.iter().all(|window| {
        window.used.is_none() && window.reset.is_none() && window.paired.is_none()
    }));
    assert_eq!(
        account.latest_reset_epochs,
        [Some(NOW + 3_600), Some(NOW + 86_400)],
        "dropping unscoped provider quota preserves the account-wide reset maxima"
    );
    let migrated_monitor = &state.monitors[monitor_id.as_str()];
    assert!(
        migrated_monitor
            .evidence
            .iter()
            .all(|evidence| evidence.source != MonitorEvidenceSource::BrokerProjection)
    );
    assert!(migrated_monitor.reset_barriers.iter().all(Option::is_none));
    drop(state);

    let current = descriptor_status(&migrated, &monitor_id, NOW + 6);
    assert_eq!(
        current.readiness.provider,
        MonitorProviderReadiness::Unknown
    );
    assert_eq!(current.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert_eq!(current.five_hour.used_percentage_basis_points, None);
    assert_eq!(current.seven_day.used_percentage_basis_points, None);
    assert!(!current.runnable);
}

#[test]
fn v4_migration_rejects_malformed_event_schema_without_rewriting_source() {
    let (directory, mut snapshot, _, _, _) = v4_history_fixture();
    let monitor = snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
        .find(|monitor| {
            monitor["events"]
                .as_array()
                .is_some_and(|events| !events.is_empty())
        })
        .expect("fixture has a monitor with events");
    monitor["events"][0]["status"]["schema_version"] =
        serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION);
    let source_bytes = serde_json::to_vec(&snapshot).expect("serialize malformed V4 source");
    std::fs::write(persisted_state_path(&directory), &source_bytes)
        .expect("write malformed V4 source");

    let error = MonitorStore::open(directory.path()).expect_err("reject malformed V4 event schema");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    assert_eq!(
        std::fs::read(persisted_state_path(&directory)).expect("read rejected source"),
        source_bytes,
        "a rejected migration must not rewrite its source"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "One migration lifecycle verifies multiple cutoff rejection and admission paths plus durable barrier behavior."
)]
fn v5_migration_unknown_reset_watermark_uses_liveness_cutoff() {
    let (directory, store) = open_store();
    let account_id = "operator-local-v5-reset-watermark-migration";
    let (binding, _) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let guard_id = start_provider_dispatch_guard(&store, &binding, "v5-cutoff", NOW);
    let reset = NOW + 86_400;
    store
        .observe_projection(&weekly_projection(NOW + 3, 1, 1, 95, Some(reset)), NOW + 3)
        .expect("publish provider reset and threshold quota before migration");
    ingest_weekly_statusline(
        &store,
        &binding,
        "retained-statusline-session",
        None,
        reset,
        NOW + 4,
    );

    let no_reset_account_id = "operator-local-v5-known-none-control";
    let no_reset_binding = bind_account(&store, no_reset_account_id, NOW + 4);
    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(&no_reset_binding),
                observation: observation_with_windows(
                    "v5-no-reset-session",
                    None,
                    None,
                    None,
                    None,
                ),
            },
            NOW + 5,
        )
        .expect("create a v5 account without a reset watermark");

    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize v5 fixture");
    let cutoff = snapshot["last_now_epoch"]
        .as_i64()
        .expect("pre-migration store clock");
    snapshot["schema_version"] = serde_json::json!(5);
    remove_v6_reset_metadata(&mut snapshot);
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(5);
        }
    }
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);

    let migrated = MonitorStore::open(directory.path()).expect("migrate v5 reset watermark");
    // The collector source is process-local and must be reselected after open.
    migrated.set_experimental_collector_source(Some("acct-provider-freshness".to_owned()));
    {
        let state = migrated.lock();
        let account = &state.accounts[account_id];
        assert_eq!(state.last_now_epoch, cutoff);
        assert_eq!(account.latest_reset_epochs[1], Some(reset));
        assert!(matches!(
            &account.statusline_reset_watermarks[1],
            StatuslineResetWatermark::Unknown
        ));
        assert_eq!(account.legacy_reset_admission_after_epochs[1], Some(cutoff));
        let no_reset_account = &state.accounts[no_reset_account_id];
        assert_eq!(no_reset_account.latest_reset_epochs[1], None);
        assert!(matches!(
            &no_reset_account.statusline_reset_watermarks[1],
            StatuslineResetWatermark::KnownNone
        ));
        assert_eq!(
            no_reset_account.legacy_reset_admission_after_epochs[1],
            None
        );
    }

    let lower_reset = reset - 1;
    let before = broker_weekly_snapshot(&migrated, account_id);
    let account_barrier_before = migrated.lock().accounts[account_id].reset_barriers[1]
        .clone()
        .expect("provider threshold latches account reset barrier");
    let monitor_barrier_before = migrated.lock().monitors[&guard_id].reset_barriers[1]
        .clone()
        .expect("provider threshold latches monitor reset barrier");
    let missing_cutoff_candidate = weekly_projection(cutoff + 2, 5, 5, 82, Some(lower_reset));
    let mut missing_cutoff_account = migrated.lock().accounts[account_id].clone();
    missing_cutoff_account.legacy_reset_admission_after_epochs[1] = None;
    let missing_cutoff_window = projection::projection_windows(
        &missing_cutoff_candidate.providers[0].accounts[0],
        cutoff + 3,
        500,
    )[1]
    .clone()
    .expect("candidate has a complete provider pair");
    assert!(!projection::update_broker_windows(
        &mut missing_cutoff_account,
        [None, Some(missing_cutoff_window)],
        [false, true],
        [false, false],
        500,
    ));
    assert_eq!(
        broker_window_snapshot(&missing_cutoff_account.broker_windows[1]),
        before,
        "Unknown migration state without its cutoff fails closed"
    );

    let rejected_receipt_at_cutoff = weekly_projection(cutoff + 1, 2, 2, 82, Some(lower_reset));
    migrated
        .observe_projection(&rejected_receipt_at_cutoff, cutoff)
        .expect("retain provider status while rejecting receipt at migration cutoff");
    assert_eq!(broker_weekly_snapshot(&migrated, account_id), before);

    let rejected_source_at_cutoff = weekly_projection(cutoff, 3, 3, 82, Some(lower_reset));
    migrated
        .observe_projection(&rejected_source_at_cutoff, cutoff + 1)
        .expect("retain provider status while rejecting source at migration cutoff");
    assert_eq!(broker_weekly_snapshot(&migrated, account_id), before);

    let future_pair = weekly_projection(cutoff + 62, 4, 4, 82, Some(lower_reset));
    let future_error = migrated
        .observe_projection(&future_pair, cutoff + 1)
        .expect_err("reject a source timestamp beyond the future skew bound");
    assert_eq!(future_error.code, MonitorIssueCode::MonitorStoreUnavailable);
    assert_eq!(broker_weekly_snapshot(&migrated, account_id), before);

    let accepted_source_at = cutoff + 2;
    let accepted_received_at = cutoff + 3;
    migrated
        .observe_projection(
            &weekly_projection(accepted_source_at, 5, 5, 82, Some(lower_reset)),
            accepted_received_at,
        )
        .expect("accept complete fresh lower pair strictly after migration cutoff");
    let accepted = broker_weekly_snapshot(&migrated, account_id);
    assert_eq!(accepted.used_value, Some(8_200));
    assert_eq!(accepted.used_reset, Some(lower_reset));
    assert_eq!(accepted.used_source_at, Some(accepted_source_at));
    assert_eq!(accepted.used_received_at, Some(accepted_received_at));
    assert_eq!(accepted.reset_value, Some(lower_reset));
    assert_eq!(accepted.reset_source_at, Some(accepted_source_at));
    assert_eq!(accepted.reset_received_at, Some(accepted_received_at));
    assert!(!accepted.pair_present);
    {
        let state = migrated.lock();
        let account = &state.accounts[account_id];
        assert_eq!(account.latest_reset_epochs[1], Some(reset));
        assert!(matches!(
            &account.statusline_reset_watermarks[1],
            StatuslineResetWatermark::Unknown
        ));
        assert_eq!(account.legacy_reset_admission_after_epochs[1], Some(cutoff));
    }
    let fresh = descriptor_status(&migrated, &guard_id, accepted_received_at);
    assert_eq!(fresh.readiness.quota, MonitorQuotaReadiness::Exhausted);
    assert_eq!(fresh.seven_day.used_percentage_basis_points, Some(8_200));
    assert_eq!(fresh.seven_day.reset_at_epoch, Some(lower_reset));
    for field in [
        fresh.seven_day.used_evidence.as_ref(),
        fresh.seven_day.reset_evidence.as_ref(),
    ] {
        assert_eq!(
            field
                .expect("lower provider fields remain current")
                .freshness,
            MonitorEvidenceFreshness::Current
        );
    }
    assert_eq!(fresh.readiness.dispatch, MonitorDispatchReadiness::Blocked);
    assert!(!fresh.runnable);

    drop(migrated);
    let migrated = MonitorStore::open(directory.path()).expect("reopen migrated lower pair");
    let duplicate_at = accepted_received_at + 1;
    ingest_weekly_statusline(
        &migrated,
        &binding,
        "retained-statusline-session",
        None,
        reset,
        duplicate_at,
    );
    {
        let state = migrated.lock();
        let account = &state.accounts[account_id];
        assert!(matches!(
            &account.statusline_reset_watermarks[1],
            StatuslineResetWatermark::Unknown
        ));
        assert_eq!(account.legacy_reset_admission_after_epochs[1], Some(cutoff));
    }

    let stale_at = accepted_source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
    let stale = descriptor_status(&migrated, &guard_id, stale_at);
    assert_eq!(stale.readiness.quota, MonitorQuotaReadiness::Exhausted);
    assert_eq!(stale.seven_day.used_percentage_basis_points, Some(8_200));
    assert_eq!(stale.seven_day.reset_at_epoch, Some(lower_reset));
    assert_eq!(stale.readiness.dispatch, MonitorDispatchReadiness::Blocked);
    assert!(!stale.runnable);
    for field in [
        stale.seven_day.used_evidence.as_ref(),
        stale.seven_day.reset_evidence.as_ref(),
    ] {
        let field = field.expect("accepted lower fields remain visible after TTL");
        assert_eq!(field.evidence_at_epoch, Some(accepted_source_at));
        assert_eq!(field.evidence_received_at_epoch, accepted_received_at);
        assert!(field.age_seconds > MONITOR_EVIDENCE_TTL_SECS as u64);
        assert_eq!(field.freshness, MonitorEvidenceFreshness::Stale);
    }
    {
        let state = migrated.lock();
        let account = &state.accounts[account_id];
        assert_eq!(account.latest_reset_epochs[1], Some(reset));
        assert!(matches!(
            &account.statusline_reset_watermarks[1],
            StatuslineResetWatermark::Unknown
        ));
        assert_eq!(account.legacy_reset_admission_after_epochs[1], Some(cutoff));
        assert!(account.broker_windows[1].paired.is_none());
        assert_eq!(
            account.reset_barriers[1]
                .as_ref()
                .expect("stale repair retains account barrier")
                .started_at_epoch,
            account_barrier_before.started_at_epoch
        );
        assert_eq!(
            state.monitors[&guard_id].reset_barriers[1]
                .as_ref()
                .expect("stale repair retains monitor barrier")
                .started_at_epoch,
            monitor_barrier_before.started_at_epoch
        );
    }

    let new_session_at = stale_at + 1;
    ingest_weekly_statusline(
        &migrated,
        &binding,
        "new-statusline-session",
        None,
        reset,
        new_session_at,
    );
    let state = migrated.lock();
    assert_eq!(
        state.accounts[account_id].latest_reset_epochs[1],
        Some(reset)
    );
    assert!(matches!(
        &state.accounts[account_id].statusline_reset_watermarks[1],
        StatuslineResetWatermark::Known {
            reset_at_epoch,
            received_at_epoch,
        } if *reset_at_epoch == reset && *received_at_epoch == new_session_at
    ));
    assert_eq!(
        state.accounts[account_id].legacy_reset_admission_after_epochs[1],
        None
    );
    assert_eq!(
        state.accounts[account_id].reset_barriers[1]
            .as_ref()
            .expect("statusline reset alone cannot clear account barrier")
            .started_at_epoch,
        account_barrier_before.started_at_epoch
    );
    assert_eq!(
        state.monitors[&guard_id].reset_barriers[1]
            .as_ref()
            .expect("statusline reset alone cannot clear monitor barrier")
            .started_at_epoch,
        monitor_barrier_before.started_at_epoch
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrokerWindowSnapshot {
    used_value: Option<i32>,
    used_reset: Option<i64>,
    used_source_at: Option<i64>,
    used_received_at: Option<i64>,
    used_sequence: Option<u64>,
    reset_value: Option<i64>,
    reset_source_at: Option<i64>,
    reset_received_at: Option<i64>,
    reset_sequence: Option<u64>,
    pair_present: bool,
    pair_source_at: Option<i64>,
    pair_received_at: Option<i64>,
    pair_sequence: Option<u64>,
}

fn broker_weekly_snapshot(store: &MonitorStore, account_id: &str) -> BrokerWindowSnapshot {
    let state = store.lock();
    broker_window_snapshot(&state.accounts[account_id].broker_windows[1])
}

fn assert_weekly_snapshot_unchanged(
    store: &MonitorStore,
    account_id: &str,
    expected: &BrokerWindowSnapshot,
) {
    let actual = broker_weekly_snapshot(store, account_id);
    assert_eq!(&actual, expected);
}

fn broker_window_snapshot(window: &ObservedWindow) -> BrokerWindowSnapshot {
    BrokerWindowSnapshot {
        used_value: window.used.as_ref().map(|field| field.value),
        used_reset: window.used.as_ref().and_then(|field| field.reset_at_epoch),
        used_source_at: window
            .used
            .as_ref()
            .and_then(|field| field.evidence_at_epoch),
        used_received_at: window.used.as_ref().map(|field| field.received_at_epoch),
        used_sequence: window.used.as_ref().map(|field| field.input_sequence),
        reset_value: window.reset.as_ref().map(|field| field.value),
        reset_source_at: window
            .reset
            .as_ref()
            .and_then(|field| field.evidence_at_epoch),
        reset_received_at: window.reset.as_ref().map(|field| field.received_at_epoch),
        reset_sequence: window.reset.as_ref().map(|field| field.input_sequence),
        pair_present: window.paired.is_some(),
        pair_source_at: window
            .paired
            .as_ref()
            .and_then(|field| field.evidence_at_epoch),
        pair_received_at: window.paired.as_ref().map(|field| field.received_at_epoch),
        pair_sequence: window.paired.as_ref().map(|field| field.input_sequence),
    }
}

fn weekly_projection(
    source_at: i64,
    generation: u64,
    broker_generation: u64,
    used_percent: i32,
    reset_at: Option<i64>,
) -> UsageProjectionV1 {
    let mut projection = provider_projection(source_at, generation, broker_generation);
    let account = &mut projection.providers[0].accounts[0];
    for window in &mut account.windows {
        if window.category == UsageWindowCategoryV1::LongRange {
            window.used_percent = Some(UsagePercent::clamp_raw(used_percent));
            window.used_raw_percent = Some(used_percent);
            window.reset_at_epoch = reset_at;
        }
    }
    for group in &mut account.metric_groups {
        if group.label == "Weekly" {
            group.observed_at_epoch = Some(source_at);
            group.fetched_at_epoch = source_at;
            group.last_success_at_epoch = Some(source_at);
            group.phase = UsageFreshnessPhaseV1::Current;
            group.is_stale = false;
            group.reset_at_epoch = reset_at;
            if let UsageMetricValueV1::Window {
                used_percent: group_used,
                used_raw_percent,
                ..
            } = &mut group.value
            {
                *group_used = Some(UsagePercent::clamp_raw(used_percent));
                *used_raw_percent = Some(used_percent);
            }
        }
    }
    projection
}

fn projection_with_quota_fields(
    source_at_epoch: i64,
    account_generation: u64,
    broker_generation: u64,
    session_used_percent: i32,
    session_reset_at_epoch: Option<i64>,
    weekly_used_percent: i32,
    weekly_reset_at_epoch: Option<i64>,
) -> UsageProjectionV1 {
    let mut projection =
        provider_projection(source_at_epoch, account_generation, broker_generation);
    set_projection_quota_field(
        &mut projection,
        UsageWindowCategoryV1::Session,
        "Session",
        session_used_percent,
        session_reset_at_epoch,
        source_at_epoch,
    );
    set_projection_quota_field(
        &mut projection,
        UsageWindowCategoryV1::LongRange,
        "Weekly",
        weekly_used_percent,
        weekly_reset_at_epoch,
        source_at_epoch,
    );
    projection
}

fn set_projection_quota_field(
    projection: &mut UsageProjectionV1,
    category: UsageWindowCategoryV1,
    label: &str,
    used_percent: i32,
    reset_at_epoch: Option<i64>,
    source_at_epoch: i64,
) {
    let account = &mut projection.providers[0].accounts[0];
    account.freshness.last_good_at_epoch = Some(source_at_epoch);
    let windows = account
        .windows
        .iter_mut()
        .filter(|window| window.category == category)
        .collect::<Vec<_>>();
    assert_eq!(windows.len(), 1, "fixture has one descriptor for {label}");
    for window in windows {
        window.label = label.to_owned();
        window.used_percent = Some(UsagePercent::clamp_raw(used_percent));
        window.used_raw_percent = Some(used_percent);
        window.reset_at_epoch = reset_at_epoch;
    }

    let groups = account
        .metric_groups
        .iter_mut()
        .filter(|group| group.label == label)
        .collect::<Vec<_>>();
    assert_eq!(groups.len(), 1, "fixture has one typed group for {label}");
    for group in groups {
        group.observed_at_epoch = Some(source_at_epoch);
        group.fetched_at_epoch = source_at_epoch;
        group.last_success_at_epoch = Some(source_at_epoch);
        group.phase = UsageFreshnessPhaseV1::Current;
        group.is_stale = false;
        group.reset_at_epoch = reset_at_epoch;
        let UsageMetricValueV1::Window {
            used_percent: group_used,
            used_raw_percent,
            ..
        } = &mut group.value
        else {
            panic!("fixture {label} metric group is not a quota window");
        };
        *group_used = Some(UsagePercent::clamp_raw(used_percent));
        *used_raw_percent = Some(used_percent);
    }
}

fn all_models_weekly_projection(
    source_at: i64,
    generation: u64,
    broker_generation: u64,
    used_percent: i32,
    reset_at: i64,
    principal_current: bool,
    duplicate_principal: bool,
) -> UsageProjectionV1 {
    let mut projection = weekly_projection(
        source_at,
        generation,
        broker_generation,
        used_percent,
        Some(reset_at),
    );
    let account = &mut projection.providers[0].accounts[0];
    for window in &mut account.windows {
        if window.category == UsageWindowCategoryV1::LongRange {
            window.label = "All models".to_owned();
        }
    }

    let principal_index = account
        .metric_groups
        .iter()
        .position(|group| group.label == "Weekly")
        .expect("weekly typed group");
    let principal = &mut account.metric_groups[principal_index];
    principal.label = "All models".to_owned();
    if principal_current {
        principal.observed_at_epoch = Some(source_at);
        principal.fetched_at_epoch = source_at;
        principal.last_success_at_epoch = Some(source_at);
        principal.phase = UsageFreshnessPhaseV1::Current;
        principal.is_stale = false;
    } else {
        principal.observed_at_epoch = None;
        principal.fetched_at_epoch = source_at;
        principal.last_success_at_epoch = Some(NOW);
        principal.phase = UsageFreshnessPhaseV1::Failed;
        principal.is_stale = true;
    }

    let mut fable = account.metric_groups[principal_index].clone();
    fable.group_id = format!("fable-weekly-{generation}");
    fable.rank = 2;
    fable.label = "Fable".to_owned();
    fable.scope.model = Some("fable".to_owned());
    fable.observed_at_epoch = Some(source_at);
    fable.fetched_at_epoch = source_at;
    fable.last_success_at_epoch = Some(source_at);
    fable.phase = UsageFreshnessPhaseV1::Current;
    fable.is_stale = false;
    account.metric_groups.push(fable);

    if duplicate_principal {
        let mut duplicate = account.metric_groups[principal_index].clone();
        duplicate.group_id = format!("duplicate-all-models-weekly-{generation}");
        duplicate.rank = 3;
        duplicate.observed_at_epoch = Some(source_at);
        duplicate.fetched_at_epoch = source_at;
        duplicate.last_success_at_epoch = Some(source_at);
        duplicate.phase = UsageFreshnessPhaseV1::Current;
        duplicate.is_stale = false;
        account.metric_groups.push(duplicate);
    }

    projection
}

fn assert_weekly_status_evidence(
    status: &MonitorStatus,
    used_basis_points: i32,
    reset_at_epoch: i64,
    source_at_epoch: i64,
    received_at_epoch: i64,
) {
    assert_eq!(
        status.seven_day.used_percentage_basis_points,
        Some(used_basis_points)
    );
    assert_eq!(status.seven_day.reset_at_epoch, Some(reset_at_epoch));
    let used = status
        .seven_day
        .used_evidence
        .as_ref()
        .expect("weekly used evidence");
    assert_eq!(used.evidence_at_epoch, Some(source_at_epoch));
    assert_eq!(used.evidence_received_at_epoch, received_at_epoch);
    let reset = status
        .seven_day
        .reset_evidence
        .as_ref()
        .expect("weekly reset evidence");
    assert_eq!(reset.evidence_at_epoch, Some(source_at_epoch));
    assert_eq!(reset.evidence_received_at_epoch, received_at_epoch);
}

#[derive(Clone, Copy)]
enum PrincipalScopeAxis {
    Service,
    Model,
    Pool,
    KeyId,
}

#[test]
fn lower_reset_freshness_uses_the_unique_principal_window_identity() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-principal-window-identity";
    let (_binding, observer_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let global_reset = NOW + 86_400;
    store
        .observe_projection(&weekly_projection(NOW, 1, 1, 72, Some(global_reset)), NOW)
        .expect("publish initial provider quota");
    let before = broker_weekly_snapshot(&store, account_id);
    let initial_status = descriptor_status(&store, &observer_id, NOW);
    assert_weekly_status_evidence(&initial_status, 7_200, global_reset, NOW, NOW);

    let cases = [
        // A fresh Fable group has the same typed values and period, but its
        // label does not identify the All models principal window.
        ("fable-collision", NOW + 4, 2, false, false, None, false),
        // Two fresh All models groups are ambiguous even though one is the
        // principal group's stale last-good twin.
        ("duplicate-label", NOW + 6, 3, false, true, None, false),
        // A scoped Fable group still cannot stand in for an unscoped All
        // models principal, even with the matching label and typed values.
        (
            "scoped-model",
            NOW + 8,
            4,
            true,
            false,
            Some(PrincipalScopeAxis::Model),
            false,
        ),
        (
            "scoped-pool",
            NOW + 10,
            5,
            true,
            false,
            Some(PrincipalScopeAxis::Pool),
            false,
        ),
        (
            "scoped-key",
            NOW + 12,
            6,
            true,
            false,
            Some(PrincipalScopeAxis::KeyId),
            false,
        ),
        (
            "scoped-service",
            NOW + 14,
            7,
            true,
            false,
            Some(PrincipalScopeAxis::Service),
            false,
        ),
        // A unique current All models group is the positive control; a Fable
        // sibling still cannot affect which group's timestamp is selected.
        ("unique-principal", NOW + 16, 8, true, false, None, true),
    ];
    for (name, source_at, generation, principal_current, duplicate, principal_scope, accepted) in
        cases
    {
        let received_at = source_at + 1;
        let mut projection = all_models_weekly_projection(
            source_at,
            generation,
            generation,
            82,
            global_reset - 5,
            principal_current,
            duplicate,
        );
        if let Some(axis) = principal_scope {
            let account = &mut projection.providers[0].accounts[0];
            let principal = account
                .metric_groups
                .iter_mut()
                .find(|group| group.label == "All models")
                .expect("All models typed group");
            match axis {
                PrincipalScopeAxis::Service => {
                    principal.scope.service = Some("anthropic".to_owned());
                }
                PrincipalScopeAxis::Model => principal.scope.model = Some("fable".to_owned()),
                PrincipalScopeAxis::Pool => principal.scope.pool = Some("fable-pool".to_owned()),
                PrincipalScopeAxis::KeyId => principal.scope.key_id = Some("fable-key".to_owned()),
            }
        }
        store
            .observe_projection(&projection, received_at)
            .unwrap_or_else(|error| panic!("observe {name} projection: {error:?}"));

        let snapshot = broker_weekly_snapshot(&store, account_id);
        let status = descriptor_status(&store, &observer_id, received_at);
        if accepted {
            assert_eq!(snapshot.used_value, Some(8_200));
            assert_eq!(snapshot.used_reset, Some(global_reset - 5));
            assert_eq!(snapshot.used_source_at, Some(source_at));
            assert_eq!(snapshot.used_received_at, Some(received_at));
            assert_eq!(snapshot.reset_value, Some(global_reset - 5));
            assert_eq!(snapshot.reset_source_at, Some(source_at));
            assert_eq!(snapshot.reset_received_at, Some(received_at));
            assert!(!snapshot.pair_present);
            assert_weekly_status_evidence(&status, 8_200, global_reset - 5, source_at, received_at);
        } else {
            assert_eq!(snapshot, before, "{name} cannot replace broker evidence");
            assert_weekly_status_evidence(&status, 7_200, global_reset, NOW, NOW);
        }
        let state = store.lock();
        assert_eq!(
            state.accounts[account_id].latest_reset_epochs[1],
            Some(global_reset)
        );
    }
}

fn ingest_weekly_statusline(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    session_id: &str,
    used: Option<i32>,
    reset: i64,
    received_at: i64,
) {
    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(binding),
                observation: observation_with_windows(session_id, None, None, used, Some(reset)),
            },
            received_at,
        )
        .expect("ingest fixture statusline window");
}

fn assert_weekly_reset_watermark(
    store: &MonitorStore,
    account_id: &str,
    reset_at_epoch: i64,
    received_at_epoch: i64,
) {
    let state = store.lock();
    assert!(matches!(
        &state.accounts[account_id].statusline_reset_watermarks[1],
        StatuslineResetWatermark::Known {
            reset_at_epoch: stored_reset,
            received_at_epoch: stored_received,
        } if *stored_reset == reset_at_epoch && *stored_received == received_at_epoch
    ));
}

fn start_provider_dispatch_guard(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    name: &str,
    now_epoch: i64,
) -> String {
    seed_zero_sgd_spend(store, &binding.account_id, now_epoch);
    let goal_id = format!("goal-provider-{name}");
    let policy = approve_strict_policy(
        store,
        binding,
        &goal_id,
        &Money::new(5_000, "SGD", 2),
        now_epoch + 1,
    );
    match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: binding_scope(binding),
                    goal_id: Some(goal_id),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: format!("provider-lower-reset-guard-{name}"),
            },
            now_epoch + 2,
        )
        .expect("start provider-backed dispatch guard")
    {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started dispatch guard, got {other:?}"),
    }
}

#[test]
fn lower_reset_provider_repair_is_fresh_then_stale_and_replay_cannot_refresh_it() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-lower-reset-repair";
    let (binding, observer_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let prior_reset = NOW + 86_400;
    store
        .observe_projection(&weekly_projection(NOW, 1, 1, 72, Some(prior_reset)), NOW)
        .expect("publish initial provider quota");
    let guard_id = start_provider_dispatch_guard(&store, &binding, "freshness", NOW + 1);

    let watermark = prior_reset + 4_000;
    let watermark_received_at = NOW + 3;
    ingest_weekly_statusline(
        &store,
        &binding,
        "weekly-reset-watermark",
        None,
        watermark,
        watermark_received_at,
    );
    ingest_weekly_statusline(
        &store,
        &binding,
        "older-overlapping-session",
        Some(9_900),
        prior_reset,
        watermark_received_at + 1,
    );

    let accepted_source_at = watermark_received_at + 2;
    let accepted_received_at = accepted_source_at + 1;
    let accepted = weekly_projection(accepted_source_at, 2, 2, 82, Some(prior_reset));
    store
        .observe_projection(&accepted, accepted_received_at)
        .expect("accept current same-source pair below the statusline watermark");

    let accepted_snapshot = broker_weekly_snapshot(&store, account_id);
    assert_eq!(accepted_snapshot.used_value, Some(8_200));
    assert_eq!(accepted_snapshot.used_reset, Some(prior_reset));
    assert_eq!(accepted_snapshot.used_source_at, Some(accepted_source_at));
    assert_eq!(
        accepted_snapshot.used_received_at,
        Some(accepted_received_at)
    );
    assert_eq!(accepted_snapshot.reset_value, Some(prior_reset));
    assert_eq!(accepted_snapshot.reset_source_at, Some(accepted_source_at));
    assert_eq!(
        accepted_snapshot.reset_received_at,
        Some(accepted_received_at)
    );
    assert!(
        !accepted_snapshot.pair_present,
        "lower repair cannot prove a reset"
    );
    {
        let state = store.lock();
        assert_eq!(
            state.accounts[account_id].latest_reset_epochs[1],
            Some(watermark)
        );
        let source = state.accounts[account_id]
            .provider_observation
            .as_ref()
            .expect("selected provider identity remains bound")
            .clone();
        assert_eq!(source.source_account_id, "acct-provider-freshness");
        assert_eq!(
            source.binding_id.as_deref(),
            Some(binding.binding_id.as_str())
        );
        assert_eq!(source.binding_revision, Some(binding.revision));
    }

    let current = descriptor_status(&store, &guard_id, accepted_received_at);
    assert_eq!(current.readiness.quota, MonitorQuotaReadiness::Ready);
    assert_eq!(current.seven_day.used_percentage_basis_points, Some(8_200));
    assert_eq!(current.seven_day.reset_at_epoch, Some(prior_reset));
    assert!(current.evidence.iter().any(|evidence| {
        evidence.source == MonitorEvidenceSource::BrokerProjection
            && evidence.session_id.is_none()
            && evidence.value
                == (MonitorEvidenceValue::QuotaUsedPercentage {
                    window: MonitorQuotaWindow::SevenDay,
                    used_percentage_basis_points: 8_200,
                })
    }));

    let replayed_at = accepted_source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
    store
        .observe_projection(&accepted, replayed_at)
        .expect("replay cached provider pair");
    assert_eq!(
        broker_weekly_snapshot(&store, account_id),
        accepted_snapshot
    );
    let stale = descriptor_status(&store, &guard_id, replayed_at);
    assert_eq!(stale.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_eq!(stale.readiness.dispatch, MonitorDispatchReadiness::Blocked);
    assert!(!stale.runnable);
    assert_eq!(stale.seven_day.used_percentage_basis_points, Some(8_200));
    assert_eq!(stale.seven_day.reset_at_epoch, Some(prior_reset));
    for field in [
        stale.seven_day.used_evidence.as_ref(),
        stale.seven_day.reset_evidence.as_ref(),
    ] {
        let field = field.expect("last-good lower-reset fields stay visible");
        assert_eq!(field.evidence_received_at_epoch, accepted_received_at);
        assert_eq!(field.age_seconds, 301);
        assert_eq!(field.freshness, MonitorEvidenceFreshness::Stale);
    }
    let observer = descriptor_status(&store, &observer_id, replayed_at);
    assert_eq!(observer.seven_day.used_percentage_basis_points, Some(8_200));
    assert_eq!(
        observer
            .seven_day
            .used_evidence
            .unwrap()
            .evidence_received_at_epoch,
        accepted_received_at
    );
}

#[test]
fn lower_reset_provider_pair_can_repair_an_account_without_prior_broker_fields() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-lower-reset-no-prior";
    let (binding, monitor_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let lower_reset = NOW + 86_400;
    let watermark = lower_reset + 4_000;
    ingest_weekly_statusline(
        &store,
        &binding,
        "reset-before-provider",
        None,
        watermark,
        NOW,
    );
    store
        .observe_projection(
            &weekly_projection(NOW + 1, 1, 1, 82, Some(lower_reset)),
            NOW + 2,
        )
        .expect("accept fresh complete source pair after watermark");

    let snapshot = broker_weekly_snapshot(&store, account_id);
    assert_eq!(snapshot.used_value, Some(8_200));
    assert_eq!(snapshot.used_reset, Some(lower_reset));
    assert_eq!(snapshot.used_source_at, Some(NOW + 1));
    assert_eq!(snapshot.used_received_at, Some(NOW + 2));
    assert!(!snapshot.pair_present);
    let state = store.lock();
    assert_eq!(
        state.accounts[account_id].latest_reset_epochs[1],
        Some(watermark)
    );
    drop(state);
    let status = descriptor_status(&store, &monitor_id, NOW + 2);
    assert_eq!(status.readiness.quota, MonitorQuotaReadiness::Ready);
    assert_eq!(status.seven_day.used_percentage_basis_points, Some(8_200));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps malformed, stale, and valid partial projection cases in one ordered regression."
)]
fn malformed_or_stale_lower_reset_projection_preserves_fields_and_current_partial_is_visible() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-lower-reset-rejected";
    let (binding, monitor_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let prior_reset = NOW + 86_400;
    store
        .observe_projection(&weekly_projection(NOW, 1, 1, 72, Some(prior_reset)), NOW)
        .expect("publish last-good weekly pair");
    let watermark = prior_reset + 4_000;
    ingest_weekly_statusline(
        &store,
        &binding,
        "rejection-watermark",
        None,
        watermark,
        NOW + 1,
    );
    let baseline = broker_weekly_snapshot(&store, account_id);
    assert_eq!(baseline.used_value, Some(7_200));
    assert!(baseline.pair_present);

    let at_watermark = weekly_projection(NOW + 1, 2, 2, 88, Some(prior_reset + 5));
    store
        .observe_projection(&at_watermark, NOW + 2)
        .expect("reject a source timestamp equal to the watermark receipt");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let mut stale = weekly_projection(NOW + 3, 3, 3, 88, Some(prior_reset + 5));
    let weekly = stale.providers[0].accounts[0]
        .metric_groups
        .iter_mut()
        .find(|group| group.label == "Weekly")
        .expect("weekly typed group");
    weekly.phase = UsageFreshnessPhaseV1::Stale;
    weekly.is_stale = true;
    weekly.observed_at_epoch = None;
    weekly.last_success_at_epoch = Some(NOW);
    store
        .observe_projection(&stale, NOW + 3)
        .expect("reject stale typed pair");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let mut missing_group = weekly_projection(NOW + 4, 4, 4, 88, Some(prior_reset + 5));
    missing_group.providers[0].accounts[0]
        .metric_groups
        .retain(|group| group.label != "Weekly");
    store
        .observe_projection(&missing_group, NOW + 4)
        .expect("reject pair without its typed weekly group");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let mut mismatched_cohort = weekly_projection(NOW + 5, 5, 5, 88, Some(prior_reset + 5));
    let weekly = mismatched_cohort.providers[0].accounts[0]
        .metric_groups
        .iter_mut()
        .find(|group| group.label == "Weekly")
        .expect("weekly typed group");
    weekly.reset_at_epoch = Some(prior_reset);
    store
        .observe_projection(&mismatched_cohort, NOW + 5)
        .expect("reject used/reset mismatch");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let mut future = weekly_projection(NOW + 7, 7, 7, 88, Some(prior_reset + 5));
    let weekly = future.providers[0].accounts[0]
        .metric_groups
        .iter_mut()
        .find(|group| group.label == "Weekly")
        .expect("weekly typed group");
    weekly.observed_at_epoch = Some(NOW + 7 + 61);
    store
        .observe_projection(&future, NOW + 7)
        .expect("reject a future-dated sibling field");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let expired_source_at = NOW + 8;
    let expired_received_at = expired_source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
    let expired = weekly_projection(expired_source_at, 8, 8, 88, Some(prior_reset + 5));
    store
        .observe_projection(&expired, expired_received_at)
        .expect("reject out-of-TTL source pair");
    assert_weekly_snapshot_unchanged(&store, account_id, &baseline);

    let mut account_state = store.lock().accounts[account_id].clone();
    let before_malformed = broker_window_snapshot(&account_state.broker_windows[1]);
    let malformed_attached_reset = ObservedWindow {
        used: Some(ObservedPercentage {
            value: 8_800,
            reset_at_epoch: Some(prior_reset + 5),
            evidence_at_epoch: Some(NOW + 10),
            received_at_epoch: NOW + 11,
            input_sequence: 0,
            claude_code_version: None,
        }),
        reset: None,
        paired: None,
    };
    assert!(!projection::update_broker_windows(
        &mut account_state,
        [None, Some(malformed_attached_reset)],
        [false, false],
        [false, false],
        100,
    ));
    assert_eq!(
        broker_window_snapshot(&account_state.broker_windows[1]),
        before_malformed,
        "a lower reset attached only to used evidence cannot bypass the watermark"
    );

    let status = descriptor_status(&store, &monitor_id, expired_received_at);
    assert_eq!(status.seven_day.used_percentage_basis_points, None);
    assert!(status.seven_day.used_evidence.is_none());

    let partial_source_at = expired_received_at + 1;
    let partial_received_at = partial_source_at + 1;
    let partial = weekly_projection(partial_source_at, 9, 9, 88, None);
    store
        .observe_projection(&partial, partial_received_at)
        .expect("admit a fresh Weekly used-only observation without reset");
    let updated = broker_weekly_snapshot(&store, account_id);
    assert_eq!(updated.used_value, Some(8_800));
    assert_eq!(updated.used_reset, None);
    assert_eq!(updated.used_source_at, Some(partial_source_at));
    assert_eq!(updated.used_received_at, Some(partial_received_at));
    assert!(updated.used_sequence > baseline.used_sequence);
    assert_eq!(updated.reset_value, baseline.reset_value);
    assert_eq!(updated.reset_source_at, baseline.reset_source_at);
    assert_eq!(updated.reset_received_at, baseline.reset_received_at);
    assert!(!updated.pair_present);
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert_eq!(account.latest_reset_epochs[1], Some(watermark));
        assert!(account.reset_barriers[1].is_none());
    }
    let partial_status = descriptor_status(&store, &monitor_id, partial_received_at);
    assert_eq!(
        partial_status.seven_day.used_percentage_basis_points,
        Some(8_800)
    );
    assert_eq!(
        partial_status
            .seven_day
            .used_evidence
            .as_ref()
            .expect("current partial Weekly usage is visible")
            .freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_ne!(
        partial_status.readiness.quota,
        MonitorQuotaReadiness::Ready,
        "missing reset evidence keeps quota fail-closed"
    );
    assert!(
        partial_status
            .issues
            .iter()
            .any(|issue| { issue.code == MonitorIssueCode::MissingReset })
    );
    assert!(!partial_status.runnable);
}

#[test]
fn lower_reset_quota_read_fence_rejects_split_and_future_pair_siblings() {
    let (_directory, store) = open_store();
    let account_id = "operator-local-lower-reset-read-fence";
    let (binding, monitor_id) =
        start_approved_projection_observer(&store, account_id, "acct-provider-freshness", NOW);
    let lower_reset = NOW + 86_400;
    let watermark = lower_reset + 4_000;
    ingest_weekly_statusline(
        &store,
        &binding,
        "read-fence-watermark",
        None,
        watermark,
        NOW,
    );
    let source_at = NOW + 1;
    let received_at = NOW + 2;
    store
        .observe_projection(
            &weekly_projection(source_at, 1, 1, 82, Some(lower_reset)),
            received_at,
        )
        .expect("publish accepted lower-reset pair");

    let state = store.lock();
    let account = &state.accounts[account_id];
    let monitor = &state.monitors[&monitor_id];
    let used_evidence = monitor
        .evidence
        .iter()
        .find(|evidence| {
            evidence.source == MonitorEvidenceSource::BrokerProjection
                && evidence.value
                    == (MonitorEvidenceValue::QuotaUsedPercentage {
                        window: MonitorQuotaWindow::SevenDay,
                        used_percentage_basis_points: 8_200,
                    })
        })
        .expect("broker weekly used evidence");
    let reset_evidence = monitor
        .evidence
        .iter()
        .find(|evidence| {
            evidence.source == MonitorEvidenceSource::BrokerProjection
                && evidence.value
                    == (MonitorEvidenceValue::QuotaReset {
                        window: MonitorQuotaWindow::SevenDay,
                        reset_at_epoch: lower_reset,
                    })
        })
        .expect("broker weekly reset evidence");

    let mut split_source = account.clone();
    split_source.broker_windows[1]
        .reset
        .as_mut()
        .unwrap()
        .evidence_at_epoch = Some(source_at + 1);
    assert!(!quota::evidence_is_effective_used(
        used_evidence,
        Some(&split_source),
        1,
        received_at,
    ));
    assert!(!quota::evidence_is_effective_reset(
        reset_evidence,
        Some(&split_source),
        1,
        received_at,
    ));

    let mut split_receipt = account.clone();
    split_receipt.broker_windows[1]
        .reset
        .as_mut()
        .unwrap()
        .received_at_epoch = received_at + 1;
    assert!(!quota::evidence_is_effective_used(
        used_evidence,
        Some(&split_receipt),
        1,
        received_at,
    ));
    assert!(!quota::evidence_is_effective_reset(
        reset_evidence,
        Some(&split_receipt),
        1,
        received_at,
    ));

    let future_source_at = received_at + 61;
    let mut future_sibling = account.clone();
    future_sibling.broker_windows[1]
        .used
        .as_mut()
        .unwrap()
        .evidence_at_epoch = Some(future_source_at);
    future_sibling.broker_windows[1]
        .reset
        .as_mut()
        .unwrap()
        .evidence_at_epoch = Some(future_source_at);
    let mut future_used = used_evidence.clone();
    future_used.evidence_at_epoch = Some(future_source_at);
    let mut future_reset = reset_evidence.clone();
    future_reset.evidence_at_epoch = Some(future_source_at);
    assert!(!quota::evidence_is_effective_used(
        &future_used,
        Some(&future_sibling),
        1,
        received_at,
    ));
    assert!(!quota::evidence_is_effective_reset(
        &future_reset,
        Some(&future_sibling),
        1,
        received_at,
    ));

    let mut future_receipt = account.clone();
    future_receipt.broker_windows[1]
        .reset
        .as_mut()
        .unwrap()
        .received_at_epoch = received_at + 61;
    let mut future_receipt_used = used_evidence.clone();
    future_receipt_used.evidence_received_at_epoch = received_at + 61;
    let mut future_receipt_reset = reset_evidence.clone();
    future_receipt_reset.evidence_received_at_epoch = received_at + 61;
    assert!(!quota::evidence_is_effective_used(
        &future_receipt_used,
        Some(&future_receipt),
        1,
        received_at,
    ));
    assert!(!quota::evidence_is_effective_reset(
        &future_receipt_reset,
        Some(&future_receipt),
        1,
        received_at,
    ));
}

#[test]
fn lower_reset_repair_cannot_release_95_or_100_percent_reset_barrier() {
    for (high_used, expected_pause) in [
        (9_500, MonitorIssueCode::LimitGuardReached),
        (10_000, MonitorIssueCode::LimitExhausted),
    ] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-lower-reset-barrier-{high_used}");
        let (binding, _) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id =
            start_provider_dispatch_guard(&store, &binding, &format!("barrier-{high_used}"), NOW);
        let prior_reset = NOW + 86_400;
        store
            .observe_projection(
                &weekly_projection(NOW + 3, 1, 1, high_used / 100, Some(prior_reset)),
                NOW + 3,
            )
            .expect("publish threshold-crossing provider quota");
        let before = {
            let state = store.lock();
            state.accounts[&account_id].reset_barriers[1]
                .as_ref()
                .expect("threshold latches durable reset barrier")
                .clone()
        };
        assert_eq!(before.pause_reason, expected_pause);
        let watermark = prior_reset + 10;
        ingest_weekly_statusline(
            &store,
            &binding,
            "barrier-watermark",
            None,
            watermark,
            NOW + 4,
        );

        let lower_but_advanced_reset = prior_reset + 5;
        store
            .observe_projection(
                &weekly_projection(NOW + 6, 2, 2, 50, Some(lower_but_advanced_reset)),
                NOW + 6,
            )
            .expect("accept lower-than-watermark quota correction");
        let state = store.lock();
        let account = &state.accounts[&account_id];
        assert_eq!(account.latest_reset_epochs[1], Some(watermark));
        assert!(account.broker_windows[1].paired.is_none());
        let after = account.reset_barriers[1]
            .as_ref()
            .expect("lower correction cannot clear the reset barrier");
        assert_eq!(after.started_at_epoch, before.started_at_epoch);
        assert_eq!(after.prior_reset_at_epoch, Some(prior_reset));
        assert_eq!(after.prior_used_percentage_basis_points, high_used);
        drop(state);

        let status = descriptor_status(&store, &guard_id, NOW + 6);
        assert!(!status.runnable);
        assert_eq!(status.readiness.dispatch, MonitorDispatchReadiness::Blocked);
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == expected_pause)
        );
        assert_eq!(status.seven_day.used_percentage_basis_points, Some(5_000));
    }
}

#[test]
fn known_statusline_lower_than_provider_global_fences_lower_provider_repair() {
    for (high_used_percent, expected_pause) in [
        (95, MonitorIssueCode::LimitGuardReached),
        (100, MonitorIssueCode::LimitExhausted),
    ] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-known-lower-reset-{high_used_percent}");
        let (binding, _) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id = start_provider_dispatch_guard(
            &store,
            &binding,
            &format!("known-lower-{high_used_percent}"),
            NOW,
        );
        let provider_reset = NOW + 86_400;
        store
            .observe_projection(
                &weekly_projection(NOW + 3, 1, 1, high_used_percent, Some(provider_reset)),
                NOW + 3,
            )
            .expect("publish provider-only global reset and threshold quota");

        let (account_barrier_before, monitor_barrier_before) = {
            let state = store.lock();
            assert!(matches!(
                &state.accounts[&account_id].statusline_reset_watermarks[1],
                StatuslineResetWatermark::KnownNone
            ));
            let account_barrier = state.accounts[&account_id].reset_barriers[1]
                .as_ref()
                .expect("provider threshold latches account reset barrier")
                .clone();
            let monitor_barrier = state.monitors[&guard_id].reset_barriers[1]
                .as_ref()
                .expect("provider threshold latches monitor reset barrier")
                .clone();
            (account_barrier, monitor_barrier)
        };
        assert_eq!(account_barrier_before.pause_reason, expected_pause);

        let lower_statusline_reset = provider_reset - 5;
        let statusline_received_at = NOW + 4;
        ingest_weekly_statusline(
            &store,
            &binding,
            "new-lower-statusline-reset",
            Some(7_500),
            lower_statusline_reset,
            statusline_received_at,
        );
        {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(account.latest_reset_epochs[1], Some(provider_reset));
            assert!(matches!(
                &account.statusline_reset_watermarks[1],
                StatuslineResetWatermark::Known {
                    reset_at_epoch,
                    received_at_epoch,
                } if *reset_at_epoch == lower_statusline_reset
                    && *received_at_epoch == statusline_received_at
            ));
            let session = account
                .sessions
                .get("new-lower-statusline-reset")
                .expect("new lower statusline session is retained");
            let window = &session.windows[1];
            assert_eq!(window.used.as_ref().map(|used| used.value), Some(7_500));
            assert_eq!(
                window.used.as_ref().and_then(|used| used.reset_at_epoch),
                Some(lower_statusline_reset)
            );
            assert!(window.paired.is_some(), "the lower statusline pair exists");
        }

        let lower_provider_reset = provider_reset - 6;
        assert!(lower_provider_reset < lower_statusline_reset);
        let repair_source_at = NOW + 6;
        let repair_received_at = NOW + 7;
        store
            .observe_projection(
                &weekly_projection(repair_source_at, 2, 2, 50, Some(lower_provider_reset)),
                repair_received_at,
            )
            .expect("Known statusline receipt admits a newer lower provider pair");
        let repaired = broker_weekly_snapshot(&store, &account_id);
        assert_eq!(repaired.used_value, Some(5_000));
        assert_eq!(repaired.used_reset, Some(lower_provider_reset));
        assert_eq!(repaired.used_source_at, Some(repair_source_at));
        assert_eq!(repaired.used_received_at, Some(repair_received_at));
        assert_eq!(repaired.reset_value, Some(lower_provider_reset));
        assert_eq!(repaired.reset_source_at, Some(repair_source_at));
        assert_eq!(repaired.reset_received_at, Some(repair_received_at));
        assert!(!repaired.pair_present);
        {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(account.latest_reset_epochs[1], Some(provider_reset));
            assert!(matches!(
                &account.statusline_reset_watermarks[1],
                StatuslineResetWatermark::Known {
                    reset_at_epoch,
                    received_at_epoch,
                } if *reset_at_epoch == lower_statusline_reset
                    && *received_at_epoch == statusline_received_at
            ));
            assert_eq!(
                account.reset_barriers[1]
                    .as_ref()
                    .expect("lower provider repair retains account barrier")
                    .started_at_epoch,
                account_barrier_before.started_at_epoch
            );
            assert_eq!(
                state.monitors[&guard_id].reset_barriers[1]
                    .as_ref()
                    .expect("lower provider repair retains monitor barrier")
                    .started_at_epoch,
                monitor_barrier_before.started_at_epoch
            );
        }
        let status = descriptor_status(&store, &guard_id, repair_received_at);
        assert_eq!(status.readiness.quota, MonitorQuotaReadiness::Exhausted);
        assert_eq!(status.seven_day.used_percentage_basis_points, Some(5_000));
        assert_eq!(status.seven_day.reset_at_epoch, Some(lower_provider_reset));
        assert_eq!(
            status
                .seven_day
                .used_evidence
                .as_ref()
                .expect("lower quota fields remain fresh")
                .freshness,
            MonitorEvidenceFreshness::Current
        );
        assert_eq!(
            status
                .seven_day
                .reset_evidence
                .as_ref()
                .expect("lower reset field remains fresh")
                .freshness,
            MonitorEvidenceFreshness::Current
        );
        assert_eq!(status.readiness.dispatch, MonitorDispatchReadiness::Blocked);
        assert!(!status.runnable);
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == expected_pause)
        );
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The persisted provider-only lifecycle checks two successive lower repairs, reads, TTL, and both quota barriers."
)]
fn known_none_provider_only_lower_repairs_advance_from_prior_receipt() {
    for (high_used_percent, expected_pause) in [
        (95, MonitorIssueCode::LimitGuardReached),
        (100, MonitorIssueCode::LimitExhausted),
    ] {
        let (_directory, store) = open_store();
        let account_id = format!("operator-local-known-none-repairs-{high_used_percent}");
        let (binding, _) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id = start_provider_dispatch_guard(
            &store,
            &binding,
            &format!("known-none-repairs-{high_used_percent}"),
            NOW,
        );
        let global_reset = NOW + 86_400;
        store
            .observe_projection(
                &weekly_projection(NOW + 3, 1, 1, high_used_percent, Some(global_reset)),
                NOW + 3,
            )
            .expect("publish provider-only global reset");
        let (account_barrier_before, monitor_barrier_before) = {
            let state = store.lock();
            assert!(matches!(
                &state.accounts[&account_id].statusline_reset_watermarks[1],
                StatuslineResetWatermark::KnownNone
            ));
            let account_barrier = state.accounts[&account_id].reset_barriers[1]
                .as_ref()
                .expect("threshold quota latches account barrier")
                .clone();
            let monitor_barrier = state.monitors[&guard_id].reset_barriers[1]
                .as_ref()
                .expect("threshold quota latches monitor barrier")
                .clone();
            (account_barrier, monitor_barrier)
        };
        assert_eq!(account_barrier_before.pause_reason, expected_pause);

        for (source_at, received_at, used_percent, reset_at, generation) in [
            (NOW + 4, NOW + 5, 45, global_reset - 1, 2),
            (NOW + 6, NOW + 7, 40, global_reset - 2, 3),
        ] {
            store
                .observe_projection(
                    &weekly_projection(
                        source_at,
                        generation,
                        generation,
                        used_percent,
                        Some(reset_at),
                    ),
                    received_at,
                )
                .expect("fresh KnownNone lower pair advances beyond prior broker receipt");
            let snapshot = broker_weekly_snapshot(&store, &account_id);
            assert_eq!(snapshot.used_value, Some(used_percent * 100));
            assert_eq!(snapshot.used_reset, Some(reset_at));
            assert_eq!(snapshot.used_source_at, Some(source_at));
            assert_eq!(snapshot.used_received_at, Some(received_at));
            assert_eq!(snapshot.reset_value, Some(reset_at));
            assert_eq!(snapshot.reset_source_at, Some(source_at));
            assert_eq!(snapshot.reset_received_at, Some(received_at));
            assert!(!snapshot.pair_present);

            let status = descriptor_status(&store, &guard_id, received_at);
            assert_eq!(status.readiness.quota, MonitorQuotaReadiness::Exhausted);
            assert_eq!(
                status.seven_day.used_percentage_basis_points,
                Some(used_percent * 100)
            );
            assert_eq!(status.seven_day.reset_at_epoch, Some(reset_at));
            assert_eq!(
                status
                    .seven_day
                    .used_evidence
                    .as_ref()
                    .expect("lower quota fields remain fresh")
                    .freshness,
                MonitorEvidenceFreshness::Current
            );
            assert_eq!(
                status
                    .seven_day
                    .reset_evidence
                    .as_ref()
                    .expect("lower reset field remains fresh")
                    .freshness,
                MonitorEvidenceFreshness::Current
            );
            assert_eq!(status.readiness.dispatch, MonitorDispatchReadiness::Blocked);
            assert!(!status.runnable);
            assert!(
                status
                    .issues
                    .iter()
                    .any(|issue| issue.code == expected_pause)
            );

            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(account.latest_reset_epochs[1], Some(global_reset));
            assert!(matches!(
                &account.statusline_reset_watermarks[1],
                StatuslineResetWatermark::KnownNone
            ));
            assert!(account.broker_windows[1].paired.is_none());
            assert_eq!(
                account.reset_barriers[1]
                    .as_ref()
                    .expect("lower pair retains account barrier")
                    .started_at_epoch,
                account_barrier_before.started_at_epoch
            );
            assert_eq!(
                state.monitors[&guard_id].reset_barriers[1]
                    .as_ref()
                    .expect("lower pair retains monitor barrier")
                    .started_at_epoch,
                monitor_barrier_before.started_at_epoch
            );
        }

        let stale_at = NOW + 6 + MONITOR_EVIDENCE_TTL_SECS + 1;
        let stale = descriptor_status(&store, &guard_id, stale_at);
        assert_eq!(stale.readiness.quota, MonitorQuotaReadiness::Exhausted);
        assert_eq!(stale.seven_day.used_percentage_basis_points, Some(4_000));
        assert_eq!(stale.seven_day.reset_at_epoch, Some(global_reset - 2));
        assert_eq!(stale.readiness.dispatch, MonitorDispatchReadiness::Blocked);
        assert!(!stale.runnable);
        for field in [
            stale.seven_day.used_evidence.as_ref(),
            stale.seven_day.reset_evidence.as_ref(),
        ] {
            let field = field.expect("accepted lower fields remain visible after TTL");
            assert_eq!(field.freshness, MonitorEvidenceFreshness::Stale);
            assert_eq!(field.evidence_at_epoch, Some(NOW + 6));
            assert_eq!(field.evidence_received_at_epoch, NOW + 7);
            assert!(field.age_seconds > MONITOR_EVIDENCE_TTL_SECS as u64);
        }
        let state = store.lock();
        let account = &state.accounts[&account_id];
        assert_eq!(account.latest_reset_epochs[1], Some(global_reset));
        assert!(account.broker_windows[1].paired.is_none());
        assert_eq!(
            account.reset_barriers[1]
                .as_ref()
                .expect("stale lower pair retains account barrier")
                .started_at_epoch,
            account_barrier_before.started_at_epoch
        );
        assert_eq!(
            state.monitors[&guard_id].reset_barriers[1]
                .as_ref()
                .expect("stale lower pair retains monitor barrier")
                .started_at_epoch,
            monitor_barrier_before.started_at_epoch
        );
    }
}

#[test]
fn known_none_without_prior_provider_fields_fails_closed_for_lower_pair() {
    let global_reset = NOW + 86_400;
    let candidate = weekly_projection(NOW + 2, 1, 1, 50, Some(global_reset - 1));
    let window = projection::projection_windows(&candidate.providers[0].accounts[0], NOW + 3, 1)[1]
        .clone()
        .expect("candidate contains a complete provider pair");
    assert!(window.paired.is_some());

    let mut account = AccountObservations {
        latest_reset_epochs: [None, Some(global_reset)],
        ..AccountObservations::default()
    };
    let before = broker_window_snapshot(&account.broker_windows[1]);
    assert!(!projection::update_broker_windows(
        &mut account,
        [None, Some(window)],
        [false, true],
        [false, false],
        1,
    ));
    assert_eq!(broker_window_snapshot(&account.broker_windows[1]), before);
    assert_eq!(account.latest_reset_epochs[1], Some(global_reset));
    assert!(matches!(
        &account.statusline_reset_watermarks[1],
        StatuslineResetWatermark::KnownNone
    ));
}

#[test]
fn known_none_rejects_malformed_prior_receipt_for_lower_admission() {
    let global_reset = NOW + 86_400;
    let initial = weekly_projection(NOW + 1, 1, 1, 50, Some(global_reset));
    let initial_window =
        projection::projection_windows(&initial.providers[0].accounts[0], NOW + 2, 1)[1]
            .clone()
            .expect("initial provider projection has a complete weekly pair");
    let mut account = AccountObservations {
        broker_windows: [ObservedWindow::default(), initial_window],
        latest_reset_epochs: [None, Some(global_reset)],
        ..AccountObservations::default()
    };
    account.broker_windows[1]
        .used
        .as_mut()
        .expect("initial usage")
        .received_at_epoch = -1;
    account.broker_windows[1]
        .reset
        .as_mut()
        .expect("initial reset")
        .received_at_epoch = -1;
    account.broker_windows[1]
        .paired
        .as_mut()
        .expect("initial paired observation")
        .received_at_epoch = -1;
    let before = broker_window_snapshot(&account.broker_windows[1]);

    let lower = weekly_projection(NOW + 3, 2, 2, 40, Some(global_reset - 1));
    let lower_window = projection::projection_windows(&lower.providers[0].accounts[0], NOW + 4, 2)
        [1]
    .clone()
    .expect("lower candidate has a complete provider pair");
    assert!(!projection::update_broker_windows(
        &mut account,
        [None, Some(lower_window)],
        [false, true],
        [false, false],
        2,
    ));
    assert_eq!(broker_window_snapshot(&account.broker_windows[1]), before);
    assert_eq!(account.latest_reset_epochs[1], Some(global_reset));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One persisted lifecycle covers duplicate, pruning, restart, repair, TTL, and barrier behavior."
)]
fn pruned_statusline_reset_watermark_survives_restart_and_fences_lower_reset_repair() {
    for (high_used_percent, expected_pause) in [
        (95, MonitorIssueCode::LimitGuardReached),
        (100, MonitorIssueCode::LimitExhausted),
    ] {
        let (directory, store) = open_store();
        let account_id = format!("operator-local-pruned-reset-watermark-{high_used_percent}");
        let (binding, _) =
            start_approved_projection_observer(&store, &account_id, "acct-provider-freshness", NOW);
        let guard_id = start_provider_dispatch_guard(
            &store,
            &binding,
            &format!("pruned-{high_used_percent}"),
            NOW,
        );
        let prior_reset = NOW + 86_400;
        store
            .observe_projection(
                &weekly_projection(NOW + 3, 1, 1, high_used_percent, Some(prior_reset)),
                NOW + 3,
            )
            .expect("publish threshold-crossing provider quota");
        let barrier_before = {
            let state = store.lock();
            state.accounts[&account_id].reset_barriers[1]
                .as_ref()
                .expect("threshold latches durable reset barrier")
                .clone()
        };
        assert_eq!(barrier_before.pause_reason, expected_pause);

        let watermark = prior_reset + 10;
        let watermark_received_at = NOW + MONITOR_EVIDENCE_TTL_SECS;
        ingest_weekly_statusline(
            &store,
            &binding,
            "pruned-watermark-session",
            None,
            watermark,
            watermark_received_at,
        );
        assert_weekly_reset_watermark(&store, &account_id, watermark, watermark_received_at);

        let duplicate_received_at = watermark_received_at + 1;
        ingest_weekly_statusline(
            &store,
            &binding,
            "pruned-watermark-session",
            None,
            watermark,
            duplicate_received_at,
        );
        assert_weekly_reset_watermark(&store, &account_id, watermark, watermark_received_at);

        let prune_at = duplicate_received_at + MONITOR_EVIDENCE_TTL_SECS + 1;
        store
            .operate(
                MonitorOperation::Ingest {
                    scope: binding_scope(&binding),
                    observation: observation_with_windows(
                        "prune-trigger-empty-session",
                        None,
                        None,
                        None,
                        None,
                    ),
                },
                prune_at,
            )
            .expect("new bound callback prunes the inactive watermark session");
        {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert!(
                !account.sessions.contains_key("pruned-watermark-session"),
                "the old callback session is pruned"
            );
            assert_eq!(account.latest_reset_epochs[1], Some(watermark));
        }
        assert_weekly_reset_watermark(&store, &account_id, watermark, watermark_received_at);

        drop(store);
        let store = MonitorStore::open(directory.path()).expect("reopen pruned watermark state");
        // Collector source configuration is process-local, so restore it for
        // the post-restart provider repair in this fixture.
        store.set_experimental_collector_source(Some("acct-provider-freshness".to_owned()));
        {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert!(!account.sessions.contains_key("pruned-watermark-session"));
            assert_eq!(account.latest_reset_epochs[1], Some(watermark));
        }
        assert_weekly_reset_watermark(&store, &account_id, watermark, watermark_received_at);

        let new_session_receipt = prune_at + 1;
        ingest_weekly_statusline(
            &store,
            &binding,
            "new-session-equal-watermark",
            None,
            watermark,
            new_session_receipt,
        );
        assert_weekly_reset_watermark(&store, &account_id, watermark, new_session_receipt);

        let lower_reset = prior_reset + 5;
        let before_older_source = broker_weekly_snapshot(&store, &account_id);
        let older_source_at = new_session_receipt - 1;
        store
            .observe_projection(
                &weekly_projection(older_source_at, 2, 2, 82, Some(lower_reset)),
                new_session_receipt + 1,
            )
            .expect("reject provider source predating the latest statusline receipt");
        assert_eq!(
            broker_weekly_snapshot(&store, &account_id),
            before_older_source
        );

        let repair_source_at = new_session_receipt + 1;
        let repair_received_at = repair_source_at + 1;
        store
            .observe_projection(
                &weekly_projection(repair_source_at, 3, 3, 50, Some(lower_reset)),
                repair_received_at,
            )
            .expect("accept current complete provider pair after latest statusline receipt");
        let repaired = broker_weekly_snapshot(&store, &account_id);
        assert_eq!(repaired.used_value, Some(5_000));
        assert_eq!(repaired.used_reset, Some(lower_reset));
        assert_eq!(repaired.used_source_at, Some(repair_source_at));
        assert_eq!(repaired.used_received_at, Some(repair_received_at));
        assert_eq!(repaired.reset_value, Some(lower_reset));
        assert_eq!(repaired.reset_source_at, Some(repair_source_at));
        assert_eq!(repaired.reset_received_at, Some(repair_received_at));
        assert!(!repaired.pair_present);
        assert_weekly_reset_watermark(&store, &account_id, watermark, new_session_receipt);
        {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(account.latest_reset_epochs[1], Some(watermark));
            let source = account
                .provider_observation
                .as_ref()
                .expect("provider source remains selected");
            assert_eq!(source.source_account_id, "acct-provider-freshness");
            assert_eq!(
                source.binding_id.as_deref(),
                Some(binding.binding_id.as_str())
            );
            assert_eq!(source.binding_revision, Some(binding.revision));
        }

        drop(store);
        let reopened = MonitorStore::open(directory.path()).expect("reopen repaired quota");
        let stale_at = repair_source_at + MONITOR_EVIDENCE_TTL_SECS + 1;
        let status = descriptor_status(&reopened, &guard_id, stale_at);
        assert_eq!(status.readiness.quota, MonitorQuotaReadiness::Exhausted);
        assert_eq!(status.readiness.dispatch, MonitorDispatchReadiness::Blocked);
        assert!(!status.runnable);
        assert_eq!(status.seven_day.used_percentage_basis_points, Some(5_000));
        assert_eq!(status.seven_day.reset_at_epoch, Some(lower_reset));
        for field in [
            status.seven_day.used_evidence.as_ref(),
            status.seven_day.reset_evidence.as_ref(),
        ] {
            let field = field.expect("repaired quota remains visible after TTL");
            assert_eq!(field.evidence_received_at_epoch, repair_received_at);
            assert!(field.age_seconds > MONITOR_EVIDENCE_TTL_SECS as u64);
            assert_eq!(field.freshness, MonitorEvidenceFreshness::Stale);
        }
        assert_weekly_reset_watermark(&reopened, &account_id, watermark, new_session_receipt);
        {
            let state = reopened.lock();
            let account = &state.accounts[&account_id];
            assert_eq!(account.latest_reset_epochs[1], Some(watermark));
            assert_eq!(
                account.broker_windows[1]
                    .used
                    .as_ref()
                    .map(|field| field.value),
                Some(5_000)
            );
            assert!(account.broker_windows[1].paired.is_none());
            let barrier = account.reset_barriers[1]
                .as_ref()
                .expect("lower provider correction cannot clear reset barrier");
            assert_eq!(barrier.started_at_epoch, barrier_before.started_at_epoch);
            assert_eq!(barrier.prior_reset_at_epoch, Some(prior_reset));
            assert_eq!(
                barrier.prior_used_percentage_basis_points,
                high_used_percent * 100
            );
            assert_eq!(barrier.pause_reason, expected_pause);
        }
    }
}
