// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Non-TUI instance discovery services.

use std::collections::{HashMap, HashSet};

use anyhow::Context;
use jackin_console::tui::state::ManagerInstanceRefreshSnapshot;
use jackin_console::tui::subscriptions::instance_refresh_interval;
use jackin_runtime::runtime::snapshot::SnapshotTransport;

type SnapshotFetchResult = (
    String,
    anyhow::Result<(
        Option<jackin_runtime::runtime::snapshot::InstanceSnapshot>,
        SnapshotTransport,
    )>,
);

#[cfg(test)]
mod tests;

pub(crate) fn load_instance_refresh_snapshot(
    paths: &jackin_core::JackinPaths,
) -> Result<ManagerInstanceRefreshSnapshot, String> {
    let index = jackin_runtime::instance::InstanceIndex::read_or_rebuild(&paths.data_dir)
        .map_err(|error| error.to_string())?;
    let mut instances = index.instances;
    let running = running_role_containers_for_refresh(paths, &mut instances);
    let running_filter = running
        .as_ref()
        .map(|containers| containers.iter().cloned().collect::<HashSet<String>>());

    let mut sessions = HashMap::new();
    let mut session_errors = HashSet::new();
    let mut admissions = HashMap::new();
    let mut snapshot_targets: Vec<String> = Vec::new();
    let mut recovered_failure = false;

    for entry in &instances {
        if is_live_instance_status(entry.status)
            && !record_live_manifest(paths, &entry.container_base, &mut admissions, &mut sessions)
        {
            recovered_failure = true;
            session_errors.insert(entry.container_base.clone());
        }
        if should_snapshot_instance(entry, running_filter.as_ref()) {
            snapshot_targets.push(entry.container_base.clone());
        }
    }

    let mut snapshots = HashMap::new();
    let mut exec_fallback_seen = false;
    let snapshot_results = fetch_snapshots_parallel(paths, &snapshot_targets);
    for (container, result) in snapshot_results {
        recovered_failure |=
            apply_snapshot_result(container, result, &mut snapshots, &mut exec_fallback_seen);
    }

    if recovered_failure {
        let _event = jackin_telemetry::record_recovered_degradation();
    }

    Ok(ManagerInstanceRefreshSnapshot {
        instances,
        sessions,
        session_errors,
        admissions,
        snapshots,
        next_interval: instance_refresh_interval(exec_fallback_seen),
    })
}

fn record_live_manifest(
    paths: &jackin_core::JackinPaths,
    container_base: &str,
    admissions: &mut HashMap<String, Vec<jackin_console::services::launch::LiveInstanceAdmission>>,
    sessions: &mut HashMap<String, Vec<jackin_core::SessionRecord>>,
) -> bool {
    let Ok(manifest) =
        jackin_runtime::instance::InstanceManifest::read(&paths.data_dir.join(container_base))
    else {
        return false;
    };

    admissions.insert(
        container_base.to_owned(),
        manifest
            .admitted_instances
            .iter()
            .map(live_instance_admission)
            .collect(),
    );
    if !manifest.sessions.is_empty() {
        sessions.insert(container_base.to_owned(), manifest.sessions);
    }
    true
}

fn live_instance_admission(
    admitted: &jackin_runtime::instance::AdmittedInstance,
) -> jackin_console::services::launch::LiveInstanceAdmission {
    jackin_console::services::launch::LiveInstanceAdmission {
        instance_id: admitted.config_id.clone(),
        agent: admitted.agent,
        account_id: admitted.account_id.clone(),
    }
}

