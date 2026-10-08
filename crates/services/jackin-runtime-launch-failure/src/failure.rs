// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch failure title, diagnosis, CLI error, and role-source resolve.
//!
//! Pure rendering over a [`LaunchStage`] plus the failure itself: the title
//! names the failed stage, the diagnosis is the error's first chain line,
//! and the CLI error passes the failure through untouched (failures arrive
//! fully rendered; wrapping would only double prefixes). Role-source
//! resolution lives here too — the pipeline resolves it on the failure path
//! next to the rendering. The exit outro rendering moved to
//! `jackin-runtime-launch-exit-outro` (S7 split 95, observation-inversion:
//! the hub keeps the universe-boundary observation and the leaf renders
//! from the already-observed outcome).

use jackin_config::AppConfig;
use jackin_core::RoleSelector;
use jackin_diagnostics;
use jackin_runtime_progress::progress::LaunchStage;

pub fn launch_failure_title(
    stage: LaunchStage,
    error: &anyhow::Error,
    _run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == LaunchStage::DerivedImage {
        return "Docker build failed".to_owned();
    }
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("docker") {
        "Docker unavailable".to_owned()
    } else if text.contains("credential") || text.contains("token") || text.contains("auth") {
        "Credential check failed".to_owned()
    } else {
        "Launch failed".to_owned()
    }
}

pub fn short_launch_diagnosis(
    stage: LaunchStage,
    error: &anyhow::Error,
    _run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == LaunchStage::DerivedImage {
        return "Building the Docker container failed.".to_owned();
    }
    error
        .chain()
        .next()
        .map_or_else(|| "launch did not complete".to_owned(), ToString::to_string)
}

pub fn launch_failure_cli_error(
    stage: LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> anyhow::Error {
    // Rendering stays here (next to the stage-specific title/diagnosis),
    // but the error itself passes through untouched: failures arrive
    // fully rendered (`DockerBuildFailed` carries the redacted build
    // stderr tail), and wrapping would only double prefixes or add paths.
    let _ = (stage, run);
    anyhow::anyhow!("{error:#}")
}

pub fn resolve_launch_role_source(
    config: &mut AppConfig,
    selector: &RoleSelector,
    restore_role_source_git: Option<&str>,
) -> anyhow::Result<(jackin_config::RoleSource, bool, bool)> {
    if let Some(git) = restore_role_source_git {
        let mut source = config
            .roles
            .get(&selector.key())
            .cloned()
            .unwrap_or_default();
        source.git = git.to_owned();
        source.trusted = true;
        return Ok((source, false, true));
    }
    let (source, is_new) = config.resolve_role_source(selector)?;
    Ok((source, is_new, false))
}
