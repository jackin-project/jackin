// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