pub(crate) fn running_role_containers() -> anyhow::Result<Vec<String>> {
    let request = jackin_process::ExecRequest::new(
        "docker",
        [
            "ps",
            "--filter",
            "label=jackin.kind=role",
            "--format",
            "{{.Names}}",
        ],
    );
    // Instance refresh is launched through spawn_blocking_subscription;
    // keep the docker listing on the shared process transport.
    let output = crate::process_telemetry::exec_sync(&request)
        .context("starting live instance reconciliation")?;
    anyhow::ensure!(output.success, "live instance reconciliation failed");
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn running_role_containers_for_refresh(
    paths: &jackin_core::JackinPaths,
    instances: &mut Vec<jackin_runtime::instance::InstanceIndexEntry>,
) -> Option<Vec<String>> {
    let running = match running_role_containers() {
        Ok(running) => running,
        Err(error) => {
            jackin_diagnostics::emit_compact_line(
                "error",
                &live_instance_reconciliation_error_line(&format!("{error:#}")),
            );
            return None;
        }
    };
    overlay_running_instances(paths, instances, &running);
    Some(running)
}

fn is_live_instance_status(status: jackin_runtime::instance::InstanceStatus) -> bool {
    matches!(
        status,
        jackin_runtime::instance::InstanceStatus::Active
            | jackin_runtime::instance::InstanceStatus::Running
    )
}

fn should_snapshot_instance(
    entry: &jackin_runtime::instance::InstanceIndexEntry,
    running_containers: Option<&HashSet<String>>,
) -> bool {
    is_live_instance_status(entry.status)
        && running_containers.is_none_or(|running| running.contains(&entry.container_base))
}

fn live_instance_reconciliation_error_line(error: &str) -> String {
    format!("jackin: error: live instance reconciliation skipped: docker ps failed: {error}")
}

pub(crate) fn overlay_running_instances(
    paths: &jackin_core::JackinPaths,
    instances: &mut Vec<jackin_runtime::instance::InstanceIndexEntry>,
    running_containers: &[String],
) {
    if running_containers.is_empty() {
        return;
    }

    let mut known: HashSet<String> = instances
        .iter()
        .map(|entry| entry.container_base.clone())
        .collect();
    for container in running_containers {
        if let Some(entry) = instances
            .iter_mut()
            .find(|entry| entry.container_base == *container)
        {
            entry.status = jackin_runtime::instance::InstanceStatus::Running;
            continue;
        }

        let state_dir = paths.data_dir.join(container);
        let Some(manifest) =
            jackin_runtime::instance::InstanceManifest::read_optional_lossy(&state_dir)
        else {
            continue;
        };
        if !known.insert(container.clone()) {
            continue;
        }
        let mut entry = manifest.to_index_entry();
        entry.status = jackin_runtime::instance::InstanceStatus::Running;
        instances.push(entry);
    }
}

fn apply_snapshot_result(
    container: String,
    result: anyhow::Result<(
        Option<jackin_runtime::runtime::snapshot::InstanceSnapshot>,
        SnapshotTransport,
    )>,
    snapshots: &mut HashMap<String, jackin_runtime::runtime::snapshot::InstanceSnapshot>,
    exec_fallback_seen: &mut bool,
) -> bool {
    let Ok((snapshot, transport)) = result else {
        return true;
    };

    *exec_fallback_seen |= transport == SnapshotTransport::DockerExecFallback;
    if let Some(snapshot) = snapshot {
        snapshots.insert(container, snapshot);
    }
    false
}

fn fetch_snapshots_parallel(
    paths: &jackin_core::JackinPaths,
    targets: &[String],
) -> Vec<SnapshotFetchResult> {
    const SNAPSHOT_FANOUT_CHUNK: usize = 8;
    let mut results = Vec::with_capacity(targets.len());
    for chunk in targets.chunks(SNAPSHOT_FANOUT_CHUNK) {
        results.extend(fetch_snapshot_chunk(paths, chunk));
    }
    results
}

fn fetch_snapshot_chunk(
    paths: &jackin_core::JackinPaths,
    chunk: &[String],
) -> Vec<SnapshotFetchResult> {
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(chunk.len());
        for container in chunk {
            let container = container.clone();
            handles.push(jackin_telemetry::spawn::thread_scoped_joined(
                scope,
                move || fetch_snapshot_for_container(paths, container),
            ));
        }
        handles.into_iter().map(join_snapshot_worker).collect()
    })
}

fn fetch_snapshot_for_container(
    paths: &jackin_core::JackinPaths,
    container: String,
) -> SnapshotFetchResult {
    let result = resolve_snapshot_container(&container).and_then(|handle| {
        jackin_runtime::runtime::snapshot::fetch_snapshot_with_transport(paths, &handle)
    });
    (container, result)
}

fn join_snapshot_worker(
    handle: std::thread::ScopedJoinHandle<'_, SnapshotFetchResult>,
) -> SnapshotFetchResult {
    handle.join().unwrap_or_else(|panic_payload| {
        let detail = panic_payload
            .downcast_ref::<&'static str>()
            .map(|s| (*s).to_owned())
            .or_else(|| panic_payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_owned());
        (
            "<unknown-container>".to_owned(),
            Err(anyhow::anyhow!("snapshot worker thread panicked: {detail}")),
        )
    })
}

fn resolve_snapshot_container(
    container_name: &str,
) -> anyhow::Result<jackin_core::ContainerHandle> {
    let request = jackin_process::ExecRequest::new(
        "docker",
        ["inspect", "--format", "{{.ID}}\\t{{.Name}}", container_name],
    );
    let output = crate::process_telemetry::exec_sync(&request)
        .context("resolving immutable container identity for snapshot")?;
    anyhow::ensure!(
        output.success,
        "docker inspect failed while resolving snapshot container {container_name}"
    );
    let line = String::from_utf8_lossy(&output.stdout);
    let (id, raw_name) = line
        .lines()
        .map(str::trim)
        .find_map(|line| line.split_once('\t'))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "docker inspect returned no identity for snapshot container {container_name}"
            )
        })?;
    let name = raw_name.trim_start_matches('/');
    anyhow::ensure!(
        name == container_name,
        "docker inspect identity changed for snapshot container {container_name}: {name}"
    );
    jackin_core::ContainerHandle::new(name, id)
}
