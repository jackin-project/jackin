// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account-level spend receipt validation and per-goal budget policy.
//!
//! Spend receipts describe an account's total for one billing period. A goal
//! captures that total as its baseline when the monitor starts, then counts
//! later increases against the goal. Session usage is deliberately not an
//! input to this module.

use jackin_protocol::{
    control::Money,
    usage_monitor::{
        MonitorAction, MonitorIssueCode, SpendRecord, SpendRecordInput, SpendRecordSource,
        SpendVerification,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SPEND_FRESHNESS_SECONDS: i64 = 300;
const SGD_CURRENCY: &str = "SGD";
const SGD_EXPONENT: u8 = 2;

/// Account-wide spend observations. The account is selected by the caller and
/// stored under its canonical account ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SpendAccountState {
    /// Most recently accepted receipt, including stale or unverified receipts.
    pub latest_record: Option<SpendRecord>,
    /// Latest verified receipt for the current billing period.
    pub current_period_record: Option<SpendRecord>,
    /// Latest verified receipt for the immediately preceding billing period.
    pub previous_period_record: Option<SpendRecord>,
}

/// Per-monitor spend state. `baseline` is the account total captured at Start;
/// cumulative spend counts only increases after that baseline.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SpendState {
    /// Fresh verified account total captured when this monitor started.
    pub baseline: Option<SpendRecord>,
    /// Latest account total already folded into cumulative goal spend.
    pub period_anchor: Option<SpendRecord>,
    /// Cumulative goal spend across billing periods, when known.
    pub cumulative_goal_spend: Option<Money>,
    /// A closed-period transition could not yet be proven.
    pub rollover_unknown: bool,
    /// False when any billing-period amount may be missing from the total.
    pub cumulative_complete: bool,
}

/// Result of evaluating account spend against one monitor's budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpendDecision {
    /// Current account-record verification as of this evaluation.
    pub verification: SpendVerification,
    /// Whether budget enforcement lacks a safe spend comparison.
    pub budget_unverifiable: bool,
    /// Whether cumulative goal spend reached the final capped threshold.
    pub budget_cap_reached: bool,
    /// Whether a billing-period rollover still needs verified evidence.
    pub rollover_unknown: bool,
    /// Issues selected by spend policy, in stable order.
    pub issues: Vec<MonitorIssueCode>,
    /// Ordered actions selected by spend policy.
    pub actions: Vec<MonitorAction>,
    /// Stable idempotency fingerprint aligned with each action.
    pub action_fingerprints: Vec<String>,
}

/// A malformed spend operation that must not be persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpendRecordReject {
    EmptyAccount,
    AccountMismatch,
    InvalidBillingPeriod,
    NegativeAmount,
    UnsupportedSource,
}

