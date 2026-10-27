// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::OperatorNoticeSink;

use crate::logging::emit_compact_line;

struct DiagnosticsOperatorNotice;

impl OperatorNoticeSink for DiagnosticsOperatorNotice {
    fn notice(&self, kind: &str, line: &str) {
        emit_compact_line(kind, line);
    }
}

pub fn install_operator_notice_sink() {
    jackin_core::set_operator_notice_sink(Box::new(DiagnosticsOperatorNotice));
}
