//! Non-TUI instance discovery services.

use std::collections::{HashMap, HashSet};

use crate::console::domain::InstanceRefreshSnapshot;
use anyhow::Context;

pub(crate) fn load_instance_refresh_snapshot(
    paths: &crate::paths::JackinPaths,
) -> Result<InstanceRefreshSnapshot, String> {
    let index = crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir)
        .map_err(|error| error.to_string())?;
    let mut instances = index.instances;
    reconcile_live_running_instances(paths, &mut instances);

    let mut sessions = HashMap::new();
    let mut session_errors = HashSet::new();
    let mut snapshot_targets: Vec<String> = Vec::new();

    for entry in &instances {
        if matches!(
            entry.status,
            crate::instance::InstanceStatus::Active | crate::instance::InstanceStatus::Running
        ) {
            let state_dir = paths.data_dir.join(&entry.container_base);
            match crate::instance::InstanceManifest::read(&state_dir) {
                Ok(manifest) if !manifest.sessions.is_empty() => {
                    sessions.insert(entry.container_base.clone(), manifest.sessions);
                }
                Ok(_) => {}
                Err(e) => {
                    crate::debug_log!(
                        "console",
                        "manifest read failed for {}: {e:#}",
                        entry.container_base
                    );
                    session_errors.insert(entry.container_base.clone());
                }
            }
            snapshot_targets.push(entry.container_base.clone());
        }
    }

    let mut snapshots = HashMap::new();
    let snapshot_results = fetch_snapshots_parallel(paths, &snapshot_targets);
    for (container, result) in snapshot_results {
        match result {
            Ok(Some(snapshot)) => {
                snapshots.insert(container, snapshot);
            }
            Ok(None) => {}
            Err(e) => {
                crate::debug_log!("console", "snapshot fetch failed for {container}: {e:#}");
            }
        }
    }

    Ok(InstanceRefreshSnapshot {
        instances,
        sessions,
        session_errors,
        snapshots,
    })
}

/// Return running role container names from the local Docker CLI.
pub(crate) fn running_role_containers() -> anyhow::Result<Vec<String>> {
    let mut command = std::process::Command::new("docker");
    command.args([
        "ps",
        "--filter",
        "label=jackin.kind=role",
        "--format",
        "{{.Names}}",
    ]);
    #[expect(
        clippy::disallowed_methods,
        reason = "instance refresh is launched through spawn_blocking_subscription"
    )]
    let output = command
        .output()
        .map_err(anyhow::Error::new)
        .context("starting docker ps for live instance reconciliation")?;
    anyhow::ensure!(
        output.status.success(),
        "docker ps exited with status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn reconcile_live_running_instances(
    paths: &crate::paths::JackinPaths,
    instances: &mut Vec<crate::instance::InstanceIndexEntry>,
) {
    let running = match running_role_containers() {
        Ok(running) => running,
        Err(error) => {
            jackin_diagnostics::emit_compact_line(
                "error",
                &live_instance_reconciliation_error_line(&format!("{error:#}")),
            );
            return;
        }
    };
    overlay_running_instances(paths, instances, &running);
}

fn live_instance_reconciliation_error_line(error: &str) -> String {
    format!("jackin: error: live instance reconciliation skipped: docker ps failed: {error}")
}

pub(crate) fn overlay_running_instances(
    paths: &crate::paths::JackinPaths,
    instances: &mut Vec<crate::instance::InstanceIndexEntry>,
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
            entry.status = crate::instance::InstanceStatus::Running;
            continue;
        }

        let state_dir = paths.data_dir.join(container);
        let Some(manifest) =
            crate::instance::InstanceManifest::read_or_log(&state_dir, "overlay_running_instances")
        else {
            continue;
        };
        if !known.insert(container.clone()) {
            continue;
        }
        let mut entry = manifest.to_index_entry();
        entry.status = crate::instance::InstanceStatus::Running;
        instances.push(entry);
    }
}

/// Fan-out snapshot fetches in parallel so wall-clock cost stays bounded by the
/// per-fetch socket timeout instead of serializing across active instances.
fn fetch_snapshots_parallel(
    paths: &crate::paths::JackinPaths,
    targets: &[String],
) -> Vec<(
    String,
    anyhow::Result<Option<crate::runtime::snapshot::InstanceSnapshot>>,
)> {
    const SNAPSHOT_FANOUT_CHUNK: usize = 8;
    let mut results = Vec::with_capacity(targets.len());
    for chunk in targets.chunks(SNAPSHOT_FANOUT_CHUNK) {
        let chunk_results = std::thread::scope(|s| {
            // Collect handles first so every thread starts before any join blocks.
            #[allow(clippy::needless_collect)]
            let handles: Vec<_> = chunk
                .iter()
                .map(|container| {
                    let container = container.clone();
                    s.spawn(move || {
                        let result = crate::runtime::snapshot::fetch_snapshot(paths, &container);
                        (container, result)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| match h.join() {
                    Ok(pair) => pair,
                    Err(panic_payload) => {
                        let detail = panic_payload
                            .downcast_ref::<&'static str>()
                            .map(|s| (*s).to_owned())
                            .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "<non-string panic payload>".to_owned());
                        (
                            "<unknown-container>".to_owned(),
                            Err(anyhow::anyhow!("snapshot worker thread panicked: {detail}")),
                        )
                    }
                })
                .collect::<Vec<_>>()
        });
        results.extend(chunk_results);
    }
    results
}

#[cfg(test)]
mod tests {
    #[test]
    fn live_instance_reconciliation_error_is_operator_visible() {
        let line =
            super::live_instance_reconciliation_error_line("failed to connect to the docker API");

        assert_eq!(
            line,
            "jackin: error: live instance reconciliation skipped: docker ps failed: \
             failed to connect to the docker API"
        );
        assert!(!line.contains("[jackin debug console]"));
    }
}
