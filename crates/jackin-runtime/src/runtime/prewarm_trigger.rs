// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Background image-prewarm trigger (D22).
//!
//! Off the operator attach path, this refreshes any stale baked role images for
//! saved workspaces so a later launch reaches the `CreateFromValidImage` fast
//! path instead of paying for a foreground build. It reuses a valid image (a
//! cheap label read) and only rebuilds when the recipe or version labels are
//! stale; launch never waits on it. The synchronous `jackin prewarm` command is
//! the operator-invoked counterpart — this is the automatic background trigger.

use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
#[cfg(not(test))]
use jackin_docker::docker_client::BollardDockerClient;

/// One role image to keep warm: a role selector, its git source, and the agent
/// runtimes whose derived images to refresh. An empty `agents` means the role's
/// full supported set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundPrewarmTarget {
    pub selector: RoleSelector,
    pub role_git: String,
    pub agents: Vec<Agent>,
}

/// Resolve background-prewarm targets from saved configuration: every workspace
/// with a default role whose git source is known. A workspace default agent
/// narrows the refresh to that one runtime; a workspace without one (or two
/// workspaces sharing a role that disagree) widens the target to the role's
/// whole supported set so nothing a launch might pick is left stale.
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
                // An empty agent list on either side means "all supported" —
                // that subsumes any narrowed list, so widen to all.
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

/// Spawn a best-effort, non-blocking background sweep that refreshes any stale
/// baked images for `targets`, off the operator attach path (D22). Returns
/// immediately; a valid image is reused without a rebuild and failures are
/// swallowed (best-effort). Each refreshed image emits the `PrewarmOnly` launch
/// plan via the shared prewarm path.
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
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.stage(
                        "background_image_prewarm_started",
                        jackin_diagnostics::DiagnosticStage::DerivedImage,
                        "refreshing stale workspace images in background",
                        Some(&targets.len().to_string()),
                    );
                }
                for target in targets {
                    let result = attempts
                        .run(
                            super::image::prewarm_role_images(
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
                    if let Some(run) = jackin_diagnostics::active_run() {
                        match result {
                            #[expect(
                                clippy::excessive_nesting,
                                reason = "restored after allow→expect"
                            )]
                            Ok(rows) => {
                                let built = rows
                                    .iter()
                                    .filter(|row| {
                                        matches!(
                                            row.status,
                                            super::image::ImagePrewarmStatus::Built
                                        )
                                    })
                                    .count();
                                run.stage(
                                    "background_image_prewarm_done",
                                    jackin_diagnostics::DiagnosticStage::DerivedImage,
                                    "background workspace image refresh complete",
                                    Some(&format!(
                                        "{}:built={}/{}",
                                        target.selector.key(),
                                        built,
                                        rows.len()
                                    )),
                                );
                            }
                            Err(error) => run.stage(
                                "background_image_prewarm_failed",
                                jackin_diagnostics::DiagnosticStage::DerivedImage,
                                "background workspace image refresh failed",
                                Some(&format!("{}: {error:#}", target.selector.key())),
                            ),
                        }
                    }
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

/// Spawn a best-effort, non-blocking background task that keeps one privileged
/// `DinD` sidecar ready for a later launch to adopt. Returns immediately; the
/// helper skips when another adoption/prewarm owns the shared lock or a live
/// kept sidecar is already recorded.
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
            async move {
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.stage(
                        "background_sidecar_prewarm_started",
                        jackin_diagnostics::DiagnosticStage::Sidecar,
                        "checking for kept DinD sidecar prewarm",
                        None,
                    );
                }
                let result = background_sidecar_prewarm_once(&paths, debug).await;
                if let Some(run) = jackin_diagnostics::active_run() {
                    match &result {
                        Ok(outcome) => run.stage(
                            "background_sidecar_prewarm_done",
                            jackin_diagnostics::DiagnosticStage::Sidecar,
                            "background sidecar prewarm complete",
                            Some(outcome.detail()),
                        ),
                        Err(error) => run.stage(
                            "background_sidecar_prewarm_failed",
                            jackin_diagnostics::DiagnosticStage::Sidecar,
                            "background sidecar prewarm failed",
                            Some(&format!("{error:#}")),
                        ),
                    }
                }
                result
            },
            classify_sidecar_prewarm_attempt,
        );
    }
}

#[derive(Debug)]
enum SidecarPrewarmOutcome {
    Completed(String),
    Skipped(&'static str),
}

impl SidecarPrewarmOutcome {
    fn detail(&self) -> &str {
        match self {
            Self::Completed(detail) => detail,
            Self::Skipped(detail) => detail,
        }
    }
}

fn classify_sidecar_prewarm_attempt(
    result: &anyhow::Result<SidecarPrewarmOutcome>,
) -> jackin_telemetry::spawn::DetachedCompletion {
    match result {
        Ok(SidecarPrewarmOutcome::Completed(_)) => {
            jackin_telemetry::spawn::DetachedCompletion::success()
        }
        Ok(SidecarPrewarmOutcome::Skipped(_)) => {
            jackin_telemetry::spawn::DetachedCompletion::skip()
        }
        Err(_) => jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        ),
    }
}

#[cfg(not(test))]
async fn background_sidecar_prewarm_once(
    paths: &JackinPaths,
    debug: bool,
) -> anyhow::Result<SidecarPrewarmOutcome> {
    let Some(_lock) = super::launch::try_lock_prewarmed_dind(paths) else {
        return Ok(SidecarPrewarmOutcome::Skipped("skip:locked"));
    };
    let docker = BollardDockerClient::connect()?;
    if super::launch::prewarmed_dind_state_is_live(paths, &docker).await {
        return Ok(SidecarPrewarmOutcome::Skipped("skip:state-live"));
    }
    let warmed = super::launch::prewarm_dind_sidecar_container(&docker, true).await?;
    super::launch::write_prewarmed_dind_state(paths, &warmed)?;
    if debug {
        Ok(SidecarPrewarmOutcome::Completed(format!(
            "prewarmed:{};ready_ms={}",
            warmed.dind, warmed.ready_ms
        )))
    } else {
        Ok(SidecarPrewarmOutcome::Completed(format!(
            "prewarmed;ready_ms={}",
            warmed.ready_ms
        )))
    }
}

#[cfg(test)]
mod tests;
