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
    /// Latest period boundary after which goal spend history cannot be
    /// asserted complete. Set when a verified correction cannot be applied
    /// because the account no longer retains its period, and conservatively
    /// during V3 migration when rollover history could have hidden such a
    /// correction. A migration horizon records uncertainty, not proof that a
    /// correction was received. V3 snapshots omit this field by default.
    #[serde(default)]
    pub historical_correction_horizon_epoch: Option<i64>,
}

/// Per-monitor spend state. `baseline` is the account total captured at Start;
/// cumulative spend counts only increases after that baseline.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SpendState {
    /// Fresh verified account total captured when this monitor started.
    pub baseline: Option<SpendRecord>,
    /// Latest account total already folded into cumulative goal spend.
    pub period_anchor: Option<SpendRecord>,
    /// Latest verified total for the immediately preceding period already
    /// folded into cumulative spend. This lets later corrections be applied
    /// exactly once after the period anchor has advanced.
    #[serde(default)]
    pub closed_period_anchor: Option<SpendRecord>,
    /// Cumulative goal spend across billing periods, when known.
    pub cumulative_goal_spend: Option<Money>,
    /// A closed-period transition could not yet be proven.
    pub rollover_unknown: bool,
    /// False when any billing-period amount may be missing from the total.
    pub cumulative_complete: bool,
}

/// Check that persisted account receipts retain their syntactic shape and
/// account binding. Unverified receipts and unsupported currencies are valid
/// account history: they remain available for audit but cannot authorize a
/// budget comparison.
pub(super) fn validate_account_spend_state(state: &SpendAccountState) -> bool {
    let records = [
        state.latest_record.as_ref(),
        state.current_period_record.as_ref(),
        state.previous_period_record.as_ref(),
    ];
    let mut account_id = None;
    for record in records.into_iter().flatten() {
        if record.account_id.trim().is_empty()
            || record.billing_period_start_epoch < 0
            || record.billing_period_end_epoch <= record.billing_period_start_epoch
            || record.amount.amount_minor < 0
            || !matches!(record.source, SpendRecordSource::OperatorReceipt)
            || (record.verification != SpendVerification::Unverified
                && !is_supported_sgd(&record.amount))
            || (record.verification == SpendVerification::Verified
                && !verified_evidence_is_coherent(record))
            || account_id.is_some_and(|account_id| account_id != record.account_id.as_str())
        {
            return false;
        }
        account_id = Some(record.account_id.as_str());
    }
    state
        .historical_correction_horizon_epoch
        .is_none_or(|epoch| epoch >= 0)
}

/// Check spend history before it is trusted by the budget guard or accepted in
/// persisted goal state. A missing baseline is retained for historical V1
/// state, but an asserted baseline must be paired with a coherent verified
/// anchor and cumulative total.
pub(super) fn validate_spend_state(state: &SpendState) -> bool {
    let baseline = state.baseline.as_ref();
    let anchor = state.period_anchor.as_ref();
    let closed_anchor = state.closed_period_anchor.as_ref();
    let cumulative = state.cumulative_goal_spend.as_ref();

    if baseline.is_some_and(|record| !valid_goal_spend_anchor(record))
        || anchor.is_some_and(|record| !valid_goal_spend_anchor(record))
        || closed_anchor.is_some_and(|record| !valid_goal_spend_anchor(record))
        || cumulative.is_some_and(|amount| !is_supported_sgd(amount) || amount.amount_minor < 0)
    {
        return false;
    }

    // Legacy history can have no original baseline. Preserve it when the
    // anchor and cumulative amount travel together, but reject partial pairs.
    if anchor.is_some() != cumulative.is_some() {
        return false;
    }

    let Some(baseline) = baseline else {
        return closed_anchor.is_none_or(|closed| {
            anchor.is_some_and(|anchor| {
                closed.account_id == anchor.account_id
                    && closed.billing_period_end_epoch == anchor.billing_period_start_epoch
            })
        });
    };
    let (Some(anchor), Some(cumulative)) = (anchor, cumulative) else {
        return false;
    };
    if baseline.account_id != anchor.account_id
        || closed_anchor.is_some_and(|closed| {
            closed.account_id != baseline.account_id
                || same_period(baseline, anchor)
                || closed.billing_period_end_epoch != anchor.billing_period_start_epoch
        })
        || (!state.cumulative_complete && !state.rollover_unknown)
    {
        return false;
    }

    if same_period(baseline, anchor) {
        if anchor.amount.amount_minor < baseline.amount.amount_minor {
            return false;
        }
        let expected_cumulative = anchor.amount.amount_minor - baseline.amount.amount_minor;
        cumulative.amount_minor == expected_cumulative
    } else {
        // The anchor may advance across any number of periods. Gaps are
        // represented by rollover_unknown/cumulative_complete and remain
        // fail-closed at evaluation time.
        anchor.billing_period_start_epoch >= baseline.billing_period_end_epoch
            && cumulative.amount_minor >= anchor.amount.amount_minor
    }
}

fn valid_goal_spend_anchor(record: &SpendRecord) -> bool {
    !record.account_id.trim().is_empty()
        && record.billing_period_start_epoch >= 0
        && record.billing_period_end_epoch > record.billing_period_start_epoch
        && record.amount.amount_minor >= 0
        && is_supported_sgd(&record.amount)
        && record.verification == SpendVerification::Verified
        && verified_evidence_is_coherent(record)
        && matches!(record.source, SpendRecordSource::OperatorReceipt)
}

