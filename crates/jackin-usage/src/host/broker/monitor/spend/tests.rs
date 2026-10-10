// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const ACCOUNT: &str = "acct-1";
const GOAL: &str = "goal-1";
const PERIOD_START: i64 = 1_000_000;
const PERIOD_END: i64 = 1_100_000;

fn amount(minor: i64) -> Money {
    Money::new(minor, "SGD", 2)
}

fn budget(minor: i64) -> Money {
    amount(minor)
}

fn receipt(
    period_start: i64,
    period_end: i64,
    amount: Money,
    evidence_at_epoch: Option<i64>,
    verified: bool,
) -> SpendRecordInput {
    SpendRecordInput {
        account_id: ACCOUNT.to_owned(),
        billing_period_start_epoch: period_start,
        billing_period_end_epoch: period_end,
        amount,
        evidence_at_epoch,
        verified,
        source: SpendRecordSource::OperatorReceipt,
    }
}

fn account_with_baseline(now: i64, baseline_minor: i64) -> SpendAccountState {
    let (account, record) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(PERIOD_START, PERIOD_END, amount(baseline_minor), None, true),
        now,
    )
    .expect("valid account spend receipt");
    assert_eq!(record.verification, SpendVerification::Verified);
    account
}

fn rolled_over_state(budget: &Money) -> (SpendAccountState, SpendState) {
    let start_time = PERIOD_START + 1_000;
    let mut account = account_with_baseline(start_time, 7_000);
    let mut state = capture_goal_baseline(&account, Some(budget), start_time);

    let (next_account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(13_000),
            Some(PERIOD_END - 10),
            true,
        ),
        PERIOD_END - 10,
    )
    .expect("in-period spend");
    account = next_account;
    advance_goal_spend(&mut state, &account, Some(budget), PERIOD_END - 5);

    let next_period_end = PERIOD_END + 100_000;
    let (next_account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_END,
            next_period_end,
            amount(200),
            Some(PERIOD_END + 1),
            true,
        ),
        PERIOD_END + 1,
    )
    .expect("next-period receipt");
    account = next_account;
    advance_goal_spend(&mut state, &account, Some(budget), PERIOD_END + 1);
    assert!(state.rollover_unknown);

    let (next_account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(14_000),
            Some(PERIOD_END + 2),
            true,
        ),
        PERIOD_END + 2,
    )
    .expect("fresh post-close total");
    account = next_account;
    advance_goal_spend(&mut state, &account, Some(budget), PERIOD_END + 3);
    assert!(state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(7_200)));
    (account, state)
}

