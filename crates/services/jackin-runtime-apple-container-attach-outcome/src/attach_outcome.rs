// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::JackinPaths;
use jackin_runtime_apple_container_running::running::is_container_running;
use jackin_runtime_isolation::isolation::finalize::AttachOutcome;
use jackin_runtime_launch_attach_outcome::attach_outcome::record_instance_attach_outcome;

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
