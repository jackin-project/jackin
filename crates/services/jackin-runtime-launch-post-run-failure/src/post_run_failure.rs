// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn emit_post_run_failure(is_firewall: bool) {
    if is_firewall {
        jackin_diagnostics::operation::isolation_firewall_failed(
            jackin_telemetry::schema::enums::NetworkMode::Allowlist,
        );
    }
}
