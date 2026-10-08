// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Run env strings for the launch run args.
//!
//! [`run_runtime_envs`] reports the extra `-e` entries the
//! current run contributes to the container run args
//! (the active telemetry invocation id, when one exists).

/// Extra container env entries identifying the current run.
///
/// Reports `JACKIN_INVOCATION_ID` for the active telemetry
/// invocation, or nothing when no invocation is active.
pub fn run_runtime_envs() -> Vec<String> {
    jackin_telemetry::identity::current_invocation().map_or_else(Vec::new, |invocation| {
        vec![format!("JACKIN_INVOCATION_ID={invocation}")]
    })
}
