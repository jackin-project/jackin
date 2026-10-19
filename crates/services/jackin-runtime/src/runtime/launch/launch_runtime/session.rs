// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::attach_failure_error;
use super::super::exit_diagnosis::{ExitPhase, diagnose_premature_exit_by_id};
use super::super::progress_helpers::StepCounter;
use super::{LaunchContext, LaunchOutcome, SidecarPrewarmReplenish};
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_runtime_launch_post_run_failure::post_run_failure::emit_post_run_failure;

pub(super) async fn apply_post_run_policy(
    ctx: &LaunchContext<'_, '_>,
    role_container: &jackin_core::ContainerHandle,
    runner: &mut impl CommandRunner,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let docker_run_opts = RunOptions {
        quiet: !ctx.debug,
        ..RunOptions::default()
    };
    let LaunchContext {
        container_name,
        debug,
        profile,
        grants,
        ..
    } = ctx;
    let mut post_run_steps: Vec<(String, [&str; 6], String, bool)> = Vec::new();
    if let Some(argv) =
        crate::runtime::docker_profile::firewall_post_run_argv(grants, role_container.id())
    {
        post_run_steps.push((
            format!("firewall_apply profile={profile}"),
            argv,
            format!("egress allowlist install failed for `{profile}` profile; container torn down (fail-closed). The agent was not started without the firewall the profile promises."),
            true,
        ));
    }
    if grants.sudo {
        post_run_steps.push((
            format!("sudo_provision profile={profile}"),
            crate::runtime::docker_profile::sudo_provision_post_run_argv(role_container.id()),
            format!("sudo provisioning failed for `{profile}` profile; container torn down (fail-closed)."),
            false,
        ));
    }
    for (label, argv, failure_context, is_firewall) in post_run_steps {
        let result = runner.run("docker", &argv, None, &docker_run_opts).await;
        if *debug {
            jackin_diagnostics::emit_debug_line(
                "launch",
                &format!(
                    "{label} exit={}",
                    if result.is_ok() { "0" } else { "nonzero" },
                ),
            );
        }
        if let Err(err) = result {
            emit_post_run_failure(is_firewall);
            if let Err(remove_err) = docker.remove_container_by_id(role_container).await {
                jackin_diagnostics::emit_compact_line(
                    "warning",
                    &format!(
                        "fail-closed teardown could not remove {container_name}: {remove_err}"
                    ),
                );
            }
            return Err(err.context(failure_context));
        }
    }
    Ok(())
}

pub(super) async fn ensure_ready(
    ctx: &LaunchContext<'_, '_>,
    role_container: &jackin_core::ContainerHandle,
    runner: &mut impl CommandRunner,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let LaunchContext {
        container_name,
        workspace,
        paths,
        account_revision,
        ..
    } = ctx;
    jackin_host::caffeinate::reconcile_when_configured(
        paths,
        docker,
        runner,
        workspace.keep_awake_enabled,
    )
    .await;

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "pre_attach_exit_check",
        Some(container_name),
    );
    if let Some(err) =
        diagnose_premature_exit_by_id(docker, runner, role_container, ExitPhase::PreAttach).await
    {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Capsule,
            "pre_attach_exit_check",
            Some("exited"),
        );
        return Err(err);
    }
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "pre_attach_exit_check",
        Some("running"),
    );
    super::super::ensure_current_or_remove_stale_container(
        account_revision,
        paths,
        role_container,
        docker,
    )
    .await?;

    Ok(())
}

async fn settle_progress(steps: &mut StepCounter) {
    steps.stage_done(crate::runtime::progress::LaunchStage::Capsule, "ready");
    steps.opening_hardline();
    if let Some(progress) = steps.progress_mut() {
        progress.settle_stage_visual().await;
    }
    steps.finish_progress();
}

