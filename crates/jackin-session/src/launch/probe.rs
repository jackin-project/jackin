// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{HookKind, LaunchId, SandboxName, SupervisorReading, sandbox_layout};
use jackin_openshell::Gateway;
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;
use crate::state::reading;

#[derive(Clone, Debug)]
pub(super) struct SandboxProbe {
    pub(super) uploaded: bool,
    pub(super) setup_done: bool,
    pub(super) hooks: HookPresence,
    pub(super) supervisor: SupervisorReading,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HookPresence {
    pub(super) setup: bool,
    pub(super) preflight: bool,
}

pub(super) async fn read(
    gateway: &dyn Gateway,
    sandbox: &SandboxName,
    launch: &LaunchId,
    cancel: &CancellationToken,
) -> Result<SandboxProbe, LaunchError> {
    let paths = [
        sandbox_layout::uploaded_marker(launch),
        sandbox_layout::setup_done_marker(launch),
        sandbox_layout::hook_path(HookKind::Setup),
        sandbox_layout::hook_path(HookKind::Preflight),
        sandbox_layout::RUN_STATE.clone(),
    ];
    let found = gateway.read_files(sandbox, &paths, cancel).await?;
    let [uploaded, setup_done, setup, preflight, state] = found.as_slice() else {
        return Err(LaunchError::ProbeInvalid {
            message: format!("expected {} files, read {}", paths.len(), found.len()),
        });
    };
    let present = |file: &Option<Vec<u8>>| file.as_ref().is_some_and(|bytes| !bytes.is_empty());
    Ok(SandboxProbe {
        uploaded: uploaded.is_some(),
        setup_done: setup_done.is_some(),
        hooks: HookPresence {
            setup: present(setup),
            preflight: present(preflight),
        },
        supervisor: reading(state.as_deref()),
    })
}
