// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod wire_failure_support;

#[test]
fn conformance_partial_success_is_not_retried() -> anyhow::Result<()> {
    wire_failure_support::assert_scripted_response(
        jackin_otlp_testbed::Behavior::PartialSuccess,
        true,
        1,
    )
}
