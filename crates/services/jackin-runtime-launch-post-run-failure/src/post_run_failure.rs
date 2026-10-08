// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Post-run failure telemetry for the launch run args path.
//!
//! [`emit_post_run_failure`] reports a failed post-run step
//! when the failure tripped the isolation firewall, so the
//! firewall breach is visible in diagnostics.

/// Report a failed post-run step to diagnostics.
///
/// Emits the isolation-firewall breach (allowlist network mode)
/// when `is_firewall` is set; otherwise a no-op.
pub fn emit_post_run_failure(is_firewall: bool) {
    if is_firewall {
        jackin_diagnostics::operation::isolation_firewall_failed(
            jackin_telemetry::schema::enums::NetworkMode::Allowlist,
        );
    }
}
