// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Reuse staleness sentinel and sibling agents.

use jackin_core::Agent;

use jackin_core::JackinPaths;
use jackin_core::RoleSelector;

#[cfg(not(test))]
use super::reuse_staleness_sentinel;

pub(crate) fn spawn_reuse_staleness_sentinel(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    selected_agent: Agent,
    image: &str,
    debug: bool,
) {
    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "reuse_staleness_sentinel_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "reuse staleness sentinel disabled in unit tests",
                Some(&format!("{}:{image}", selected_agent.slug())),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        let selector = selector.clone();
        let role_git = role_git.to_owned();
        let branch_override = branch_override.map(str::to_owned);
        let image = image.to_owned();
        jackin_telemetry::spawn::spawn_prewarm_job(
            jackin_telemetry::schema::enums::JobType::ImagePrewarm,
            async move {
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.stage(
                        "reuse_staleness_sentinel_started",
                        jackin_diagnostics::DiagnosticStage::DerivedImage,
                        "checking reused runtime image staleness in background",
                        Some(&format!("{}:{image}", selected_agent.slug())),
                    );
                }

                let result = reuse_staleness_sentinel(
                    &paths,
                    &selector,
                    &role_git,
                    branch_override.as_deref(),
                    selected_agent,
                    &image,
                    debug,
                )
                .await;

                if let Some(run) = jackin_diagnostics::active_run() {
                    match &result {
                        Ok(Some(row)) => run.stage(
                            "reuse_staleness_sentinel_done",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "refreshed reused runtime image in background",
                            Some(&format!(
                                "{}:{:?}:{}",
                                row.agent.slug(),
                                row.status,
                                row.image
                            )),
                        ),
                        Ok(None) => run.stage(
                            "reuse_staleness_sentinel_done",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "reused runtime image is still fresh",
                            Some(&format!("{}:{image}", selected_agent.slug())),
                        ),
                        Err(error) => run.stage(
                            "reuse_staleness_sentinel_failed",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "reuse staleness sentinel failed",
                            Some(&format!("{}: {error:#}", selected_agent.slug())),
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
        );
    }
}

pub(crate) fn sibling_agents(
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    selected_agent: Agent,
) -> Vec<Agent> {
    validated_repo
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect()
}

#[cfg(not(test))]
pub(crate) enum SiblingImagePrewarmOutcome {
    Reused,
    Built,
}
