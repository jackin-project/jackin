// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `CleanupTiming` drop guard and failure reporting.

pub(crate) struct CleanupTiming {
    name: &'static str,
}

impl Drop for CleanupTiming {
    fn drop(&mut self) {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Cleanup,
            self.name,
            None,
        );
    }
}

pub(crate) fn cleanup_timing(name: &'static str) -> CleanupTiming {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Cleanup,
        name,
        None,
    );
    CleanupTiming { name }
}

pub(crate) fn cleanup_failure(_message: impl AsRef<str>) {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
}
