// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sibling runtime/image prewarm spawning.

use jackin_core::Agent;

use jackin_core::JackinPaths;
use jackin_core::RoleSelector;

#[cfg(not(test))]
use jackin_telemetry::spawn::JoinSetExt as _;

#[cfg(not(test))]
use super::{SiblingImagePrewarmOutcome, prewarm_sibling_image};
use super::{agent_binary_prepare_summary, prepare_agent_binaries, sibling_agents};

pub(crate) fn spawn_sibling_runtime_prewarm(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
    selected_image_reused: bool,
) -> Option<tokio::task::JoinHandle<jackin_telemetry::spawn::DetachedCompletion>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    let siblings = validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect::<Vec<_>>();
    if siblings.is_empty() {
        if let Some(run) = &active_run {
            run.stage(
                "runtime_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::AgentBinaries,
                "no sibling runtime binaries to prewarm",
                Some(selected_agent.slug()),
            );
        }
        return None;
    }
    if !selected_image_reused {
        if let Some(run) = &active_run {
            run.stage(
                "runtime_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::AgentBinaries,
                "selected image was rebuilt; skipping sibling runtime binary prewarm to avoid competing with foreground launch",
                Some(selected_agent.slug()),
            );
        }
        return None;
    }

    let paths = paths.clone();
    let agents = siblings
        .iter()
        .map(|agent| agent.slug())
        .collect::<Vec<_>>()
        .join(",");
    if let Some(run) = &active_run {
        let reason = format!("sibling_runtime_prewarm:{agents}");
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
    }
    Some(jackin_telemetry::spawn::spawn_prewarm_job(
        jackin_telemetry::schema::enums::JobType::ImagePrewarm,
        async move {
            if let Some(run) = &active_run {
                run.stage(
                    "runtime_prewarm_started",
                    jackin_diagnostics::DiagnosticStage::AgentBinaries,
                    "prewarming sibling runtime binaries",
                    Some(&agents),
                );
            }
            if let Some(run) = &active_run {
                run.timing_started(
                    jackin_diagnostics::DiagnosticStage::AgentBinaries,
                    "sibling_runtime_prewarm",
                    Some(&agents),
                );
            }
            let result = prepare_agent_binaries(
                &paths,
                &siblings,
                jackin_diagnostics::DiagnosticStage::AgentBinaries,
                false,
            )
            .await;
            let timing_detail = match &result {
                Ok(prepared) => agent_binary_prepare_summary(prepared),
                Err(error) => format!("failed: {error:#}"),
            };
            if let Some(run) = &active_run {
                run.timing_done(
                    jackin_diagnostics::DiagnosticStage::AgentBinaries,
                    "sibling_runtime_prewarm",
                    Some(&timing_detail),
                );
            }
            if let Some(run) = active_run {
                match &result {
                    Ok(prepared) => run.stage(
                        "runtime_prewarm_done",
                        jackin_diagnostics::DiagnosticStage::AgentBinaries,
                        "prewarmed sibling runtime binaries",
                        Some(&agent_binary_prepare_summary(prepared)),
                    ),
                    Err(error) => run.stage(
                        "runtime_prewarm_failed",
                        jackin_diagnostics::DiagnosticStage::AgentBinaries,
                        "sibling runtime binary prewarm failed",
                        Some(&format!("{error:#}")),
                    ),
                }
            }
            if result.is_ok() {
                jackin_telemetry::spawn::DetachedCompletion::success()
            } else {
                jackin_telemetry::spawn::DetachedCompletion::failure(
                    jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
                )
            }
        },
        |completion| *completion,
    ))
}

pub(crate) fn spawn_sibling_image_prewarm(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
    selected_image_reused: bool,
) {
    let siblings = sibling_agents(validated_repo, selected_agent);
    if siblings.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "no sibling runtime images to prewarm",
                Some(selected_agent.slug()),
            );
        }
        return;
    }
    if !selected_image_reused {
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "selected image was rebuilt; skipping sibling image prewarm to avoid competing with foreground launch",
                Some(selected_agent.slug()),
            );
        }
        return;
    }

    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "sibling_image_prewarm_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "sibling runtime image prewarm disabled in unit tests",
                Some(selected_agent.slug()),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        jackin_telemetry::spawn::spawn_prewarm_job(
            jackin_telemetry::schema::enums::JobType::ImagePrewarm,
            async move {
                let agents = siblings
                    .iter()
                    .map(|agent| agent.slug())
                    .collect::<Vec<_>>()
                    .join(",");
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.stage(
                        "sibling_image_prewarm_started",
                        jackin_diagnostics::DiagnosticStage::DerivedImage,
                        "prewarming sibling runtime images",
                        Some(&agents),
                    );
                }

                jackin_diagnostics::active_timing_started(
                    jackin_diagnostics::DiagnosticStage::DerivedImage,
                    "sibling_image_prewarm",
                    Some(&agents),
                );
                let (built, reused, failed) = prewarm_sibling_images_concurrently(
                    paths,
                    selector,
                    role_git,
                    branch_override,
                    siblings,
                )
                .await;
                let timing_detail = if failed.is_empty() {
                    format!("built={built}; reused={reused}")
                } else {
                    format!("built={built}; reused={reused}; failed={}", failed.len())
                };
                jackin_diagnostics::active_timing_done(
                    jackin_diagnostics::DiagnosticStage::DerivedImage,
                    "sibling_image_prewarm",
                    Some(&timing_detail),
                );
                if let Some(run) = jackin_diagnostics::active_run() {
                    if failed.is_empty() {
                        run.stage(
                            "sibling_image_prewarm_done",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "prewarmed sibling runtime images",
                            Some(&format!("built={built}; reused={reused}")),
                        );
                    } else {
                        run.stage(
                            "sibling_image_prewarm_failed",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "sibling runtime image prewarm finished with failures",
                            Some(&failed.join("; ")),
                        );
                    }
                }
                if failed.is_empty() {
                    jackin_telemetry::spawn::DetachedCompletion::success()
                } else {
                    jackin_telemetry::spawn::DetachedCompletion::failure(
                        jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
                    )
                }
            },
            |completion| *completion,
        );
    }
}

#[cfg(not(test))]
pub(crate) async fn prewarm_sibling_images_concurrently(
    paths: JackinPaths,
    selector: RoleSelector,
    role_git: String,
    branch_override: Option<String>,
    siblings: Vec<Agent>,
) -> (usize, usize, Vec<String>) {
    let mut built = 0usize;
    let mut reused = 0usize;
    let mut failed = Vec::new();
    let mut tasks = tokio::task::JoinSet::new();
    for sibling in siblings {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.clone();
        let branch_override = branch_override.clone();
        tasks.spawn_joined_on(async move {
            let result = prewarm_sibling_image(
                &paths,
                &selector,
                &role_git,
                branch_override.as_deref(),
                sibling,
            )
            .await;
            (sibling, result)
        });
    }

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((_, Ok(SiblingImagePrewarmOutcome::Reused))) => reused += 1,
            Ok((_, Ok(SiblingImagePrewarmOutcome::Built))) => built += 1,
            Ok((sibling, Err(error))) => {
                failed.push(format!("{}: {error:#}", sibling.slug()));
            }
            Err(error) => failed.push(format!("task: {error:#}")),
        }
    }
    failed.sort();
    (built, reused, failed)
}