#[test]
fn spend_state_validation_blocks_unverified_baselines_and_incompatible_cumulative_money() {
    let now = PERIOD_START + 10;
    let account = account_with_baseline(now, 1_000);
    let budget = budget(5_000);
    let valid = capture_goal_baseline(&account, Some(&budget), now);
    assert!(validate_spend_state(&valid));

    let mut unverified_baseline = valid.clone();
    unverified_baseline
        .baseline
        .as_mut()
        .expect("captured baseline")
        .verification = SpendVerification::Unverified;
    assert!(!validate_spend_state(&unverified_baseline));
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &unverified_baseline,
        now,
    );
    assert!(decision.budget_unverifiable);
    assert!(decision.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetUnverifiable,
    }));

    let mut future_evidence_baseline = valid.clone();
    future_evidence_baseline
        .baseline
        .as_mut()
        .expect("captured baseline")
        .evidence_at_epoch = Some(now + 1);
    assert!(!validate_spend_state(&future_evidence_baseline));
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &future_evidence_baseline,
        now,
    );
    assert!(decision.budget_unverifiable);

    let mut incompatible_cumulative = valid.clone();
    incompatible_cumulative.cumulative_goal_spend = Some(Money::new(0, "USD", SGD_EXPONENT));
    assert!(!validate_spend_state(&incompatible_cumulative));
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &incompatible_cumulative,
        now,
    );
    assert!(decision.budget_unverifiable);
    assert!(decision.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetUnverifiable,
    }));

    let mut future_verified_current = account.clone();
    future_verified_current
        .current_period_record
        .as_mut()
        .expect("current verified receipt")
        .evidence_at_epoch = Some(now + 1);
    assert!(!validate_account_spend_state(&future_verified_current));
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &future_verified_current,
        &valid,
        now,
    );
    assert!(decision.budget_unverifiable);

    let mut negative_historical_horizon = account.clone();
    negative_historical_horizon.historical_correction_horizon_epoch = Some(-1);
    assert!(!validate_account_spend_state(&negative_historical_horizon));

    let mut wrong_exponent = valid.clone();
    wrong_exponent.cumulative_goal_spend = Some(Money::new(0, SGD_CURRENCY, SGD_EXPONENT + 1));
    assert!(!validate_spend_state(&wrong_exponent));

    let mut negative_cumulative = valid.clone();
    negative_cumulative.cumulative_goal_spend = Some(Money::new(-1, SGD_CURRENCY, SGD_EXPONENT));
    assert!(!validate_spend_state(&negative_cumulative));

    let mut same_period_marker = valid.clone();
    same_period_marker.closed_period_anchor = same_period_marker.baseline.clone();
    assert!(!validate_spend_state(&same_period_marker));

    let mut partial_state = valid;
    partial_state.period_anchor = None;
    assert!(!validate_spend_state(&partial_state));
}

#[test]
fn closed_period_marker_must_match_goal_account_and_anchor_boundary() {
    let budget = budget(100_000);
    let (_, valid) = rolled_over_state(&budget);
    assert!(validate_spend_state(&valid));

    let mut mismatched_account = valid.clone();
    mismatched_account
        .closed_period_anchor
        .as_mut()
        .expect("closed-period marker")
        .account_id = "another-account".to_owned();
    assert!(!validate_spend_state(&mismatched_account));

    let mut noncontiguous_marker = valid;
    noncontiguous_marker
        .closed_period_anchor
        .as_mut()
        .expect("closed-period marker")
        .billing_period_end_epoch -= 1;
    assert!(!validate_spend_state(&noncontiguous_marker));
}

#[test]
fn spend_state_validation_preserves_baseline_less_historical_unknowns() {
    let now = PERIOD_START + 10;
    let account = account_with_baseline(now, 1_000);
    let budget = budget(5_000);
    let mut historical = capture_goal_baseline(&account, Some(&budget), now);
    historical.baseline = None;

    assert!(validate_spend_state(&historical));
    let decision = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &historical, now);
    assert!(decision.budget_unverifiable);
}

#[test]
fn rejects_unbound_or_malformed_receipts_and_persists_unsupported_currency_unverified() {
    let initial = SpendAccountState::default();
    assert_eq!(
        record_account_spend(
            &initial,
            ACCOUNT,
            SpendRecordInput {
                account_id: "another-account".to_owned(),
                ..receipt(PERIOD_START, PERIOD_END, amount(100), None, true)
            },
            PERIOD_START + 1,
        )
        .unwrap_err(),
        SpendRecordReject::AccountMismatch
    );
    assert_eq!(
        record_account_spend(
            &initial,
            ACCOUNT,
            receipt(PERIOD_START, PERIOD_END, amount(-1), None, true),
            PERIOD_START + 1,
        )
        .unwrap_err(),
        SpendRecordReject::NegativeAmount
    );
    assert_eq!(
        record_account_spend(
            &initial,
            ACCOUNT,
            receipt(PERIOD_END, PERIOD_START, amount(100), None, true),
            PERIOD_START + 1,
        )
        .unwrap_err(),
        SpendRecordReject::InvalidBillingPeriod
    );

    let (account, record) = record_account_spend(
        &initial,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            Money::new(10_000, "USD", 2),
            None,
            true,
        ),
        PERIOD_START + 10,
    )
    .expect("USD is syntactically valid and remains auditable");
    assert_eq!(record.verification, SpendVerification::Unverified);
    assert_eq!(account.latest_record, Some(record));
    assert!(account.current_period_record.is_none());
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget(10_000)),
        &account,
        &SpendState::default(),
        PERIOD_START + 10,
    );
    assert!(decision.budget_unverifiable);
    assert!(
        decision
            .issues
            .contains(&MonitorIssueCode::BudgetUnverifiable)
    );
    assert!(decision.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetUnverifiable
    }));
}