/// Accept a syntactically valid account spend receipt and update the account's
/// verified period records. Currency, explicit attestation, and freshness are
/// safety-verification inputs: unsupported currency is persisted as
/// `Unverified` so budget policy can fail closed.
pub(crate) fn record_account_spend(
    account: &SpendAccountState,
    monitored_account_id: &str,
    input: SpendRecordInput,
    received_at_epoch: i64,
) -> Result<(SpendAccountState, SpendRecord), SpendRecordReject> {
    if monitored_account_id.trim().is_empty() {
        return Err(SpendRecordReject::EmptyAccount);
    }
    if input.account_id.trim().is_empty() {
        return Err(SpendRecordReject::EmptyAccount);
    }
    if input.account_id != monitored_account_id {
        return Err(SpendRecordReject::AccountMismatch);
    }
    if input.billing_period_start_epoch < 0
        || input.billing_period_end_epoch <= input.billing_period_start_epoch
    {
        return Err(SpendRecordReject::InvalidBillingPeriod);
    }
    if input.amount.amount_minor < 0 {
        return Err(SpendRecordReject::NegativeAmount);
    }
    if !matches!(input.source, SpendRecordSource::OperatorReceipt) {
        return Err(SpendRecordReject::UnsupportedSource);
    }

    let mut record = SpendRecord {
        account_id: input.account_id,
        billing_period_start_epoch: input.billing_period_start_epoch,
        billing_period_end_epoch: input.billing_period_end_epoch,
        amount: input.amount,
        evidence_at_epoch: input.evidence_at_epoch,
        evidence_received_at_epoch: received_at_epoch,
        source: input.source,
        verification: SpendVerification::Unverified,
    };

    let period_is_active = received_at_epoch >= record.billing_period_start_epoch
        && received_at_epoch < record.billing_period_end_epoch
        && evidence_time(&record) >= record.billing_period_start_epoch;
    let closed_period_receipt = received_at_epoch >= record.billing_period_end_epoch
        && record
            .evidence_at_epoch
            .is_some_and(|evidence_at| evidence_at >= record.billing_period_end_epoch);
    let period_is_verifiable = period_is_active || closed_period_receipt;
    if input.verified
        && is_supported_sgd(&record.amount)
        && period_is_verifiable
        && record_is_fresh(&record, received_at_epoch)
    {
        record.verification = SpendVerification::Verified;
    } else if input.verified && is_supported_sgd(&record.amount) && period_is_verifiable {
        record.verification = SpendVerification::Stale;
    }

    let mut next = account.clone();
    let mut update_latest = !closed_period_receipt;
    if record.verification == SpendVerification::Verified {
        let mut applied = false;
        if closed_period_receipt {
            (applied, update_latest) = apply_closed_period_record(&mut next, &mut record);
        } else {
            match next.current_period_record.as_ref() {
                None => {
                    next.current_period_record = Some(record.clone());
                    applied = true;
                }
                Some(current) if same_period(current, &record) => {
                    if record.amount.amount_minor < current.amount.amount_minor {
                        // Account totals cannot fall within one billing period;
                        // do not let a correction reduce cumulative spend.
                        record.verification = SpendVerification::Unverified;
                    } else {
                        next.current_period_record = Some(record.clone());
                        applied = true;
                    }
                }
                Some(current) => {
                    let previous = next
                        .previous_period_record
                        .as_ref()
                        .filter(|previous| same_period(previous, &record));
                    match classify_period_transition(&record, current, previous) {
                        PeriodTransition::Decreased => {
                            record.verification = SpendVerification::Unverified;
                        }
                        PeriodTransition::UpdatePrevious => {
                            next.previous_period_record = Some(record.clone());
                            applied = true;
                        }
                        PeriodTransition::StartNextPeriod => {
                            next.previous_period_record = Some(current.clone());
                            next.current_period_record = Some(record.clone());
                            applied = true;
                        }
                        PeriodTransition::Invalid => {
                            // Overlapping or out-of-order periods cannot replace
                            // the current account total safely.
                            record.verification = SpendVerification::Unverified;
                        }
                    }
                }
            }
        }
        if !applied && record.verification == SpendVerification::Verified {
            record.verification = SpendVerification::Unverified;
        }
    }
    if update_latest {
        next.latest_record = Some(record.clone());
    }
    Ok((next, record))
}

