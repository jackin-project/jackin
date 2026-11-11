// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{CheckRow, Refresh, Validation, ValidationOutcome};
use jackin_console::domain::account::{
    AccountIdentity, Confidence, CredentialSource, IssueCode, Lifecycle, Recoverability,
    RecoverableIssue, ValidationLevel,
};
use jackin_console::domain::reason::{Found, IssueReason, RefreshOutcome, UsageReason};
use jackin_console::domain::usage::{
    AccountUsage, Freshness, FreshnessInfo, QuotaStatus, QuotaWindow, WindowCategory,
};
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::world::Msg;
use jackin_core::AccountName;
use jackin_session::error_chain;
use jackin_usage::{
    LimitWindow, Refresh as Cache, Remaining, UnavailableReason, UsageAccount, UsageOutcome,
    UsageRequest, WindowKind,
};

use crate::bridge::Settled;
use crate::checks::{found_of, skipped};
use crate::observe::{Inputs, locked};
use crate::project::{provider, source};

fn category(kind: WindowKind) -> WindowCategory {
    match kind {
        WindowKind::FiveHour => WindowCategory::Session,
        WindowKind::Weekly | WindowKind::Monthly => WindowCategory::LongRange,
        WindowKind::Credits | WindowKind::Spend | WindowKind::Other => WindowCategory::Other,
    }
}

fn window(found: &LimitWindow) -> QuotaWindow {
    let used_pct = found.used.map(jackin_usage::Percent::rounded);
    let spend = if matches!(found.remaining, Some(Remaining::Amount { .. })) {
        Some(found.remaining_text())
    } else if found.spent.is_some() {
        Some(found.spent_text())
    } else {
        None
    };
    QuotaWindow {
        id: found.kind.id(),
        label: found.label.clone(),
        category: category(found.kind),
        unit: jackin_console::domain::usage::WindowUnit::Percent,
        used: None,
        limit: None,
        used_pct,
        reset_secs: found.resets_at.map(|at| at.0),
        spend_label: spend.clone(),
        status: used_pct.map_or(QuotaStatus::Available, QuotaStatus::from_pct),
        note: spend,
    }
}

struct Reason {
    code: IssueCode,
    usage: UsageReason,
    status: QuotaStatus,
    recoverability: Recoverability,
    retry_secs: Option<i64>,
    failed: bool,
}

fn usage_reason(reason: &UnavailableReason, now: i64) -> UsageReason {
    match reason {
        UnavailableReason::NoCredentials => UsageReason::NoCredentials,
        UnavailableReason::ExpiredLogin => UsageReason::ExpiredLogin,
        UnavailableReason::Unsupported { .. } => UsageReason::Unsupported,
        UnavailableReason::RateLimited { retry_at } => UsageReason::RateLimited {
            retry_in_secs: retry_at.map(|at| at.0 - now),
        },
        UnavailableReason::Network(detail) => UsageReason::Network {
            detail: detail.clone(),
        },
        UnavailableReason::Provider(detail) => UsageReason::Provider {
            detail: detail.clone(),
        },
    }
}

fn reason_of(reason: &UnavailableReason, now: i64) -> Reason {
    let (code, status, recoverability, failed) = match reason {
        UnavailableReason::ExpiredLogin => (
            IssueCode::Unauthorized,
            QuotaStatus::Error,
            Recoverability::ActionRequired,
            false,
        ),
        UnavailableReason::NoCredentials => (
            IssueCode::CredentialFileMissing,
            QuotaStatus::Error,
            Recoverability::ActionRequired,
            false,
        ),
        UnavailableReason::Unsupported { .. } => (
            IssueCode::QuotaUnsupported,
            QuotaStatus::Unsupported,
            Recoverability::Unsupported,
            false,
        ),
        UnavailableReason::RateLimited { .. } => (
            IssueCode::RateLimited,
            QuotaStatus::Unavailable,
            Recoverability::Retryable,
            false,
        ),
        UnavailableReason::Network(_) | UnavailableReason::Provider(_) => (
            IssueCode::ProviderUnavailable,
            QuotaStatus::Unavailable,
            Recoverability::Retryable,
            true,
        ),
    };
    Reason {
        code,
        usage: usage_reason(reason, now),
        status,
        recoverability,
        retry_secs: match reason {
            UnavailableReason::RateLimited { retry_at } => retry_at.map(|at| at.0),
            _ => None,
        },
        failed,
    }
}

pub(crate) struct Mapped {
    pub(crate) usage: AccountUsage,
    pub(crate) issue: Option<RecoverableIssue>,
    pub(crate) outcome: RefreshOutcome,
}

fn refresh_outcome(freshness: jackin_usage::Freshness) -> RefreshOutcome {
    match freshness {
        jackin_usage::Freshness::Fresh => RefreshOutcome::Current,
        jackin_usage::Freshness::Stale { .. } => RefreshOutcome::Stale,
    }
}

