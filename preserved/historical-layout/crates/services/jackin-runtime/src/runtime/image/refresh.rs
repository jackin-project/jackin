// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Selected image refresh and reuse staleness checks.

use jackin_core::Agent;

use jackin_core::JackinPaths;
use jackin_core::RoleSelector;

use jackin_image::version_check;

use super::ImageInvalidationReason;
#[cfg(not(test))]
use super::prewarm_agent_image;

pub(crate) fn spawn_selected_image_refresh(
    paths: &JackinPaths,
    selector: &RoleSelector,
    role_git: &str,
    branch_override: Option<&str>,
    selected_agent: Agent,
    reason: ImageInvalidationReason,
    debug: bool,
) {
    #[cfg(test)]
    {
        let _ = (paths, selector, role_git, branch_override, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "selected_image_refresh_skipped",
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "selected image refresh disabled in unit tests",
                Some(&format!("{}:{}", selected_agent.slug(), reason.as_str())),
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
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.stage(
                        "selected_image_refresh_started",
                        jackin_diagnostics::DiagnosticStage::DerivedImage,
                        "refreshing selected runtime image in background",
                        Some(&format!("{}:{}", selected_agent.slug(), reason.as_str())),
                    );
                }

                let timing_detail = format!("{}:{}", selected_agent.slug(), reason.as_str());
                jackin_diagnostics::active_timing_started(
                    jackin_diagnostics::DiagnosticStage::DerivedImage,
                    "selected_image_refresh",
                    Some(&timing_detail),
                );
                let result = prewarm_agent_image(
                    &paths,
                    &selector,
                    &role_git,
                    branch_override.as_deref(),
                    selected_agent,
                    debug,
                )
                .await;
                let timing_done = match &result {
                    Ok(row) => format!("{}:{:?}", row.agent.slug(), row.status),
                    Err(error) => format!("{}: failed: {error:#}", selected_agent.slug()),
                };
                jackin_diagnostics::active_timing_done(
                    jackin_diagnostics::DiagnosticStage::DerivedImage,
                    "selected_image_refresh",
                    Some(&timing_done),
                );

                if let Some(run) = jackin_diagnostics::active_run() {
                    match &result {
                        Ok(row) => run.stage(
                            "selected_image_refresh_done",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "refreshed selected runtime image in background",
                            Some(&format!(
                                "{}:{:?}:{}",
                                row.agent.slug(),
                                row.status,
                                row.image
                            )),
                        ),
                        Err(error) => run.stage(
                            "selected_image_refresh_failed",
                            jackin_diagnostics::DiagnosticStage::DerivedImage,
                            "selected runtime image refresh failed",
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

pub(crate) fn reuse_needs_background_staleness_check(
    paths: &JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    image: &str,
) -> bool {
    validated_repo.manifest.published_image.is_some()
        || validated_repo
            .manifest
            .supported_agents()
            .into_iter()
            .any(|agent| version_check::stored_version(paths, agent, image).is_some())
}
