// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_failure_support;

#[test]
fn conformance_unauthenticated_is_not_retried() -> anyhow::Result<()> {
    wire_failure_support::assert_scripted_response(
        jackin_otlp_testbed::Behavior::Reject(tonic::Code::Unauthenticated),
        false,
        1,
    )
}
