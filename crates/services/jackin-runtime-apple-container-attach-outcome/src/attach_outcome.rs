// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;

use anyhow::Result;
use jackin_core::JackinPaths;
use jackin_runtime_apple_container_client::apple_container_client::{
    AppleContainerInfo, AppleContainerStatus,
};
use jackin_runtime_isolation::isolation::finalize::AttachOutcome;
use jackin_runtime_launch_attach_outcome::attach_outcome::record_instance_attach_outcome;

mod diagnostics;

pub async fn record_attach_outcome(
    paths: &JackinPaths,
    container_id: &str,
    exit_code: Option<i32>,
) {
    let result = record_attach_outcome_with(
        container_id,
        exit_code,
        || jackin_runtime_apple_container_running::running::inspect_container(container_id),
        |outcome| record_instance_attach_outcome(paths, container_id, outcome),
    )
    .await;
    if let Err(error) = result {
        diagnostics::emit(
            container_id,
            diagnostics::AttachOutcomeDiagnostic::InspectionUncertain(&error),
        );
    }
}

fn outcome_for_status(
    status: AppleContainerStatus,
    exit_code: Option<i32>,
) -> Option<AttachOutcome> {
    match status {
        AppleContainerStatus::Running => Some(AttachOutcome::still_running()),
        AppleContainerStatus::Stopped => Some(AttachOutcome::stopped(exit_code.unwrap_or(-1))),
        AppleContainerStatus::Unknown | AppleContainerStatus::Stopping => None,
    }
}

async fn record_attach_outcome_with<P, PF, R>(
    container_id: &str,
    exit_code: Option<i32>,
    probe: P,
    record: R,
) -> Result<()>
where
    P: FnOnce() -> PF,
    PF: Future<Output = Result<Option<AppleContainerInfo>>>,
    R: FnOnce(AttachOutcome) -> Result<()>,
{
    let Some(info) = probe().await? else {
        diagnostics::emit(container_id, diagnostics::AttachOutcomeDiagnostic::Absent);
        return Ok(());
    };
    anyhow::ensure!(
        info.id == container_id,
        "Apple Container inspection returned a different id"
    );
    let Some(outcome) = outcome_for_status(info.status, exit_code) else {
        let state = match info.status {
            AppleContainerStatus::Unknown => diagnostics::UncertainObservedState::Unknown,
            AppleContainerStatus::Stopping => diagnostics::UncertainObservedState::Stopping,
            AppleContainerStatus::Running | AppleContainerStatus::Stopped => return Ok(()),
        };
        diagnostics::emit(
            container_id,
            diagnostics::AttachOutcomeDiagnostic::StateUncertain(state),
        );
        return Ok(());
    };
    if let Err(error) = record(outcome) {
        diagnostics::emit(
            container_id,
            diagnostics::AttachOutcomeDiagnostic::RecordingFailed(&error),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
