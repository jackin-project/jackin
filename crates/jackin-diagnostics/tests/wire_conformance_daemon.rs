// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_support;

#[test]
fn conformance_wire_daemon_delivers_all_three_signals() {
    wire_support::assert_three_signal_delivery(jackin_diagnostics::ServiceIdentity::DAEMON);
}
