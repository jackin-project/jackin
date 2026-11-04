// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{CheckRow, Refresh, Validation, ValidationOutcome};
use jackin_console::domain::account::{
    AccountIdentity, Confidence, CredentialSource, IssueCode, Lifecycle, Recoverability,
    RecoverableIssue, ValidationLevel,
};
use jackin_console::domain::usage::{
    AccountUsage, Freshness, FreshnessInfo, QuotaStatus, QuotaWindow, WindowCategory,
};
use jackin_console::world::Msg;
use jackin_core::AccountName;
use jackin_session::error_chain;
use jackin_usage::{
    LimitWindow, Refresh as Cache, Remaining, UnavailableReason, UsageAccount, UsageOutcome,
    UsageRequest, WindowKind,
};

use crate::bridge::Settled;
use crate::observe::Inputs;
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
    QuotaWindow {
        id: found.kind.id(),
        label: found.label.clone(),
        category: category(found.kind),
        unit: jackin_console::domain::usage::WindowUnit::Percent,
        used: None,
        limit: None,
        used_pct,
        reset_secs: found.resets_at.map(|at| at.0),
        spend_label: matches!(found.remaining, Some(Remaining::Amount { .. }))
            .then(|| found.remaining_text()),
        status: used_pct.map_or(QuotaStatus::Available, QuotaStatus::from_pct),
        note: None,
    }
}

struct Reason {
    code: IssueCode,
    status: QuotaStatus,
    recoverability: Recoverability,
    retry_secs: Option<i64>,
    failed: bool,
}

fn reason_of(reason: &UnavailableReason) -> Reason {
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
            usage: AccountUsage {
                freshness,
                windows: windows.iter().map(window).collect(),
            },
            issue: None,
        },
        UsageOutcome::Unavailable(reason) => {
            let Reason {
                code,
                status,
                recoverability,
                retry_secs,
                failed,
            } = reason_of(reason);
            let note = reason.to_string();
            let sentinel = QuotaWindow::sentinel(
                WindowKind::Other.id(),
                provider(found.provider).usage_surface().surface_name(),
                status,
                &note,
            );
            let mut issue = RecoverableIssue::new(code, note, recoverability);
            issue.retry_secs = retry_secs;
            Mapped {
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

fn row(text: &str) -> CheckRow {
    CheckRow::Ok(text.to_owned())
}

pub(crate) fn validation(
    found: &jackin_usage::AccountUsage,
    source: &CredentialSource,
    now: i64,
) -> ValidationOutcome {
    let Mapped { usage, issue } = mapped(found, now);
    let material = row(&source.safe_detail());
    match &found.outcome {
        UsageOutcome::Windows(_) => ValidationOutcome {
            level: Some(ValidationLevel::QuotaReadable),
            identity: AccountIdentity::default(),
            confidence: Confidence::Authoritative,
            lifecycle: Lifecycle::Available,
            issue,
            usage: Some(usage),
            material,
            identity_row: CheckRow::Skipped(String::new()),
            quota_row: row(ValidationLevel::QuotaReadable.label()),
        },
        UsageOutcome::Unavailable(reason) => {
            let note = reason.to_string();
            let (level, lifecycle, identity_row) = match reason {
                UnavailableReason::NoCredentials => (
                    None,
                    Lifecycle::NeedsSecret,
                    CheckRow::Skipped(String::new()),
                ),
                UnavailableReason::ExpiredLogin => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::NeedsLogin,
                    CheckRow::Failed(note.clone()),
                ),
                UnavailableReason::Unsupported { .. } => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::Available,
                    CheckRow::Skipped(String::new()),
                ),
                UnavailableReason::RateLimited { .. } => (
                    Some(ValidationLevel::IdentityAuthenticated),
                    Lifecycle::Available,
                    CheckRow::Skipped(String::new()),
                ),
                UnavailableReason::Network(_) | UnavailableReason::Provider(_) => (
                    Some(ValidationLevel::MaterialDiscovered),
                    Lifecycle::Unavailable,
                    CheckRow::Skipped(String::new()),
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
                    CheckRow::Failed(note.clone())
                },
                identity_row,
                quota_row: CheckRow::Failed(note),
            }
        }
    }
}

struct Read {
    found: jackin_usage::AccountUsage,
    source: CredentialSource,
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
        let request = UsageRequest {
            accounts: vec![UsageAccount::from_account(core)],
            refresh: Cache::Force,
        };
        let report = self
            .usage
            .report(&request, &self.cancel)
            .await
            .map_err(|error| said(&error))?;
        let found = report
            .accounts
            .into_iter()
            .find(|found| found.account == name)
            .ok_or_else(|| said(&jackin_usage::UsageError::Cancelled))?;
        Ok(Read {
            found,
            source: source(core.credential(), provider(core.provider())),
            now: self.host.clock.now().0,
        })
    }

    pub(crate) async fn refresh_account(&self, account: &str) -> Settled {
        let result = self.read_usage(account).await.map(|read| {
            let Mapped { usage, issue } = mapped(&read.found, read.now);
            Box::new(Refresh {
                outcome: usage.freshness.phase.label().to_owned(),
                usage,
                issue,
                last_refresh_secs: read.now,
            })
        });
        Msg::AccountRefreshed {
            account: account.to_owned(),
            result,
        }
        .into()
    }

    pub(crate) async fn validate_account(&self, account: &str) -> Settled {
        let result = self.read_usage(account).await.map(|read| {
            Validation::Checked(Box::new(validation(&read.found, &read.source, read.now)))
        });
        Msg::AccountValidated {
            account: account.to_owned(),
            result,
        }
        .into()
    }
}