pub(crate) fn mapped(found: &jackin_usage::AccountUsage, now: i64) -> Mapped {
    let freshness = match found.freshness {
        jackin_usage::Freshness::Fresh => FreshnessInfo::current(now),
        jackin_usage::Freshness::Stale { since } => FreshnessInfo {
            phase: Freshness::Stale,
            last_good_secs: Some(since.0),
            retry_secs: None,
        },
    };
    match &found.outcome {
        UsageOutcome::Windows(windows) => Mapped {
            outcome: refresh_outcome(found.freshness),
            usage: AccountUsage {
                freshness,
                windows: windows.iter().map(window).collect(),
            },
            issue: None,
        },
        UsageOutcome::Unavailable(reason) => {
            let Reason {
                code,
                usage: why,
                status,
                recoverability,
                retry_secs,
                failed,
            } = reason_of(reason, now);
            let why = IssueReason::Usage(why);
            let sentinel = QuotaWindow::sentinel(
                WindowKind::Other.id(),
                provider(found.provider).map_or_else(
                    || found.provider.as_str(),
                    |p| p.usage_surface().surface_name(),
                ),
                status,
                &Vocabulary::openshell().reason(&why),
            );
            let mut issue = RecoverableIssue::new(code, why, recoverability);
            issue.retry_secs = retry_secs;
            Mapped {
                outcome: if failed {
                    RefreshOutcome::Failed
                } else {
                    refresh_outcome(found.freshness)
                },
                usage: AccountUsage {
                    freshness: if failed {
                        FreshnessInfo::failed(None, retry_secs)
                    } else {
                        freshness
                    },
                    windows: vec![sentinel],
                },
                issue: Some(issue),
            }
        }
    }
}

pub(crate) fn validation(
    found: &jackin_usage::AccountUsage,
    source: &CredentialSource,
    now: i64,
) -> ValidationOutcome {
    let Mapped { usage, issue, .. } = mapped(found, now);
    let material = CheckRow::Ok(found_of(source));
    match &found.outcome {
        UsageOutcome::Windows(_) => ValidationOutcome {
            level: Some(ValidationLevel::QuotaReadable),
            identity: AccountIdentity::default(),
            confidence: Confidence::Authoritative,
            lifecycle: Lifecycle::Available,
            issue,
            usage: Some(usage),
            material,
            identity_row: skipped(),
            quota_row: CheckRow::Ok(Found::Level(ValidationLevel::QuotaReadable)),
        },
        UsageOutcome::Unavailable(reason) => {
            let why = IssueReason::Usage(usage_reason(reason, now));
            let (level, lifecycle, identity_row) = match reason {
                UnavailableReason::NoCredentials => (None, Lifecycle::NeedsSecret, skipped()),
                UnavailableReason::ExpiredLogin => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::NeedsLogin,
                    CheckRow::Failed(why.clone()),
                ),
                UnavailableReason::Unsupported { .. } => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::Available,
                    skipped(),
                ),
                UnavailableReason::RateLimited { .. } => (
                    Some(ValidationLevel::IdentityAuthenticated),
                    Lifecycle::Available,
                    skipped(),
                ),
                UnavailableReason::Network(_) | UnavailableReason::Provider(_) => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::Unavailable,
                    skipped(),
                ),
            };
            ValidationOutcome {
                level,
                identity: AccountIdentity::default(),
                confidence: Confidence::None,
                lifecycle,
                issue,
                usage: Some(usage),
                material: if level.is_some() {
                    material
                } else {
                    CheckRow::Failed(why.clone())
                },
                identity_row,
                quota_row: CheckRow::Failed(why),
            }
        }
    }
}

struct Read {
    found: jackin_usage::AccountUsage,
    source: CredentialSource,
    account: UsageAccount,
    now: i64,
}

impl Inputs {
    async fn read_usage(&self, account: &str) -> Result<Read, String> {
        let said = |error: &dyn std::error::Error| error_chain(error);
        let name = AccountName::parse(account).map_err(|error| said(&error))?;
        let _turn = self.usage_gate.lock().await;
        let config = self.config.load().await.map_err(|error| said(&error))?;
        let core = config
            .require_account(&name)
            .map_err(|error| said(&error))?;
        let account = UsageAccount::from_account(core);
        let found = match self.report_of(&account).await {
            Ok(found) => found,
            Err(error) => {
                locked(&self.readings).missed(account, error.clone(), self.host.clock.now().0);
                return Err(error);
            }
        };
        Ok(Read {
            found,
            source: source(core.credential(), provider(core.provider())),
            account,
            now: self.host.clock.now().0,
        })
    }

    async fn report_of(
        &self,
        account: &UsageAccount,
    ) -> Result<jackin_usage::AccountUsage, String> {
        let request = UsageRequest {
            accounts: vec![account.clone()],
            refresh: Cache::Force,
        };
        let report = self
            .usage
            .report(&request, &self.cancel)
            .await
            .map_err(|error| error_chain(&error))?;
        report
            .accounts
            .into_iter()
            .find(|found| found.account == account.name)
            .ok_or_else(|| error_chain(&jackin_usage::UsageError::Cancelled))
    }

    pub(crate) async fn refresh_account(
        &self,
        account: &str,
        requested: CredentialSource,
    ) -> Settled {
        let read = self.read_usage(account).await;
        let source = read.as_ref().map_or(requested, |read| read.source.clone());
        let result = read.map(|read| {
            let Mapped {
                usage,
                issue,
                outcome,
            } = mapped(&read.found, read.now);
            let refresh = Refresh {
                outcome,
                usage,
                issue,
                last_refresh_secs: read.now,
            };
            locked(&self.readings).read(read.account, refresh.clone());
            Box::new(refresh)
        });
        Msg::AccountRefreshed {
            account: account.to_owned(),
            source: Box::new(source),
            result,
        }
        .into()
    }

    pub(crate) async fn validate_account(
        &self,
        account: &str,
        requested: CredentialSource,
    ) -> Settled {
        let read = self.read_usage(account).await;
        let source = read.as_ref().map_or(requested, |read| read.source.clone());
        let result = read.map(|read| {
            let outcome = validation(&read.found, &read.source, read.now);
            locked(&self.readings).checked(read.account, outcome.clone(), read.now);
            Validation::Checked(Box::new(outcome))
        });
        Msg::AccountValidated {
            account: account.to_owned(),
            source: Box::new(source),
            result,
        }
        .into()
    }
}
