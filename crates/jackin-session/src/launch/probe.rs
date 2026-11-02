// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    HookKind, LaunchId, SandboxName, SupervisorReading, SupervisorState, sandbox_layout,
};
use jackin_openshell::Gateway;

use crate::launch::LaunchError;

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

pub(crate) fn reading(bytes: Option<&[u8]>) -> SupervisorReading {
    let Some(bytes) = bytes else {
        return SupervisorReading::Unreadable;
    };
    match std::str::from_utf8(bytes) {
        Ok(text) => match SupervisorState::parse(text) {
            Ok(state) => SupervisorReading::Reported(state),
            Err(error) => SupervisorReading::Invalid(error.to_string()),
        },
        Err(_) => SupervisorReading::Invalid("the state is not valid UTF-8".to_owned()),
    }
}

pub(super) async fn read(
    gateway: &dyn Gateway,
    sandbox: &SandboxName,
    launch: &LaunchId,
) -> Result<SandboxProbe, LaunchError> {
    let paths = [
        sandbox_layout::uploaded_marker(launch),
        sandbox_layout::setup_done_marker(launch),
        sandbox_layout::hook_path(HookKind::Setup),
        sandbox_layout::hook_path(HookKind::Preflight),
        sandbox_layout::RUN_STATE.clone(),
    ];
    let found = gateway.read_files(sandbox, &paths).await?;
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