fn spawn_background(
    ctx: &LaunchContext<'_, '_>,
) -> Option<tokio::task::JoinHandle<jackin_telemetry::spawn::DetachedCompletion>> {
    let LaunchContext {
        selector,
        debug,
        agent,
        paths,
        selected_image_refresh,
        reuse_staleness_sentinel,
        sibling_prewarm,
        ..
    } = ctx;
    if let Some(refresh) = selected_image_refresh {
        crate::runtime::image::spawn_selected_image_refresh(
            paths,
            selector,
            refresh.role_git,
            refresh.branch_override,
            *agent,
            refresh.reason,
            *debug,
        );
    }
    if let Some(sentinel) = reuse_staleness_sentinel {
        crate::runtime::image::spawn_reuse_staleness_sentinel(
            paths,
            selector,
            sentinel.role_git,
            sentinel.branch_override,
            *agent,
            sentinel.image,
            *debug,
        );
    }
    let sibling_runtime_prewarm = crate::runtime::image::spawn_sibling_runtime_prewarm(
        paths,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    crate::runtime::image::spawn_sibling_image_prewarm(
        paths,
        selector,
        sibling_prewarm.role_git,
        sibling_prewarm.branch_override,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    sibling_runtime_prewarm
}

async fn attach_foreground(
    ctx: &LaunchContext<'_, '_>,
    role_container: &jackin_core::ContainerHandle,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let LaunchContext {
        container_name,
        debug,
        paths,
        account_revision,
        ..
    } = ctx;
    let session_result =
        crate::runtime::attach::reconnect_or_create_session_with_container_handle_with_lease(
            paths,
            container_name,
            None,
            account_revision,
            docker,
            runner,
            role_container,
            None,
        )
        .await;
    eprintln!();
    if let Err(err) = session_result {
        if err.is::<super::super::super::attach::ReconnectAdmissionFailure>()
            || err.is::<super::super::GenerationLeaseViolation>()
        {
            return Err(err);
        }
        let inspect = docker.inspect_container_by_id(role_container).await;
        if let Some(diag) = super::super::diagnose_with_state_by_id(
            runner,
            role_container,
            &inspect,
            ExitPhase::PostAttach,
        )
        .await
        {
            return Err(diag);
        }
        if !super::super::is_known_socket_close(&err, &inspect) {
            return Err(err);
        }
        let attach_detail = attach_failure_error(container_name, &err);
        if *debug {
            jackin_diagnostics::emit_debug_line(
                "session",
                &format!(
                    "clean container exit for {container_name}; proceeding to finalize \
                     (attach shutdown detail: {attach_detail})"
                ),
            );
        }
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact(
                jackin_telemetry::schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN,
                &format!("container {container_name} exited cleanly after session"),
            );
        }
    }
    Ok(())
}

pub(super) async fn finish(
    ctx: &LaunchContext<'_, '_>,
    steps: &mut StepCounter,
    role_container: jackin_core::ContainerHandle,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<LaunchOutcome> {
    settle_progress(steps).await;
    let _sibling_runtime_prewarm = spawn_background(ctx);
    let LaunchContext {
        debug,
        paths,
        sidecar_prewarm_replenish,
        non_interactive,
        account_revision,
        ..
    } = ctx;
    if *non_interactive {
        steps.stage_done(
            crate::runtime::progress::LaunchStage::Hardline,
            "detached (non-interactive launch)",
        );
        if matches!(
            sidecar_prewarm_replenish,
            SidecarPrewarmReplenish::AfterAttach
        ) {
            crate::runtime::prewarm_trigger::spawn_background_sidecar_prewarm(paths, *debug);
        }
        super::super::ensure_current_or_remove_stale_container(
            account_revision,
            paths,
            &role_container,
            docker,
        )
        .await?;
        return Ok(LaunchOutcome::Detached(role_container));
    }
    attach_foreground(ctx, &role_container, docker, runner).await?;
    steps.stage_done(crate::runtime::progress::LaunchStage::Hardline, "open");
    if matches!(
        sidecar_prewarm_replenish,
        SidecarPrewarmReplenish::AfterAttach
    ) {
        crate::runtime::prewarm_trigger::spawn_background_sidecar_prewarm(paths, *debug);
    }

    Ok(LaunchOutcome::ForegroundSessionEnded(role_container))
}