/// Reconcile a post-close total without replacing the current account total
/// when the evidence belongs to the preceding period.
fn apply_closed_period_record(
    state: &mut SpendAccountState,
    record: &mut SpendRecord,
) -> (bool, bool) {
    let mut applied = false;
    let mut update_latest = false;
    if let Some(previous) = state
        .previous_period_record
        .as_ref()
        .filter(|previous| same_period(previous, record))
    {
        if record.amount.amount_minor < previous.amount.amount_minor {
            record.verification = SpendVerification::Unverified;
        } else {
            state.previous_period_record = Some(record.clone());
            applied = true;
        }
    }
    if !applied
        && let Some(current) = state.current_period_record.as_ref()
        && same_period(current, record)
    {
        if record.amount.amount_minor < current.amount.amount_minor {
            record.verification = SpendVerification::Unverified;
        } else {
            state.current_period_record = Some(record.clone());
            applied = true;
            update_latest = true;
        }
    }
    (applied, update_latest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeriodTransition {
    Decreased,
    UpdatePrevious,
    StartNextPeriod,
    Invalid,
}

/// Classify a current-period receipt against the current and immediately
/// previous period totals before changing either anchor.
fn classify_period_transition(
    record: &SpendRecord,
    current: &SpendRecord,
    previous: Option<&SpendRecord>,
) -> PeriodTransition {
    match previous.filter(|previous| same_period(previous, record)) {
        Some(previous) if record.amount.amount_minor < previous.amount.amount_minor => {
            PeriodTransition::Decreased
        }
        Some(_) => PeriodTransition::UpdatePrevious,
        None if record.billing_period_start_epoch >= current.billing_period_end_epoch => {
            PeriodTransition::StartNextPeriod
        }
        None => PeriodTransition::Invalid,
    }
}

/// Capture a per-monitor baseline from the account's latest current-period
/// total. If no fresh, verified, compatible record exists, the returned state
/// has no baseline and spend remains unverifiable; it is never initialized to
/// zero implicitly.
pub(crate) fn capture_goal_baseline(
    account: &SpendAccountState,
    budget: Option<&Money>,
    now_epoch: i64,
) -> SpendState {
    let (Some(latest), Some(current)) = (
        account.latest_record.as_ref(),
        account.current_period_record.as_ref(),
    ) else {
        return SpendState::default();
    };
    let latest_is_current_or_previous = same_period(latest, current)
        || account
            .previous_period_record
            .as_ref()
            .is_some_and(|previous| same_period(latest, previous));
    if latest.verification != SpendVerification::Verified
        || !latest_is_current_or_previous
        || !record_is_fresh(latest, now_epoch)
        || !record_is_fresh(current, now_epoch)
        || !money_matches_budget(&current.amount, budget)
        || now_epoch < current.billing_period_start_epoch
        || now_epoch >= current.billing_period_end_epoch
    {
        return SpendState::default();
    }

    SpendState {
        baseline: Some(current.clone()),
        period_anchor: Some(current.clone()),
        cumulative_goal_spend: Some(Money::new(0, SGD_CURRENCY, SGD_EXPONENT)),
        rollover_unknown: false,
        cumulative_complete: true,
    }
}

/// Fold the latest verified account total into one monitor's cumulative spend.
/// Call after recording an account receipt and during reconciliation so a
/// closed period can be recognized. Repeated calls over the same evidence are
/// idempotent.
pub(crate) fn advance_goal_spend(
    state: &mut SpendState,
    account: &SpendAccountState,
    budget: Option<&Money>,
    now_epoch: i64,
) {
    if state.baseline.is_none() {
        return;
    }

    let (Some(current), Some(anchor), Some(cumulative)) = (
        account.current_period_record.as_ref(),
        state.period_anchor.as_ref(),
        state.cumulative_goal_spend.as_ref(),
    ) else {
        state.rollover_unknown = true;
        return;
    };

    if !money_matches_budget(&current.amount, budget)
        || current.verification != SpendVerification::Verified
        || !record_is_fresh(current, now_epoch)
    {
        return;
    }

    if same_period(anchor, current) {
        if current.amount.amount_minor < anchor.amount.amount_minor {
            state.rollover_unknown = true;
            state.cumulative_complete = false;
            return;
        }
        let increase = current.amount.amount_minor - anchor.amount.amount_minor;
        if let Some(updated) = add_minor(cumulative, increase) {
            state.cumulative_goal_spend = Some(updated);
            state.period_anchor = Some(current.clone());
        } else {
            state.rollover_unknown = true;
            state.cumulative_complete = false;
            return;
        }
        if now_epoch >= current.billing_period_end_epoch {
            // A closed current period still needs a verified next-period total.
            state.rollover_unknown = true;
        }
        return;
    }

    let Some(closed) = account.previous_period_record.as_ref() else {
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    };
    let rollover_is_proven = same_period(anchor, closed)
        && closed.verification == SpendVerification::Verified
        && evidence_time(closed) >= closed.billing_period_end_epoch
        && record_is_fresh(closed, now_epoch)
        && closed.amount.currency == anchor.amount.currency
        && closed.amount.exponent == anchor.amount.exponent
        && closed.amount.amount_minor >= anchor.amount.amount_minor
        && current.billing_period_start_epoch == closed.billing_period_end_epoch
        && now_epoch >= closed.billing_period_end_epoch;
    if !rollover_is_proven {
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    }

    let closed_increase = closed.amount.amount_minor - anchor.amount.amount_minor;
    let Some(after_closed_period) = add_minor(cumulative, closed_increase) else {
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    };
    let Some(after_rollover) = add_minor(&after_closed_period, current.amount.amount_minor) else {
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    };

    state.cumulative_goal_spend = Some(after_rollover);
    state.period_anchor = Some(current.clone());
    state.rollover_unknown = false;
    state.cumulative_complete = true;
}

/// Evaluate spend thresholds for one goal and produce stable action
/// fingerprints. The caller should call `advance_goal_spend` first.
pub(crate) fn evaluate_spend_policy(
    account_id: &str,
    goal_id: &str,
    budget: Option<&Money>,
    account: &SpendAccountState,
    state: &SpendState,
    now_epoch: i64,
) -> SpendDecision {
    let latest = account.latest_record.as_ref();
    let mut verification =
        latest.map_or(SpendVerification::Unavailable, |record| record.verification);
    if verification == SpendVerification::Verified
        && latest.is_some_and(|record| !record_is_fresh(record, now_epoch))
    {
        verification = SpendVerification::Stale;
    }

    let rollover_unknown = state.rollover_unknown
        || (state.baseline.is_some()
            && (!state.cumulative_complete
                || state
                    .period_anchor
                    .as_ref()
                    .is_some_and(|anchor| now_epoch >= anchor.billing_period_end_epoch)));
    let baseline_is_compatible = state
        .baseline
        .as_ref()
        .is_some_and(|baseline| money_matches_budget(&baseline.amount, budget));
    let current_is_compatible = account
        .current_period_record
        .as_ref()
        .is_some_and(|current| money_matches_budget(&current.amount, budget));
    let budget_is_valid = budget.is_some_and(valid_budget);
    let budget_unverifiable = !budget_is_valid
        || verification != SpendVerification::Verified
        || !baseline_is_compatible
        || !current_is_compatible
        || state.cumulative_goal_spend.is_none()
        || !state.cumulative_complete
        || rollover_unknown;

    let mut issues = Vec::new();
    if verification == SpendVerification::Unavailable || state.baseline.is_none() {
        push_issue(&mut issues, MonitorIssueCode::SpendUnavailable);
    } else if verification == SpendVerification::Stale {
        push_issue(&mut issues, MonitorIssueCode::SpendStale);
    } else if verification != SpendVerification::Verified
        || !baseline_is_compatible
        || !current_is_compatible
    {
        push_issue(&mut issues, MonitorIssueCode::SpendUnverified);
    }
    if rollover_unknown {
        push_issue(&mut issues, MonitorIssueCode::SpendRolloverUnverified);
    }
    if budget_unverifiable {
        push_issue(&mut issues, MonitorIssueCode::BudgetUnverifiable);
    }

    let mut actions = Vec::new();
    if budget_unverifiable {
        actions.push(MonitorAction::Checkpoint {
            goal_id: goal_id.to_owned(),
        });
        actions.push(MonitorAction::Pause {
            reason: MonitorIssueCode::BudgetUnverifiable,
        });
    } else if let (Some(budget), Some(cumulative)) = (budget, state.cumulative_goal_spend.as_ref())
    {
        if reaches_capped_threshold(cumulative.amount_minor, budget.amount_minor, 4_000, 80) {
            push_issue(&mut issues, MonitorIssueCode::BudgetWarn);
            actions.push(MonitorAction::Warn {
                reason: MonitorIssueCode::BudgetWarn,
            });
        }
        if reaches_capped_threshold(cumulative.amount_minor, budget.amount_minor, 4_500, 90) {
            push_issue(&mut issues, MonitorIssueCode::BudgetCheckpoint);
            actions.push(MonitorAction::Checkpoint {
                goal_id: goal_id.to_owned(),
            });
            actions.push(MonitorAction::ReduceDispatch {
                max_parallel: Some(0),
            });
        }
        if reaches_capped_threshold(cumulative.amount_minor, budget.amount_minor, 4_800, 96) {
            push_issue(&mut issues, MonitorIssueCode::BudgetPause);
            actions.push(MonitorAction::Pause {
                reason: MonitorIssueCode::BudgetPause,
            });
        }
    }

    let budget_cap_reached = !budget_unverifiable
        && matches!((budget, state.cumulative_goal_spend.as_ref()), (Some(budget), Some(spend))
            if reaches_capped_threshold(spend.amount_minor, budget.amount_minor, 5_000, 100));

    let action_fingerprints = actions
        .iter()
        .map(|action| action_fingerprint(action, account_id, goal_id, state, budget_unverifiable))
        .collect();
    SpendDecision {
        verification,
        budget_unverifiable,
        budget_cap_reached,
        rollover_unknown,
        issues,
        actions,
        action_fingerprints,
    }
}

fn valid_budget(budget: &Money) -> bool {
    is_supported_sgd(budget) && budget.amount_minor > 0
}

fn is_supported_sgd(amount: &Money) -> bool {
    amount.currency == SGD_CURRENCY && amount.exponent == SGD_EXPONENT
}

fn money_matches_budget(amount: &Money, budget: Option<&Money>) -> bool {
    if !is_supported_sgd(amount) {
        return false;
    }
    budget.is_none_or(|budget| {
        valid_budget(budget)
            && amount.currency == budget.currency
            && amount.exponent == budget.exponent
    })
}

fn record_is_fresh(record: &SpendRecord, now_epoch: i64) -> bool {
    let evidence_at = evidence_time(record);
    let age = i128::from(now_epoch) - i128::from(evidence_at);
    record.evidence_received_at_epoch <= now_epoch
        && (0..=i128::from(SPEND_FRESHNESS_SECONDS)).contains(&age)
}

fn evidence_time(record: &SpendRecord) -> i64 {
    record
        .evidence_at_epoch
        .unwrap_or(record.evidence_received_at_epoch)
}

fn same_period(left: &SpendRecord, right: &SpendRecord) -> bool {
    left.billing_period_start_epoch == right.billing_period_start_epoch
        && left.billing_period_end_epoch == right.billing_period_end_epoch
}

fn add_minor(base: &Money, increase: i64) -> Option<Money> {
    let amount_minor = i128::from(base.amount_minor) + i128::from(increase);
    i64::try_from(amount_minor)
        .ok()
        .map(|amount_minor| Money::new(amount_minor, base.currency.clone(), base.exponent))
}

fn reaches_capped_threshold(
    amount_minor: i64,
    budget_minor: i64,
    fixed_sgd_minor: i64,
    budget_percent: i64,
) -> bool {
    i128::from(amount_minor) >= i128::from(fixed_sgd_minor)
        || i128::from(amount_minor) * 100 >= i128::from(budget_minor) * i128::from(budget_percent)
}

fn push_issue(issues: &mut Vec<MonitorIssueCode>, issue: MonitorIssueCode) {
    if !issues.contains(&issue) {
        issues.push(issue);
    }
}

fn action_fingerprint(
    action: &MonitorAction,
    account_id: &str,
    goal_id: &str,
    state: &SpendState,
    budget_unverifiable: bool,
) -> String {
    let action_key = match action {
        MonitorAction::Checkpoint { .. } => {
            if budget_unverifiable {
                "checkpoint:budget_unverifiable"
            } else {
                "checkpoint:budget_threshold"
            }
        }
        MonitorAction::ReduceDispatch { max_parallel } => {
            if *max_parallel == Some(0) {
                "stop_dispatch"
            } else {
                "reduce_dispatch"
            }
        }
        MonitorAction::Pause { reason } => match reason {
            MonitorIssueCode::BudgetUnverifiable => "pause:budget_unverifiable",
            MonitorIssueCode::BudgetPause => "pause:budget_pause",
            _ => "pause:other",
        },
        MonitorAction::Warn { reason } => match reason {
            MonitorIssueCode::BudgetWarn => "warn:budget_warn",
            _ => "warn:other",
        },
        MonitorAction::Wait { .. } => "wait",
    };
    let baseline_period = state.baseline.as_ref().map(|record| {
        (
            record.billing_period_start_epoch,
            record.billing_period_end_epoch,
        )
    });
    let active_period = state.period_anchor.as_ref().map(|record| {
        (
            record.billing_period_start_epoch,
            record.billing_period_end_epoch,
        )
    });

    let mut hasher = Sha256::new();
    for field in ["spend-policy-v1", account_id, goal_id, action_key] {
        hash_field(&mut hasher, field.as_bytes());
    }
    hash_period(&mut hasher, baseline_period);
    hash_period(&mut hasher, active_period);
    let digest = hasher.finalize();
    let mut fingerprint = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _write_result = write!(fingerprint, "{byte:02x}");
    }
    fingerprint
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hash_period(hasher: &mut Sha256, period: Option<(i64, i64)>) {
    match period {
        Some((start, end)) => {
            hasher.update([1]);
            hasher.update(start.to_be_bytes());
            hasher.update(end.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

#[cfg(test)]
mod tests {
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
        let account = account_with_baseline(now, 1_000);
        let mut state = capture_goal_baseline(&account, Some(&budget), now);
        state.cumulative_goal_spend = Some(amount(3_999));
        let below = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(below.actions.is_empty());

        state.cumulative_goal_spend = Some(amount(4_000));
        let warn = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert_eq!(warn.actions.len(), 1);
        assert!(warn.issues.contains(&MonitorIssueCode::BudgetWarn));
        let repeated =
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now + 1);
        assert_eq!(warn.action_fingerprints, repeated.action_fingerprints);

        state.cumulative_goal_spend = Some(amount(4_499));
        let below_checkpoint =
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert_eq!(below_checkpoint.actions, warn.actions);

        state.cumulative_goal_spend = Some(amount(4_500));
        let checkpoint = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(checkpoint.actions.contains(&MonitorAction::Checkpoint {
            goal_id: GOAL.to_owned()
        }));
        assert!(checkpoint.actions.contains(&MonitorAction::ReduceDispatch {
            max_parallel: Some(0)
        }));

        state.cumulative_goal_spend = Some(amount(4_799));
        let below_pause =
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(!below_pause.actions.contains(&MonitorAction::Pause {
            reason: MonitorIssueCode::BudgetPause
        }));

        state.cumulative_goal_spend = Some(amount(4_800));
        let pause = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(pause.actions.contains(&MonitorAction::Pause {
            reason: MonitorIssueCode::BudgetPause
        }));
        assert!(!pause.budget_cap_reached);

        state.cumulative_goal_spend = Some(amount(5_000));
        let cap = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(cap.budget_cap_reached);
        assert!(cap.actions.contains(&MonitorAction::Pause {
            reason: MonitorIssueCode::BudgetPause
        }));
    }

    #[test]
    fn lower_custom_budget_caps_each_threshold_proportionally() {
        let now = PERIOD_START + 1_000;
        let budget = budget(3_000);
        let account = account_with_baseline(now, 1_000);
        let mut state = capture_goal_baseline(&account, Some(&budget), now);

        state.cumulative_goal_spend = Some(amount(2_399));
        assert!(
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .actions
                .is_empty()
        );
        state.cumulative_goal_spend = Some(amount(2_400));
        assert!(
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .actions
                .contains(&MonitorAction::Warn {
                    reason: MonitorIssueCode::BudgetWarn
                })
        );

        state.cumulative_goal_spend = Some(amount(2_699));
        assert!(
            !evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .actions
                .contains(&MonitorAction::Checkpoint {
                    goal_id: GOAL.to_owned()
                })
        );
        state.cumulative_goal_spend = Some(amount(2_700));
        let stop = evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now);
        assert!(stop.actions.contains(&MonitorAction::Checkpoint {
            goal_id: GOAL.to_owned()
        }));
        assert!(stop.actions.contains(&MonitorAction::ReduceDispatch {
            max_parallel: Some(0)
        }));

        state.cumulative_goal_spend = Some(amount(2_879));
        assert!(
            !evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .actions
                .contains(&MonitorAction::Pause {
                    reason: MonitorIssueCode::BudgetPause
                })
        );
        state.cumulative_goal_spend = Some(amount(2_880));
        assert!(
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .actions
                .contains(&MonitorAction::Pause {
                    reason: MonitorIssueCode::BudgetPause
                })
        );

        state.cumulative_goal_spend = Some(amount(3_000));
        assert!(
            evaluate_spend_policy(ACCOUNT, GOAL, Some(&budget), &account, &state, now)
                .budget_cap_reached
        );
    }
}
