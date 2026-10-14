// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Default)]
pub struct AmpUsage {
    pub account_label: Option<String>,
    pub daily_remaining_percent: Option<u8>,
    pub individual_credits: Option<f64>,
    pub workspace_balances: Vec<AmpWorkspaceBalance>,
    pub subscription: Option<AmpSubscription>,
    pub renewal: Option<AmpRenewal>,
    pub billing_period: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmpWorkspaceBalance {
    pub name: String,
    pub remaining: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmpSubscription {
    pub plan: String,
    pub kind: AmpSubscriptionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AmpSubscriptionKind {
    Tier {
        agent_remaining: f64,
        agent_limit: f64,
        orb_remaining_hours: Option<f64>,
        orb_limit_hours: Option<f64>,
    },
    Legacy {
        agent_remaining_percent: u8,
        orb_remaining_percent: u8,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmpRenewal {
    pub value: u64,
    pub months: bool,
}

impl AmpRenewal {
    pub fn label(&self) -> String {
        let unit = if self.months { "month" } else { "day" };
        let plural = if self.value == 1 { "" } else { "s" };
        format!("renews in {} {unit}{plural}", self.value)
    }

    pub fn resets_at(&self, now: i64) -> i64 {
        let days = if self.months {
            self.value.saturating_mul(30)
        } else {
            self.value
        };
        let seconds = days.saturating_mul(86_400);
        now.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}
