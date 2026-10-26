// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[must_use]
pub fn jackin_status_command(expected_protocol_major: u16) -> String {
    format!(
        "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule protocol-check --expected-major {expected_protocol_major} && /jackin/runtime/jackin-capsule status"
    )
}

pub fn parse_session_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Sessions:")
            .and_then(|value| value.trim().parse().ok())
    })
}