#[test]
fn missing_or_unverifiable_spend_with_budget_checkpoints_and_pauses() {
    let result = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget(10_000)),
        &SpendAccountState::default(),
        &SpendState::default(),
        PERIOD_START,
    );
    assert!(result.budget_unverifiable);
    assert!(
        result
            .issues
            .contains(&MonitorIssueCode::BudgetUnverifiable)
    );
    assert_eq!(
        result.actions,
        vec![
            MonitorAction::Checkpoint {
                goal_id: GOAL.to_owned()
            },
            MonitorAction::Pause {
                reason: MonitorIssueCode::BudgetUnverifiable
            },
        ]
    );
    assert_eq!(result.action_fingerprints.len(), 2);
    assert_eq!(result.action_fingerprints[0].len(), 64);

    let no_budget = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        None,
        &SpendAccountState::default(),
        &SpendState::default(),
        PERIOD_START,
    );
    assert!(no_budget.budget_unverifiable);
    assert!(
        no_budget
            .issues
            .contains(&MonitorIssueCode::BudgetUnverifiable)
    );
    assert_eq!(no_budget.actions, result.actions);
}

#[test]
fn freshness_is_bounded_at_three_hundred_seconds() {
    let received_at = PERIOD_START + 1_000;
    let (_, at_limit) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(100),
            Some(received_at - 300),
            true,
        ),
        received_at,
    )
    .expect("300-second-old evidence is accepted");
    assert_eq!(at_limit.verification, SpendVerification::Verified);

    let (account, record) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(100),
            Some(received_at - 301),
            true,
        ),
        received_at,
    )
    .expect("stale evidence is stored for status");
    assert_eq!(record.verification, SpendVerification::Stale);
    assert_eq!(account.latest_record, Some(record));

    let (_, future) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(100),
            Some(received_at + 1),
            true,
        ),
        received_at,
    )
    .expect("future evidence is stored for status");
    assert_eq!(future.verification, SpendVerification::Stale);
}

#[test]
fn verification_requires_broker_time_inside_the_billing_period() {
    let cases = [
        (PERIOD_START - 1, SpendVerification::Unverified),
        (PERIOD_START, SpendVerification::Verified),
        (PERIOD_END - 1, SpendVerification::Verified),
        (PERIOD_END, SpendVerification::Unverified),
    ];
    for (now, expected) in cases {
        let (_, record) = record_account_spend(
            &SpendAccountState::default(),
            ACCOUNT,
            receipt(PERIOD_START, PERIOD_END, amount(100), None, true),
            now,
        )
        .expect("period boundary receipt remains auditable");
        assert_eq!(record.verification, expected, "now={now}");
    }
}

#[test]
fn a_future_billing_period_cannot_become_a_goal_baseline() {
    let now = PERIOD_START - 100;
    let (account, record) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(PERIOD_START, PERIOD_END, amount(100), None, true),
        now,
    )
    .expect("future-period receipt remains auditable");
    assert_eq!(record.verification, SpendVerification::Unverified);
    assert!(
        capture_goal_baseline(&account, Some(&budget(10_000)), now)
            .baseline
            .is_none()
    );
}

