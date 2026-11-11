// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Write};
use std::sync::{Mutex, MutexGuard, PoisonError};

use jackin_console::domain::doctor::{DoctorCheck, DoctorReport, DoctorState};
use jackin_core::GatewayName;
use jackin_session::{Check, CheckStatus, DoctorReport as SessionReport};
use junie_tui::theme::StatusKind;

struct OneLine<'a>(&'a str);

impl fmt::Display for OneLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, word) in self.0.split_whitespace().enumerate() {
            if index > 0 {
                f.write_char(' ')?;
            }
            f.write_str(word)?;
        }
        Ok(())
    }
}

fn status(status: CheckStatus) -> StatusKind {
    match status {
        CheckStatus::Pass => StatusKind::Ok,
        CheckStatus::Warn => StatusKind::Warn,
        CheckStatus::Fail => StatusKind::Fail,
        CheckStatus::Skip => StatusKind::Skipped,
    }
}

fn check(check: &Check) -> DoctorCheck {
    let detail = OneLine(&check.detail).to_string();
    DoctorCheck {
        label: check.id.label().to_owned(),
        status: status(check.status),
        summary: detail.clone(),
        found: Some(detail),
        expected: None,
        fix: matches!(check.status, CheckStatus::Warn | CheckStatus::Fail)
            .then(|| check.fix.to_owned()),
    }
}

pub(crate) fn report(
    report: &SessionReport,
    gateway: Option<&GatewayName>,
    at_secs: i64,
) -> DoctorReport {
    DoctorReport {
        checks: report.checks.iter().map(check).collect(),
        at_secs,
        gateway: gateway.map(ToString::to_string),
    }
}

#[derive(Debug, Default)]
pub(crate) struct DoctorCell {
    last: Mutex<Option<DoctorReport>>,
}

impl DoctorCell {
    fn last(&self) -> MutexGuard<'_, Option<DoctorReport>> {
        self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn state(&self) -> DoctorState {
        self.last()
            .clone()
            .map_or(DoctorState::NeverRun, DoctorState::Done)
    }

    pub(crate) fn record(&self, report: DoctorReport) {
        *self.last() = Some(report);
    }
}
