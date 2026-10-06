// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Amp` usage, subscription, and renewal types.

/// The current Amp `userDisplayBalanceInfo.displayText` contract, shared by the
/// API and CLI paths through one parser: account identity, the Amp Free daily
/// remaining percentage, the monthly subscription pools (Agent dollars + Orb
/// hours, Tier or legacy Subscription shape), individual credit balance, and
/// per-workspace balances. Each pool keeps its own unit — dollars, hours, and
/// percents are never compressed into one metric.
#[derive(Debug, Clone, Default)]
pub(crate) struct AmpUsage {
    pub(crate) account_label: Option<String>,
    pub(crate) daily_remaining_percent: Option<u8>,
    pub(crate) individual_credits: Option<f64>,
    pub(crate) workspace_balances: Vec<AmpWorkspaceBalance>,
    pub(crate) subscription: Option<AmpSubscription>,
    pub(crate) renewal: Option<AmpRenewal>,
    pub(crate) billing_period: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpWorkspaceBalance {
    pub(crate) name: String,
    pub(crate) remaining: f64,
}

/// One monthly Amp subscription: the plan name (the funding route — which
/// subscription pays) plus the Agent/Orb pools in their native units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpSubscription {
    pub(crate) plan: String,
    pub(crate) kind: AmpSubscriptionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AmpSubscriptionKind {
    /// `Amp <plan> Tier: agent usage $<rem> of $<limit> remaining …` with an
    /// optional independent `orb usage <R>h of <L>h …` segment. Dollar/hours
    /// balances are authoritative; no rounded CLI percent is used.
    Tier {
        agent_remaining: f64,
        agent_limit: f64,
        orb_remaining_hours: Option<f64>,
        orb_limit_hours: Option<f64>,
    },
    /// Legacy `Subscription <plan>: <N>% other usage and <M>% orb usage
    /// remaining …` (or `Amp <plan> Subscription:`) percent pools.
    Legacy {
        agent_remaining_percent: u8,
        orb_remaining_percent: u8,
    },
}

/// Subscription renewal countdown (`resets upon renewal in <N> days|months`).
/// Months are calendar-approximated as 30 days; the pace label always shows
/// the raw countdown so the approximation is visible.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpRenewal {
    pub(crate) value: u64,
    pub(crate) months: bool,
}

impl AmpRenewal {
    pub(crate) fn label(&self) -> String {
        let unit = if self.months { "month" } else { "day" };
        let plural = if self.value == 1 { "" } else { "s" };
        format!("renews in {} {unit}{plural}", self.value)
    }

    pub(crate) fn resets_at(&self, now: i64) -> i64 {
        let days = if self.months {
            self.value.saturating_mul(30)
        } else {
            self.value
        };
        let seconds = days.saturating_mul(86_400);
        now.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}