#[test]
fn cumulative_goal_spend_counts_only_increases_after_start_baseline() {
    let start_time = PERIOD_START + 1_000;
    let account = account_with_baseline(start_time, 7_000);
    let budget = budget(10_000);
    let mut state = capture_goal_baseline(&account, Some(&budget), start_time);
    assert_eq!(state.cumulative_goal_spend, Some(amount(0)));

    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(11_000),
            Some(start_time + 10),
            true,
        ),
        start_time + 10,
    )
    .expect("same-period spend update");
    advance_goal_spend(&mut state, &account, Some(&budget), start_time + 10);
    assert_eq!(state.cumulative_goal_spend, Some(amount(4_000)));
    assert_eq!(
        evaluate_spend_policy(
            ACCOUNT,
            GOAL,
            Some(&budget),
            &account,
            &state,
            start_time + 10,
        )
        .actions,
        vec![MonitorAction::Warn {
            reason: MonitorIssueCode::BudgetWarn
        }]
    );
}

#[test]
fn lower_same_period_total_never_reduces_cumulative_spend() {
    let start_time = PERIOD_START + 1_000;
    let account = account_with_baseline(start_time, 7_000);
    let budget = budget(10_000);
    let mut state = capture_goal_baseline(&account, Some(&budget), start_time);
    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(11_000),
            Some(start_time + 1),
            true,
        ),
        start_time + 1,
    )
    .expect("increase");
    advance_goal_spend(&mut state, &account, Some(&budget), start_time + 1);
    assert_eq!(state.cumulative_goal_spend, Some(amount(4_000)));

    let (account, lower_record) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(9_000),
            Some(start_time + 2),
            true,
        ),
        start_time + 2,
    )
    .expect("lower total remains auditable");
    assert_eq!(lower_record.verification, SpendVerification::Unverified);
    advance_goal_spend(&mut state, &account, Some(&budget), start_time + 2);
    assert_eq!(state.cumulative_goal_spend, Some(amount(4_000)));
    let result = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &state,
        start_time + 2,
    );
    assert!(result.budget_unverifiable);
    assert!(
        result
            .issues
            .contains(&MonitorIssueCode::BudgetUnverifiable)
    );
}

#[test]
fn rollover_requires_a_fresh_verified_old_total_and_contiguous_next_period() {
    let start_time = PERIOD_START + 1_000;
    let budget = budget(100_000);
    let account = account_with_baseline(start_time, 7_000);
    let mut state = capture_goal_baseline(&account, Some(&budget), start_time);

    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(13_000),
            Some(PERIOD_END - 10),
            true,
        ),
        PERIOD_END - 10,
    )
    .expect("in-period spend");
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END - 5);
    assert_eq!(state.cumulative_goal_spend, Some(amount(6_000)));

    let next_start = PERIOD_END;
    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            next_start,
            next_start + 100_000,
            amount(200),
            Some(next_start + 1),
            true,
        ),
        next_start + 1,
    )
    .expect("next-period receipt");
    advance_goal_spend(&mut state, &account, Some(&budget), next_start + 1);
    assert!(state.rollover_unknown);
    assert!(!state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(6_000)));

    let previous_latest = account.latest_record.clone();
    let previous_current = account.current_period_record.clone();
    let (account, close_record) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(14_000),
            Some(PERIOD_END + 2),
            true,
        ),
        PERIOD_END + 2,
    )
    .expect("fresh post-close old-period total");
    assert_eq!(close_record.verification, SpendVerification::Verified);
    assert_eq!(account.previous_period_record, Some(close_record));
    assert_eq!(account.current_period_record, previous_current);
    assert_eq!(account.latest_record, previous_latest);

    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 3);
    assert!(!state.rollover_unknown);
    assert!(state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(7_200)));
}

