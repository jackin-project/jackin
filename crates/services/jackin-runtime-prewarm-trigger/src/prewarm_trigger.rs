// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
#[cfg(not(test))]
use jackin_docker::docker_client::BollardDockerClient;
#[cfg(not(test))]
use jackin_runtime_image::image::prewarm_role_images;
#[cfg(not(test))]
use jackin_runtime_launch_dind::launch_dind::{
    prewarm_dind_sidecar_container_with_paths, prewarmed_dind_state_is_live,
    try_lock_prewarmed_dind, write_prewarmed_dind_state,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundPrewarmTarget {
    pub selector: RoleSelector,
    pub role_git: String,
    pub agents: Vec<Agent>,
}

#[must_use]
#[expect(
    clippy::excessive_nesting,
    reason = "Pre-warm target resolution: per-role + per-mount nested with \
              per-`Vec` containment checks. The nesting is the per-target \
              dedup protocol."
)]
pub fn background_prewarm_targets(config: &AppConfig) -> Vec<BackgroundPrewarmTarget> {
    let mut targets: std::collections::BTreeMap<String, BackgroundPrewarmTarget> =
        std::collections::BTreeMap::new();
    for workspace in config.workspaces.values() {
        let Some(role) = workspace.default_role.as_deref() else {
            continue;
        };
        let Ok(selector) = RoleSelector::parse(role) else {
            continue;
        };
        let Some(source) = config.roles.get(&selector.key()) else {
            continue;
        };
        let agents = workspace
            .default_agent
            .map_or_else(Vec::new, |agent| vec![agent]);
        targets
            .entry(selector.key())
            .and_modify(|existing| {
                if existing.agents.is_empty() || agents.is_empty() {
                    existing.agents.clear();
                } else {
                    for agent in &agents {
                        if !existing.agents.contains(agent) {
                            existing.agents.push(*agent);
                        }
                    }
                }
            })
            .or_insert(BackgroundPrewarmTarget {
                selector,
                role_git: source.git.clone(),
                agents,
            });
    }
    targets.into_values().collect()
}

pub fn spawn_background_image_prewarm(
    paths: &JackinPaths,
    targets: Vec<BackgroundPrewarmTarget>,
    debug: bool,
) {
    if targets.is_empty() {
        return;
    }

    #[cfg(test)]
    {
        let _ = (paths, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "background_image_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "background image prewarm disabled in unit tests",
                Some(&targets.len().to_string()),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        jackin_telemetry::spawn::spawn_prewarm_job_attempts(
            jackin_telemetry::schema::enums::JobType::ImagePrewarm,
            move |attempts| async move {
                let mut failed = false;
                for target in targets {
                    let result = attempts
                        .run(
                            prewarm_role_images(
                                &paths,
                                &target.selector,
                                &target.role_git,
                                None,
                                &target.agents,
                                debug,
                            ),
                            classify_image_prewarm_attempt,
                        )
                        .await;
                    failed |= result.is_err();
                }
                failed
            },
        );
    }
}

#[cfg(not(test))]
fn classify_image_prewarm_attempt<T, E>(
    result: &Result<T, E>,
) -> jackin_telemetry::spawn::DetachedCompletion {
    if result.is_ok() {
        jackin_telemetry::spawn::DetachedCompletion::success()
    } else {
        jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        )
    }
}

pub fn spawn_background_sidecar_prewarm(paths: &JackinPaths, debug: bool) {
    #[cfg(test)]
    {
        let _ = (paths, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "background_sidecar_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "background sidecar prewarm disabled in unit tests",
                None,
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        jackin_telemetry::spawn::spawn_prewarm_job(
            jackin_telemetry::schema::enums::JobType::SidecarPrewarm,
            async move { background_sidecar_prewarm_once(&paths, debug).await },
            classify_sidecar_prewarm_attempt,
        );
    }
}

#[derive(Debug)]
pub enum SidecarPrewarmOutcome {
    Completed,
    Skipped,
}

pub fn classify_sidecar_prewarm_attempt(
    result: &anyhow::Result<SidecarPrewarmOutcome>,
) -> jackin_telemetry::spawn::DetachedCompletion {
    match result {
        Ok(SidecarPrewarmOutcome::Completed) => {
            jackin_telemetry::spawn::DetachedCompletion::success()
        }
        Ok(SidecarPrewarmOutcome::Skipped) => jackin_telemetry::spawn::DetachedCompletion::skip(),
        Err(_) => jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        ),
    }
}

#[cfg(not(test))]
async fn background_sidecar_prewarm_once(
    paths: &JackinPaths,
    _debug: bool,
) -> anyhow::Result<SidecarPrewarmOutcome> {
    let Some(_lock) = try_lock_prewarmed_dind(paths).await else {
        return Ok(SidecarPrewarmOutcome::Skipped);
    };
    let docker = BollardDockerClient::connect()?;
    if prewarmed_dind_state_is_live(paths, &docker).await {
        return Ok(SidecarPrewarmOutcome::Skipped);
    }
    let warmed = prewarm_dind_sidecar_container_with_paths(paths, &docker, true).await?;
    write_prewarmed_dind_state(paths, &warmed)?;
    Ok(SidecarPrewarmOutcome::Completed)
}
