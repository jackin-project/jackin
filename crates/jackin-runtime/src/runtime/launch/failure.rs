//! Launch failure title, diagnosis, CLI error, source resolve, and render_exit extracted.

use jackin_diagnostics;
use jackin_docker::docker_client::DockerApi;

use crate::instance::InstanceIndex;
use crate::runtime::progress::LaunchStage;
use crate::runtime::universe::ExitClaim;
use jackin_config::AppConfig;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;

pub(crate) fn launch_failure_title(
    stage: LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == LaunchStage::DerivedImage && run.and_then(docker_build_output_artifact).is_some() {
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

pub(crate) fn short_launch_diagnosis(
    stage: LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> String {
    if stage == LaunchStage::DerivedImage && run.and_then(docker_build_output_artifact).is_some() {
        return "Building the Docker container failed.".to_owned();
    }
    error
        .chain()
        .next()
        .map_or_else(|| "launch did not complete".to_owned(), ToString::to_string)
}

pub(crate) fn docker_build_output_artifact(
    run: &jackin_diagnostics::RunDiagnostics,
) -> Option<std::path::PathBuf> {
    let docker_output = run.command_output_path("docker-build");
    docker_output.exists().then_some(docker_output)
}

pub(crate) fn launch_failure_cli_error(
    stage: LaunchStage,
    error: &anyhow::Error,
    run: Option<&jackin_diagnostics::RunDiagnostics>,
) -> anyhow::Error {
    if stage != LaunchStage::DerivedImage {
        return anyhow::anyhow!("{error:#}");
    }
    let Some(run) = run else {
        return anyhow::anyhow!("{error:#}");
    };
    let Some(docker_output) = docker_build_output_artifact(run) else {
        return anyhow::anyhow!("{error:#}");
    };
    let mut report = String::from("Docker build command failed");
    let mut table = tabled::Table::builder([
        ["run id", run.run_id()],
        ["run diagnostics", &run.path().display().to_string()],
        ["docker output", &docker_output.display().to_string()],
    ])
    .build();
    table
        .with(tabled::settings::Style::modern())
        .with(tabled::settings::Remove::row(
            tabled::settings::object::Rows::first(),
        ));
    report.push_str("\n\n");
    report.push_str(&table.to_string());
    anyhow::anyhow!("{report}")
}

pub(crate) fn resolve_launch_role_source(
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

pub(crate) async fn render_exit(paths: &JackinPaths, docker: &impl DockerApi) {
    let force_outro = crate::runtime::universe::force_boundary_outro_enabled();
    let running = match super::list_running_agent_names(docker).await {
        Ok(names) => names,
        Err(e) => {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact(
                    "exit_summary",
                    &format!("skipping boundary outro; running-container list failed: {e:#}"),
                );
            }
            return;
        }
    };

    if !running.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap_or(InstanceIndex {
                version: 0,
                instances: Vec::new(),
            });
            let (headline, rows) = crate::runtime::exit_summary::summary(&running, &index);
            run.compact(
                "exit_summary",
                &format!("{headline}; boundary outro skipped"),
            );
            for row in rows {
                run.compact("exit_summary", &row);
            }
        }
        if !force_outro {
            return;
        }
    }

    // Last container left the construct: clear the session marker and show the
    // two-screen outro (decelerating warp, then closing caption). Exits that
    // leave other instances running skip this entirely because the operator is
    // still inside the Construct.
    let elapsed = if force_outro && !running.is_empty() {
        None
    } else {
        match crate::runtime::universe::take_exit_claim(paths) {
            ExitClaim::Claimed { elapsed } => elapsed,
            ExitClaim::Missing if force_outro => None,
            ExitClaim::Missing => return,
        }
    };
    if !crate::runtime::progress::rich_terminal_supported() {
        return;
    }
    // Defensive: the attach paths already re-assert the alt screen the moment
    // the capsule exec returns, so the post-attach work never flashes the
    // shell. Re-assert once more before the rich outro in case render_exit is
    // reached by a path that did not go through the attach.
    jackin_diagnostics::reassert_alt_screen();
    let host_owned = jackin_diagnostics::host_screen_owned();
    crate::runtime::progress::launch_output().warp_out(host_owned);
    crate::runtime::progress::launch_output().warp_end_caption(elapsed, host_owned);
}
