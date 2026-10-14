// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::BuildLogSink;

#[derive(Debug)]
pub struct DiagnosticsBuildLogSink;

impl BuildLogSink for DiagnosticsBuildLogSink {
    fn push_line(&self, line: &str) {
        jackin_diagnostics::build_log::push_line(line);
    }
}
