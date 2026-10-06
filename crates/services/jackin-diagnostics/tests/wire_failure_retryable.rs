// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_failure_support;

#[test]
fn conformance_unavailable_retries_at_most_three_times() -> anyhow::Result<()> {
    wire_failure_support::assert_scripted_response(
        jackin_otlp_testbed::Behavior::Reject(tonic::Code::Unavailable),
        false,
        3,
    )
}
