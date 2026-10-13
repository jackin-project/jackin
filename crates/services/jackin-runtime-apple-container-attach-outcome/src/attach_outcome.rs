// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Post-attach outcome recording shared by the apple-container paths.
//!
//! [`record_attach_outcome`] probes the container through the shared
//! running-state probe and records the outcome into the instance
//! manifest so `jackin --inspect` can show whether a role crashed.
//! A still-running container records running regardless of the exit
//! code; a stopped one records the code (`-1` when signalled).

use jackin_core::JackinPaths;
use jackin_runtime_apple_container_running::running::is_container_running;
use jackin_runtime_isolation::isolation::finalize::AttachOutcome;
use jackin_runtime_launch_attach_outcome::attach_outcome::record_instance_attach_outcome;

/// Record the post-attach outcome into the instance manifest so
/// `jackin --inspect` can show whether a role crashed. This records the outcome
/// only; unlike the Docker reconnect path it does not run session
/// finalization/teardown — apple-container finalization is not yet wired.
/// Best-effort: a missing/corrupt manifest is a no-op (logged downstream).
pub async fn record_attach_outcome(
    paths: &JackinPaths,
    container_name: &str,
    exit_code: Option<i32>,
) {
    let outcome = if is_container_running(container_name).await {
        AttachOutcome::still_running()
    } else {
        AttachOutcome::stopped(exit_code.unwrap_or(-1))
    };
    drop(record_instance_attach_outcome(
        paths,
        container_name,
        outcome,
    ));
}