#[test]
fn post_rollover_closed_period_corrections_are_counted_once_across_restart() {
    let budget = budget(10_000);
    let (account, mut state) = rolled_over_state(&budget);
    let baseline = state.baseline.clone();
    assert_eq!(
        state
            .closed_period_anchor
            .as_ref()
            .map(|record| record.amount.amount_minor),
        Some(14_000)
    );

    let (account, correction) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(16_000),
            Some(PERIOD_END + 4),
            true,
        ),
        PERIOD_END + 4,
    )
    .expect("fresh prior-period correction");
    assert_eq!(correction.verification, SpendVerification::Verified);
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 5);
    assert_eq!(state.cumulative_goal_spend, Some(amount(9_200)));
    assert_eq!(
        state
            .closed_period_anchor
            .as_ref()
            .map(|record| record.amount.amount_minor),
        Some(16_000)
    );
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &state,
        PERIOD_END + 5,
    );
    assert!(decision.actions.contains(&MonitorAction::ReduceDispatch {
        max_parallel: Some(0),
    }));

    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 6);
    assert_eq!(state.cumulative_goal_spend, Some(amount(9_200)));

    let persisted = serde_json::to_value(&state).expect("serialize goal spend state");
    let mut state: SpendState =
        serde_json::from_value(persisted).expect("restore goal spend state");
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 7);
    assert_eq!(state.cumulative_goal_spend, Some(amount(9_200)));

    let (account, correction) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(18_000),
            Some(PERIOD_END + 8),
            true,
        ),
        PERIOD_END + 8,
    )
    .expect("second fresh prior-period correction");
    assert_eq!(correction.verification, SpendVerification::Verified);
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 9);
    assert_eq!(state.cumulative_goal_spend, Some(amount(11_200)));
    assert_eq!(state.baseline, baseline);
}

#[test]
fn historical_rollover_without_closed_marker_fails_closed_after_deserialization() {
    let budget = budget(10_000);
    let (account, state) = rolled_over_state(&budget);
    let mut persisted = serde_json::to_value(&state).expect("serialize goal spend state");
    persisted
        .as_object_mut()
        .expect("state object")
        .remove("closed_period_anchor");
    let mut legacy_state: SpendState =
        serde_json::from_value(persisted).expect("deserialize prior state shape");
    assert!(validate_spend_state(&legacy_state));
    assert!(legacy_state.closed_period_anchor.is_none());

    advance_goal_spend(&mut legacy_state, &account, Some(&budget), PERIOD_END + 4);
    assert_eq!(legacy_state.cumulative_goal_spend, Some(amount(7_200)));
    assert!(!legacy_state.cumulative_complete);
    assert!(legacy_state.rollover_unknown);
    let decision = evaluate_spend_policy(
        ACCOUNT,
        GOAL,
        Some(&budget),
        &account,
        &legacy_state,
        PERIOD_END + 4,
    );
    assert!(decision.budget_unverifiable);
    assert!(decision.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetUnverifiable,
    }));
}

#[test]
fn correction_older_than_retained_previous_period_is_unverified() {
    let budget = budget(100_000);
    let (account, mut state) = rolled_over_state(&budget);
    let prior_period_record = account.previous_period_record.clone();
    let received_at = PERIOD_END + 4;
    let (account, correction) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START - 100_000,
            PERIOD_START,
            amount(500),
            Some(received_at),
            true,
        ),
        received_at,
    )
    .expect("unretained old period stays auditable");
    assert_eq!(correction.verification, SpendVerification::Unverified);
    assert_eq!(account.previous_period_record, prior_period_record);
    assert_eq!(
        account.historical_correction_horizon_epoch,
        Some(PERIOD_START)
    );

    // The correction is for a period that ended exactly when this goal's
    // baseline period began, so it cannot affect this goal's cumulative total.
    advance_goal_spend(&mut state, &account, Some(&budget), received_at);
    assert!(state.cumulative_complete);
    assert!(!state.rollover_unknown);
}

struct TwoRolloverSpendState {
    account: SpendAccountState,
    state: SpendState,
    later_goal_state: SpendState,
    latest_p2: Option<SpendRecord>,
    p0_start: i64,
    p0_end: i64,
    p2_start: i64,
}

