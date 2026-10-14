// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn run_runtime_envs() -> Vec<String> {
    jackin_telemetry::identity::current_invocation().map_or_else(Vec::new, |invocation| {
        vec![format!("JACKIN_INVOCATION_ID={invocation}")]
    })
}
