// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_support;

#[test]
fn conformance_wire_host_delivers_all_three_signals() -> anyhow::Result<()> {
    wire_support::assert_three_signal_delivery(jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT)
}
