// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Hardline instance inspection and state descriptions.

use jackin_instance::{InstanceManifest, RegistrationState};

use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use jackin_docker::docker_client::ContainerState;
use jackin_runtime_attach_admission::admission::validate_recorded_role_handle;
use jackin_runtime_attach_sessions::sessions::{AgentSessionInventory, inspect_agent_sessions};

pub async fn inspect_hardline_instance(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<String> {
    let state_dir = paths.data_dir.join(container_name);
    // `--inspect` is the operator's recovery tool. Distinguish "no
    // manifest yet" (pre-restore) from "manifest unreadable" (torn
    // JSON) so the render below does not lie about the latter.
    let manifest_result: Result<Option<InstanceManifest>, String> =
        InstanceManifest::read_optional(&state_dir).map_err(|e| e.to_string());
    let manifest = manifest_result.as_ref().ok().and_then(Option::as_ref);
    let resources = jackin_instance::DockerResources::from_container_name(container_name);
    let dind_name = manifest.map_or_else(
        || resources.dind_container.clone(),
        |manifest| manifest.docker.dind_container.clone(),
    );
    let network_name = manifest.as_ref().map_or_else(
        || resources.network.clone(),
        |manifest| manifest.docker.network.clone(),
    );
    let certs_volume = manifest.as_ref().map_or_else(
        || resources.certs_volume.clone(),
        |manifest| manifest.docker.certs_volume.clone(),
    );

    let (role_inspection, dind_state_raw, network_result) = tokio::join!(
        docker.inspect_container_by_name(container_name),
        async {
            if let Some(dind_name) = dind_name.as_deref() {
                let inspection = docker.inspect_container_by_name(dind_name).await;
                Some(match inspection.handle {
                    Some(handle) => docker.inspect_container_by_id(&handle).await,
                    None => inspection.state,
                })
            } else {
                None
            }
        },
        inspect_docker_network(docker, &network_name),
    );
    let role_container_state = role_inspection.state;
    let sessions = match role_inspection.handle {
        Some(container) => match validate_recorded_role_handle(paths, container_name, &container) {
            Ok(()) => inspect_agent_sessions(docker, &container, &role_container_state).await,
            Err(error) => AgentSessionInventory::Unavailable(error.to_string()),
        },
        None => AgentSessionInventory::NotRunning,
    };
    let role_state = role_container_state.inspect_label();
    let dind_state = dind_state_raw
        .as_ref()
        .map_or_else(|| "disabled".to_owned(), ContainerState::inspect_label);
    let network_state = describe_network_state(network_result);
    let mounts = describe_mount_state(&state_dir);

    let mut lines = vec![
        format!("Instance: {container_name}"),
        format!("State directory: {}", state_dir.display()),
    ];
    match &manifest_result {
        Ok(Some(manifest)) => {
            lines.extend([
                format!("Instance ID: {}", manifest.instance_id),
                format!("Workspace: {}", manifest.workspace_label),
                format!("Role: {}", manifest.role_key),
                format!("Agent: {}", manifest.agent_runtime),
                format!("Status: {}", manifest.status.label()),
                format!("Updated: {}", manifest.updated_at),
            ]);
            if let Some(outcome) = &manifest.last_attach_outcome {
                lines.push(format!("Last attach outcome: {outcome}"));
            }
            for admitted in &manifest.admitted_instances {
                if admitted.registration_state != RegistrationState::Current {
                    lines.push(format!(
                        "Registration {} ({}): {}",
                        admitted.config_id,
                        admitted.account_id,
                        admitted.registration_state.label()
                    ));
                }
            }
            if let Some(source_ref) = &manifest.role_source_ref {
                lines.push(format!(
                    "Role source: {} ({source_ref})",
                    manifest.role_source_git
                ));
            } else if !manifest.role_source_git.is_empty() {
                lines.push(format!("Role source: {}", manifest.role_source_git));
            }
        }
        Ok(None) => lines.push("Manifest: missing".to_owned()),
        Err(error) => lines.push(format!("Manifest: unreadable ({error})")),
    }

    lines.extend([
        format!("Role container: {container_name} ({role_state})"),
        format!("Agent sessions: {}", describe_agent_sessions(&sessions)),
        format!(
            "DinD container: {} ({dind_state})",
            dind_name.as_deref().unwrap_or("none")
        ),
        format!("Docker network: {network_name} ({network_state})"),
        format!(
            "DinD cert volume: {}",
            certs_volume.as_deref().unwrap_or("none")
        ),
        format!("Mounts: {mounts}"),
    ]);
    Ok(lines.join("\n"))
}

pub fn describe_agent_session_count(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "sessions:not_running".to_owned(),
        AgentSessionInventory::Unavailable(_) => "sessions:unavailable".to_owned(),
        AgentSessionInventory::Sessions(sessions) => format!("sessions:{}", sessions.len()),
    }
}

pub fn describe_agent_sessions(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "not running".to_owned(),
        AgentSessionInventory::Unavailable(reason) => format!("unavailable: {reason}"),
        AgentSessionInventory::Sessions(sessions) if sessions.is_empty() => {
            "none detected".to_owned()
        }
        AgentSessionInventory::Sessions(sessions) => sessions
            .iter()
            .map(|session| session.name.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    }
}

pub fn describe_network_state(state: DockerNetworkState) -> String {
    match state {
        DockerNetworkState::Present => "present".to_owned(),
        DockerNetworkState::NotFound => "missing".to_owned(),
        DockerNetworkState::InspectUnavailable(reason) => format!("unavailable: {reason}"),
    }
}

pub fn describe_mount_state(state_dir: &std::path::Path) -> String {
    match jackin_isolation::state::MountSummary::for_state_dir(state_dir) {
        Ok(summary) => summary.inspect_label(),
        Err(e) => format!("unknown (error reading state: {e})"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerNetworkState {
    Present,
    NotFound,
    InspectUnavailable(String),
}

pub async fn inspect_docker_network(docker: &impl DockerApi, network: &str) -> DockerNetworkState {
    match docker.inspect_network(network).await {
        Ok(Some(_)) => DockerNetworkState::Present,
        Ok(None) => DockerNetworkState::NotFound,
        Err(e) => DockerNetworkState::InspectUnavailable(e.to_string()),
    }
}

pub fn missing_restore_message(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<Option<String>> {
    let state_dir = paths.data_dir.join(container_name);
    let Some(mut manifest) = InstanceManifest::read_optional(&state_dir)? else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    manifest.mark_restore_available(paths)?;
    Ok(Some(format!(
        "container '{container_name}' is missing, but jackin-managed local state remains recoverable at {}. \
         Run `jackin load` from the matching workspace to rebuild it, or `jackin eject {container_name} --purge` \
         to discard it. Anything written only to the deleted container's writable layer is gone and will not be restored, including ad-hoc package installs, global files outside mounted paths, and DinD images.",
        state_dir.display()
    )))
}