fn spend_state_after_two_rollovers(budget: &Money) -> TwoRolloverSpendState {
    let p0_start = PERIOD_START;
    let p0_end = p0_start + 100;
    let p1_start = p0_end;
    let p1_end = p1_start + 100;
    let p2_start = p1_end;
    let baseline_at = p0_start + 1;

    let (mut account, _) = record_account_spend(
        &SpendAccountState::default(),
        ACCOUNT,
        receipt(p0_start, p0_end, amount(7_000), None, true),
        baseline_at,
    )
    .expect("fresh p0 baseline");
    let mut state = capture_goal_baseline(&account, Some(budget), baseline_at);
    assert!(state.cumulative_complete);

    // Fold p0 spend, then prove the first rollover with a post-close p0 total.
    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p0_start, p0_end, amount(13_000), Some(p0_end - 1), true),
        p0_end - 1,
    )
    .expect("fresh p0 total");
    advance_goal_spend(&mut state, &account, Some(budget), p0_end - 1);

    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p1_start, p1_end, amount(200), Some(p1_start + 1), true),
        p1_start + 1,
    )
    .expect("fresh p1 opening total");
    advance_goal_spend(&mut state, &account, Some(budget), p1_start + 1);
    assert!(state.rollover_unknown);

    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p0_start, p0_end, amount(14_000), Some(p0_end + 2), true),
        p0_end + 2,
    )
    .expect("fresh p0 closing total");
    advance_goal_spend(&mut state, &account, Some(budget), p0_end + 3);
    assert!(state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(7_200)));

    // Close p1 and open p2. At this point p0 has aged out of the account's
    // two retained period slots, while the goal still includes its spend.
    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p1_start, p1_end, amount(1_000), Some(p1_end - 1), true),
        p1_end - 1,
    )
    .expect("fresh p1 total");
    advance_goal_spend(&mut state, &account, Some(budget), p1_end - 1);

    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p1_start, p1_end, amount(1_200), Some(p1_end + 1), true),
        p1_end + 1,
    )
    .expect("fresh p1 closing total");
    advance_goal_spend(&mut state, &account, Some(budget), p1_end + 2);

    (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            p2_start,
            p2_start + 10_000,
            amount(300),
            Some(p2_start + 1),
            true,
        ),
        p2_start + 1,
    )
    .expect("fresh p2 opening total");
    advance_goal_spend(&mut state, &account, Some(budget), p2_start + 1);
    assert!(state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(8_500)));
    let later_goal_state = capture_goal_baseline(&account, Some(budget), p2_start + 1);
    assert!(later_goal_state.cumulative_complete);
    let latest_p2 = account.latest_record.clone();

    TwoRolloverSpendState {
        account,
        state,
        later_goal_state,
        latest_p2,
        p0_start,
        p0_end,
        p2_start,
    }
}

