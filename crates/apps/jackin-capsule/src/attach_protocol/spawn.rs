// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Initial spawn request construction.

use crate::protocol::attach::SpawnRequest;

pub(crate) fn initial_spawn_request(initial_agent: &str) -> SpawnRequest {
    if initial_agent.is_empty() {
        SpawnRequest::Shell
    } else {
        SpawnRequest::Instance(initial_agent.to_owned())
    }
}

/// Boot tabs for a fresh daemon: the initial instance first, then every
/// other launch instance in config order, so `default_launch` selects the
/// instances a launch starts. Shell-only launches keep the single initial
/// spawn and never invent agent tabs.
pub(crate) fn initial_spawn_requests(
    initial_agent: &str,
    launch_config: &jackin_protocol::CapsuleConfig,
) -> Vec<SpawnRequest> {
    let initial = initial_spawn_request(initial_agent);
    let mut requests = vec![initial.clone()];
    if let SpawnRequest::Instance(initial_id) = &initial {
        requests.extend(
            launch_config
                .instances
                .iter()
                .filter(|id| *id != initial_id)
                .map(|id| SpawnRequest::Instance(id.clone())),
        );
    }
    requests
}

pub(crate) fn spawn_request_label(request: &SpawnRequest) -> String {
    match request {
        SpawnRequest::Instance(target) => format!("instance {target:?}"),
        SpawnRequest::Shell => "shell".to_owned(),
    }
}
