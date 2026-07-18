// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_telemetry::schema::enums::{ErrorType, PtyExitReason};

pub(super) fn reason(
    status: Result<&portable_pty::ExitStatus, &std::io::Error>,
    cancelled: bool,
) -> PtyExitReason {
    if cancelled {
        return PtyExitReason::Cancelled;
    }
    match status {
        Ok(status) if status.success() => PtyExitReason::Clean,
        Ok(status) if status.signal().is_some() => PtyExitReason::Signal,
        Ok(_) => PtyExitReason::NonzeroExit,
        Err(_) => PtyExitReason::WaitFailed,
    }
}

pub(super) fn error_type(reason: PtyExitReason) -> Option<ErrorType> {
    match reason {
        PtyExitReason::Signal | PtyExitReason::NonzeroExit => Some(ErrorType::ProcessExitNonzero),
        PtyExitReason::WaitFailed => Some(ErrorType::IoError),
        PtyExitReason::Clean | PtyExitReason::Cancelled => None,
    }
}