fn verified_evidence_is_coherent(record: &SpendRecord) -> bool {
    record.evidence_received_at_epoch >= 0
        && record.evidence_at_epoch.is_none_or(|evidence_at| {
            evidence_at >= 0 && evidence_at <= record.evidence_received_at_epoch
        })
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
            if closed_period_receipt {
                next.historical_correction_horizon_epoch = Some(
                    next.historical_correction_horizon_epoch
                        .unwrap_or(0)
                        .max(record.billing_period_end_epoch),
                );
            }
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
    if !validate_account_spend_state(account) {
        return SpendState::default();
    }
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
        closed_period_anchor: None,
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
    if !validate_spend_state(state)
        || !validate_account_spend_state(account)
        || state.baseline.is_none()
    {
        return;
    }

    let (Some(current), Some(anchor), Some(mut cumulative)) = (
        account.current_period_record.clone(),
        state.period_anchor.clone(),
        state.cumulative_goal_spend.clone(),
    ) else {
        state.rollover_unknown = true;
        return;
    };

    let Some(baseline) = state.baseline.as_ref() else {
        return;
    };
    if account
        .historical_correction_horizon_epoch
        .is_some_and(|period_end| period_end > baseline.billing_period_start_epoch)
    {
        // An unretained correction may change spend accumulated after this
        // goal's baseline. Keep the known estimate for display, but do not
        // restore completeness from newer account receipts.
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    }
    if !same_period(baseline, &anchor) && state.closed_period_anchor.is_none() {
        // Historical state may have already folded one or more periods but
        // cannot say which closing total its cumulative amount includes. Do
        // not infer a correction baseline or restore completeness from newer
        // evidence.
        state.rollover_unknown = true;
        state.cumulative_complete = false;
        return;
    }

    if !money_matches_budget(&current.amount, budget)
        || current.verification != SpendVerification::Verified
        || !record_is_fresh(&current, now_epoch)
    {
        return;
    }

    if same_period(&anchor, &current) {
        let mut reconciled_closed_anchor = None;
        if let Some(closed_anchor) = state.closed_period_anchor.clone() {
            let Some(closed) = account
                .previous_period_record
                .as_ref()
                .filter(|record| same_period(record, &closed_anchor))
            else {
                state.rollover_unknown = true;
                state.cumulative_complete = false;
                return;
            };
            if closed != &closed_anchor {
                let correction_is_verifiable = closed.verification == SpendVerification::Verified
                    && evidence_time(closed) >= closed.billing_period_end_epoch
                    && record_is_fresh(closed, now_epoch)
                    && closed.amount.currency == closed_anchor.amount.currency
                    && closed.amount.exponent == closed_anchor.amount.exponent
                    && closed.amount.amount_minor >= closed_anchor.amount.amount_minor;
                if !correction_is_verifiable {
                    state.rollover_unknown = true;
                    state.cumulative_complete = false;
                    return;
                }
                let correction = closed.amount.amount_minor - closed_anchor.amount.amount_minor;
                let Some(updated) = add_minor(&cumulative, correction) else {
                    state.rollover_unknown = true;
                    state.cumulative_complete = false;
                    return;
                };
                cumulative = updated;
                reconciled_closed_anchor = Some(closed.clone());
            }
        }
        if current.amount.amount_minor < anchor.amount.amount_minor {
            state.rollover_unknown = true;
            state.cumulative_complete = false;
            return;
        }
        let increase = current.amount.amount_minor - anchor.amount.amount_minor;
        if let Some(updated) = add_minor(&cumulative, increase) {
            state.cumulative_goal_spend = Some(updated);
            state.period_anchor = Some(current.clone());
            if let Some(closed_anchor) = reconciled_closed_anchor {
                state.closed_period_anchor = Some(closed_anchor);
            }
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
    let rollover_is_proven = same_period(&anchor, closed)
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
    let Some(after_closed_period) = add_minor(&cumulative, closed_increase) else {
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
    state.closed_period_anchor = Some(closed.clone());
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
    let spend_state_is_valid = validate_spend_state(state);
    let account_state_is_valid = validate_account_spend_state(account)
        && [
            account.latest_record.as_ref(),
            account.current_period_record.as_ref(),
            account.previous_period_record.as_ref(),
        ]
        .into_iter()
        .flatten()
        .all(|record| record.account_id == account_id);
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
    let cumulative_is_compatible = state
        .cumulative_goal_spend
        .as_ref()
        .is_some_and(|cumulative| money_matches_budget(cumulative, budget));
    let budget_is_valid = budget.is_some_and(valid_budget);
    let budget_unverifiable = !budget_is_valid
        || !spend_state_is_valid
        || !account_state_is_valid
        || verification != SpendVerification::Verified
        || !baseline_is_compatible
        || !current_is_compatible
        || !cumulative_is_compatible
        || !state.cumulative_complete
        || rollover_unknown;

    let mut issues = Vec::new();
    if verification == SpendVerification::Unavailable || state.baseline.is_none() {
        push_issue(&mut issues, MonitorIssueCode::SpendUnavailable);
    } else if verification == SpendVerification::Stale {
        push_issue(&mut issues, MonitorIssueCode::SpendStale);
    } else if verification != SpendVerification::Verified
        || !spend_state_is_valid
        || !account_state_is_valid
        || !baseline_is_compatible
        || !current_is_compatible
        || !cumulative_is_compatible
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
mod tests;
