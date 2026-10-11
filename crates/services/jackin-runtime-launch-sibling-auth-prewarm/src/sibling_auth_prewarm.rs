// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Sibling-agent auth prewarm for a launch role.
//!
//! The selected agent owns the foreground launch; every other agent the
//! role manifest supports gets its auth slot prewarmed on the blocking
//! pool while the pipeline continues. The await phase must run before
//! `RoleState::prepare_for_bindings` (which acquires the leases that
//! protect paths mounted into the live container), so each phase keeps
//! one owner and a canceled blocking worker stays safe to detach.
//!
//! Split out of `jackin-runtime` (S7 split 97); the old
//! `jackin_runtime::runtime::launch::spawn_sibling_auth_prewarm` path
//! keeps working through the hub re-export.

use anyhow::Context;
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_instance::RoleState;

#[derive(Debug)]
pub struct SiblingAuthPrewarm<'a> {
    pub manifest: &'a jackin_manifest::RoleManifest,
    pub config: &'a AppConfig,
    pub workspace_name: &'a str,
    pub role_key: &'a str,
}

pub fn spawn_sibling_auth_prewarm(
    paths: &JackinPaths,
    container_name: &str,
    prewarm: &SiblingAuthPrewarm<'_>,
    selected_agent: jackin_core::Agent,
) -> Option<tokio::task::JoinHandle<()>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    let sibling_agents = prewarm
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect::<Vec<_>>();
    if sibling_agents.is_empty() {
        if let Some(run) = &active_run {
            run.compact(
                "sibling_auth_prewarm_skipped",
                &format!("no sibling agents for selected agent {selected_agent}"),
            );
        }
        return None;
    }

    let paths_owned = paths.clone();
    let home_dir = paths.home_dir.clone();
    let container_name = container_name.to_owned();
    let config = prewarm.config.clone();
    let workspace_name = prewarm.workspace_name.to_owned();
    let role_key = prewarm.role_key.to_owned();
    let agents = sibling_agents
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if let Some(run) = &active_run {
        run.compact(
            "sibling_auth_prewarm_started",
            &format!(
                "prewarming {} sibling auth slots for selected agent {selected_agent}: {}",
                sibling_agents.len(),
                agents.join(", ")
            ),
        );
    }
    let timing_detail = agents.join(",");
    if let Some(run) = &active_run {
        let reason = format!("sibling_auth_prewarm:{timing_detail}");
        let detail = serde_json::json!({
            "plan": "PrewarmOnly",
            "reason": reason,
            "container": null,
        })
        .to_string();
        run.stage(
            "launch_plan",
            jackin_diagnostics::DiagnosticStage::Restore,
            "selected launch plan PrewarmOnly",
            Some(&detail),
        );
        run.timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "sibling_auth_prewarm",
            Some(&timing_detail),
        );
    }

    Some(spawn_auth_prewarm_worker(move || {
        let ws = jackin_core::WorkspaceName::parse(&workspace_name).ok();
        let instances: Vec<jackin_config::ResolvedInstance> =
            match jackin_config::resolve_launch(&config, ws.as_ref(), &role_key, None, None) {
                Ok(instances) => instances
                    .into_iter()
                    .filter(|instance| sibling_agents.contains(&instance.agent))
                    .collect(),
                Err(error) => {
                    if let Some(run) = active_run {
                        run.compact("sibling_auth_prewarm_failed", &error.to_string());
                    }
                    return;
                }
            };
        // Sibling instances keep their config-ID keys so prewarm lands in
        // the same slots the foreground prepare will own; sibling agents
        // without instances get a placeholder Ignore binding each.
        let mut bindings =
            match jackin_runtime_launch_capsule_setup::capsule_setup::instance_auth_bindings(
                &config, &instances,
            ) {
                Ok(bindings) => bindings,
                Err(error) => {
                    if let Some(run) = active_run {
                        run.compact("sibling_auth_prewarm_failed", &error.to_string());
                    }
                    return;
                }
            };
        for agent in &sibling_agents {
            if !instances.iter().any(|instance| instance.agent == *agent) {
                bindings.push(jackin_instance::InstanceAuthBinding::new(
                    "default",
                    *agent,
                    jackin_config::AuthForwardMode::Ignore,
                    None,
                ));
            }
        }
        let result = RoleState::prewarm_auth_for_bindings(
            &paths_owned,
            &container_name,
            &bindings,
            &home_dir,
        );
        let timing_done = match &result {
            Ok(count) => format!("{count} slots"),
            Err(error) => format!("failed: {error}"),
        };
        if let Some(run) = &active_run {
            run.timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "sibling_auth_prewarm",
                Some(&timing_done),
            );
        }

        if let Some(run) = active_run {
            match result {
                Ok(count) => run.compact(
                    "sibling_auth_prewarm_done",
                    &format!(
                        "prewarmed {count} sibling auth slots for selected agent {selected_agent}"
                    ),
                ),
                Err(error) => run.compact(
                    "sibling_auth_prewarm_failed",
                    &format!(
                        "sibling auth prewarm failed for selected agent {selected_agent}: {error}"
                    ),
                ),
            }
        }
    }))
}

/// Run auth prewarm on the blocking pool. A running `spawn_blocking` task
/// cannot be aborted; if launch cancellation drops its join handle, the
/// worker still finishes its own writes before releasing their short-lived
/// target locks.
pub fn spawn_auth_prewarm_worker<F, R>(work: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    jackin_telemetry::spawn::joined_blocking(work)
}

/// Wait for sibling auth writes before role-state mount admission.
///
/// This phase must run before `RoleState::prepare_for_bindings`: that method
/// acquires the leases that protect paths mounted into the live container.
/// Keeping prewarm before admission gives each phase one owner and makes a
/// canceled blocking worker safe to detach without cloning those leases.
pub async fn await_sibling_auth_prewarm(
    prewarm: Option<tokio::task::JoinHandle<()>>,
) -> anyhow::Result<()> {
    if let Some(prewarm) = prewarm {
        prewarm
            .await
            .context("sibling auth prewarm task panicked")?;
    }
    Ok(())
}