#[test]
fn unretained_correction_after_two_rollovers_latches_only_affected_goal() {
    let budget = budget(100_000);
    let TwoRolloverSpendState {
        account,
        mut state,
        mut later_goal_state,
        latest_p2,
        p0_start,
        p0_end,
        p2_start,
    } = spend_state_after_two_rollovers(&budget);

    let (account, correction) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(p0_start, p0_end, amount(16_000), Some(p2_start + 2), true),
        p2_start + 2,
    )
    .expect("old correction remains auditable");
    assert_eq!(correction.verification, SpendVerification::Unverified);
    assert_eq!(account.latest_record, latest_p2);
    assert_eq!(account.historical_correction_horizon_epoch, Some(p0_end));

    let (account, earlier_correction) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            p0_start - 100,
            p0_start,
            amount(500),
            Some(p2_start + 3),
            true,
        ),
        p2_start + 3,
    )
    .expect("even older correction remains auditable");
    assert_eq!(
        earlier_correction.verification,
        SpendVerification::Unverified
    );
    assert_eq!(account.historical_correction_horizon_epoch, Some(p0_end));

    advance_goal_spend(&mut state, &account, Some(&budget), p2_start + 3);
    assert_eq!(state.cumulative_goal_spend, Some(amount(8_500)));
    assert!(!state.cumulative_complete);
    assert!(state.rollover_unknown);
    advance_goal_spend(&mut later_goal_state, &account, Some(&budget), p2_start + 3);
    assert!(later_goal_state.cumulative_complete);
    assert!(!later_goal_state.rollover_unknown);
    let decision =
        evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, p2_start + 3);
    assert!(decision.budget_unverifiable);
    assert!(decision.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetUnverifiable,
    }));

    // Both the account horizon and the goal latch survive storage and newer
    // fresh p2 observations; the known estimate remains visible.
    let mut account: SpendAccountState = serde_json::from_value(
        serde_json::to_value(&account).expect("serialize account spend state"),
    )
    .expect("restore account spend state");
    let mut state: SpendState = serde_json::from_value(
        serde_json::to_value(&state).expect("serialize latched goal spend state"),
    )
    .expect("restore latched goal spend state");
    let (next_account, new_current) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            p2_start,
            p2_start + 10_000,
            amount(400),
            Some(p2_start + 3),
            true,
        ),
        p2_start + 3,
    )
    .expect("fresh newer p2 total");
    assert_eq!(new_current.verification, SpendVerification::Verified);
    account = next_account;
    advance_goal_spend(&mut state, &account, Some(&budget), p2_start + 3);
    assert_eq!(state.cumulative_goal_spend, Some(amount(8_500)));
    assert!(!state.cumulative_complete);
    assert!(state.rollover_unknown);

    let mut old_v3 =
        serde_json::to_value(SpendAccountState::default()).expect("serialize new account state");
    old_v3
        .as_object_mut()
        .expect("account state object")
        .remove("historical_correction_horizon_epoch");
    let migrated: SpendAccountState =
        serde_json::from_value(old_v3).expect("restore existing V3 account state");
    assert_eq!(migrated.historical_correction_horizon_epoch, None);
}

#[test]
fn stale_ending_total_keeps_rollover_unknown_until_fresh_post_close_receipt() {
    let start_time = PERIOD_START + 1_000;
    let budget = budget(100_000);
    let account = account_with_baseline(start_time, 7_000);
    let mut state = capture_goal_baseline(&account, Some(&budget), start_time);

    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(13_000),
            Some(PERIOD_END - 400),
            true,
        ),
        PERIOD_END - 400,
    )
    .expect("in-period total accepted when received");
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END - 400);
    assert_eq!(state.cumulative_goal_spend, Some(amount(6_000)));

    let next_start = PERIOD_END;
    let (account, _) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            next_start,
            next_start + 100_000,
            amount(200),
            Some(next_start + 1),
            true,
        ),
        next_start + 1,
    )
    .expect("next-period total");
    advance_goal_spend(&mut state, &account, Some(&budget), next_start + 1);
    assert!(state.rollover_unknown);
    assert!(!state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(6_000)));

    let previous_latest = account.latest_record.clone();
    let previous_current = account.current_period_record.clone();
    let (account, closing_record) = record_account_spend(
        &account,
        ACCOUNT,
        receipt(
            PERIOD_START,
            PERIOD_END,
            amount(14_000),
            Some(PERIOD_END + 2),
            true,
        ),
        PERIOD_END + 2,
    )
    .expect("fresh post-close total remains verifiable");
    assert_eq!(closing_record.verification, SpendVerification::Verified);
    assert_eq!(account.current_period_record, previous_current);
    assert_eq!(account.latest_record, previous_latest);
    advance_goal_spend(&mut state, &account, Some(&budget), PERIOD_END + 3);
    assert!(!state.rollover_unknown);
    assert!(state.cumulative_complete);
    assert_eq!(state.cumulative_goal_spend, Some(amount(7_200)));
}

