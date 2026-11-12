// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_console::backend::{Refresh, ValidationOutcome, apply_validation};
use jackin_console::domain::account::{
    Account, AccountRegistry, IssueCode, Recoverability, RecoverableIssue,
};
use jackin_console::domain::reason::{IssueReason, UsageReason};
use jackin_console::domain::usage::Freshness;
use jackin_core::{AccountName, Config};
use jackin_usage::UsageAccount;

#[derive(Debug)]
enum Step {
    Read(Box<Refresh>),
    Missed {
        error: String,
        at: i64,
    },
    Checked {
        outcome: Box<ValidationOutcome>,
        at: i64,
    },
}

impl Step {
    fn replay(&self, account: &mut Account) {
        match self {
            Step::Read(refresh) => {
                account.usage = refresh.usage.clone();
                account.issue = refresh.issue.clone();
                account.last_refresh_secs = Some(refresh.last_refresh_secs);
            }
            Step::Missed { error, at } => {
                account.usage.freshness.phase = if account.usage.freshness.last_good_secs.is_some()
                {
                    Freshness::Stale
                } else {
                    Freshness::Failed
                };
                account.issue = Some(RecoverableIssue::new(
                    IssueCode::ProviderUnavailable,
                    IssueReason::Usage(UsageReason::Provider {
                        detail: error.clone(),
                    }),
                    Recoverability::Retryable,
                ));
                account.last_refresh_secs = Some(*at);
            }
            Step::Checked { outcome, at } => {
                let windows = account.usage.windows.clone();
                apply_validation(account, outcome, *at);
                if account.usage.windows.is_empty() {
                    account.usage.windows = windows;
                }
            }
        }
    }
}

#[derive(Debug)]
struct Kept {
    source: UsageAccount,
    steps: Vec<Step>,
}

#[derive(Debug, Default)]
pub(crate) struct Readings {
    kept: BTreeMap<AccountName, Kept>,
}

impl Readings {
    pub(crate) fn read(&mut self, source: UsageAccount, refresh: Refresh) {
        let kept = self.of(source);
        kept.steps
            .retain(|step| !matches!(step, Step::Read(_) | Step::Missed { .. }));
        kept.steps.push(Step::Read(Box::new(refresh)));
    }

    pub(crate) fn missed(&mut self, source: UsageAccount, error: String, at: i64) {
        let kept = self.of(source);
        kept.steps
            .retain(|step| !matches!(step, Step::Missed { .. }));
        kept.steps.push(Step::Missed { error, at });
    }

    pub(crate) fn checked(&mut self, source: UsageAccount, outcome: ValidationOutcome, at: i64) {
        let kept = self.of(source);
        kept.steps
            .retain(|step| !matches!(step, Step::Checked { .. }));
        kept.steps.push(Step::Checked {
            outcome: Box::new(outcome),
            at,
        });
    }

    fn of(&mut self, source: UsageAccount) -> &mut Kept {
        let kept = self
            .kept
            .entry(source.name.clone())
            .or_insert_with(|| Kept {
                source: source.clone(),
                steps: Vec::new(),
            });
        if kept.source != source {
            *kept = Kept {
                source,
                steps: Vec::new(),
            };
        }
        kept
    }

    pub(crate) fn overlay(&mut self, config: &Config, registry: &mut AccountRegistry) {
        self.kept.retain(|name, kept| {
            config
                .account(name)
                .is_some_and(|core| UsageAccount::from_account(core) == kept.source)
        });
        for (name, kept) in &self.kept {
            if let Some(account) = registry.get_mut(name.as_str()) {
                for step in &kept.steps {
                    step.replay(account);
                }
            }
        }
    }
}
