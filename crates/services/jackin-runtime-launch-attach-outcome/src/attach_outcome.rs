// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Instance-status writes and attach-outcome records for instance manifests.

use jackin_core::JackinPaths;
use jackin_instance::{InstanceIndex, InstanceManifest, InstanceStatus};
use jackin_runtime_isolation::isolation::finalize::AttachOutcome;
use std::path::Path;

pub fn write_instance_status(
    paths: &JackinPaths,
    state_dir: &Path,
    manifest: &mut InstanceManifest,
    status: InstanceStatus,
) -> anyhow::Result<()> {
    manifest.mark_status(status);
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

pub fn write_instance_attach_outcome(
    paths: &JackinPaths,
    state_dir: &Path,
    manifest: &mut InstanceManifest,
    outcome: AttachOutcome,
) -> anyhow::Result<()> {
    if matches!(outcome, AttachOutcome::StillRunning) {
        manifest.mark_status(InstanceStatus::Running);
    } else {
        manifest.touch();
    }
    manifest.last_attach_outcome = Some(format_attach_outcome(outcome));
    manifest.write(state_dir)?;
    InstanceIndex::update_manifest(&paths.data_dir, manifest)?;
    Ok(())
}

pub fn record_instance_attach_outcome(
    paths: &JackinPaths,
    container_name: &str,
    outcome: AttachOutcome,
) -> anyhow::Result<()> {
    let state_dir = paths.data_dir.join(container_name);
    // Missing manifest is a legitimate no-op; corrupt manifest is
    // logged so the attach-outcome record is not silently dropped.
    let Some(mut manifest) = InstanceManifest::read_optional_lossy(&state_dir) else {
        return Ok(());
    };
    write_instance_attach_outcome(paths, &state_dir, &mut manifest, outcome)
}

pub fn format_attach_outcome(outcome: AttachOutcome) -> String {
    match outcome {
        AttachOutcome::OomKilled => "oom_killed".to_owned(),
        AttachOutcome::StillRunning => "running".to_owned(),
        AttachOutcome::Stopped(code) => format!("exit:{code}"),
    }
}