#[test]
fn threshold_boundaries_are_exact_and_fingerprints_are_stable() {
    let now = PERIOD_START + 1_000;
    let budget = budget(10_000);
    let evaluate_at = |cumulative: i64, evaluated_at: i64| {
        let baseline_account = account_with_baseline(now, 1_000);
        let mut state = capture_goal_baseline(&baseline_account, Some(&budget), now);
        let (account, record) = record_account_spend(
            &baseline_account,
            ACCOUNT,
            receipt(
                PERIOD_START,
                PERIOD_END,
                amount(1_000 + cumulative),
                Some(now),
                true,
            ),
            now,
        )
        .expect("verified total matching the threshold fixture");
        assert_eq!(record.verification, SpendVerification::Verified);
        advance_goal_spend(&mut state, &account, Some(&budget), now);
        evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, evaluated_at)
    };

    let below = evaluate_at(3_999, now);
    assert!(below.actions.is_empty());

    let warn = evaluate_at(4_000, now);
    assert_eq!(warn.actions.len(), 1);
    assert!(warn.issues.contains(&MonitorIssueCode::BudgetWarn));
    let repeated = evaluate_at(4_000, now + 1);
    assert_eq!(warn.action_fingerprints, repeated.action_fingerprints);

    let below_checkpoint = evaluate_at(4_499, now);
    assert_eq!(below_checkpoint.actions, warn.actions);

    let checkpoint = evaluate_at(4_500, now);
    assert!(checkpoint.actions.contains(&MonitorAction::Checkpoint {
        goal_id: GOAL.to_owned()
    }));
    assert!(checkpoint.actions.contains(&MonitorAction::ReduceDispatch {
        max_parallel: Some(0)
    }));

    let below_pause = evaluate_at(4_799, now);
    assert!(!below_pause.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetPause
    }));

    let pause = evaluate_at(4_800, now);
    assert!(pause.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetPause
    }));
    assert!(!pause.budget_cap_reached);

    let cap = evaluate_at(5_000, now);
    assert!(cap.budget_cap_reached);
    assert!(cap.actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetPause
    }));
}

#[test]
fn lower_custom_budget_caps_each_threshold_proportionally() {
    let now = PERIOD_START + 1_000;
    let budget = budget(3_000);
    let evaluate_at = |cumulative: i64| {
        let baseline_account = account_with_baseline(now, 1_000);
        let mut state = capture_goal_baseline(&baseline_account, Some(&budget), now);
        let (account, record) = record_account_spend(
            &baseline_account,
            ACCOUNT,
            receipt(
                PERIOD_START,
                PERIOD_END,
                amount(1_000 + cumulative),
                Some(now),
                true,
            ),
            now,
        )
        .expect("verified total matching the threshold fixture");
        assert_eq!(record.verification, SpendVerification::Verified);
        advance_goal_spend(&mut state, &account, Some(&budget), now);
        evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
    };

    assert!(evaluate_at(2_399).actions.is_empty());
    assert!(evaluate_at(2_400).actions.contains(&MonitorAction::Warn {
        reason: MonitorIssueCode::BudgetWarn
    }));

    assert!(
        !evaluate_at(2_699)
            .actions
            .contains(&MonitorAction::Checkpoint {
                goal_id: GOAL.to_owned()
            })
    );
    let stop = evaluate_at(2_700);
    assert!(stop.actions.contains(&MonitorAction::Checkpoint {
        goal_id: GOAL.to_owned()
    }));
    assert!(stop.actions.contains(&MonitorAction::ReduceDispatch {
        max_parallel: Some(0)
    }));

    assert!(!evaluate_at(2_879).actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetPause
    }));
    assert!(evaluate_at(2_880).actions.contains(&MonitorAction::Pause {
        reason: MonitorIssueCode::BudgetPause
    }));

    assert!(evaluate_at(3_000).budget_cap_reached);
}
