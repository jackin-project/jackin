// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Shell and agent session spawning.

#![expect(
    clippy::print_stderr,
    reason = "attach flow emits intentional terminal spacing on stderr"
)]

use jackin_core::container_paths;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_protocol::attach::SpawnRequest;

use jackin_core::JackinPaths;

use super::{
    finalize_reconnected_foreground_session_with_handle, git_policy_env_pairs,
    host_alt_screen_exec_flag, insert_run_as_user, require_container_reachable,
    require_container_running, require_current_instance_admission, set_role_terminal_title,
};

/// Open a one-shot interactive zsh shell in a running container.
///
/// Ephemeral one-shot — no persistent session, no reconnect on detach. Used
/// by `jackin hardline --shell` and the console Shell action.
pub async fn spawn_shell_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let (admission_lease, container) = require_container_reachable(
        paths,
        container_name,
        docker,
        "restart it before opening a shell",
    )
    .await?;
    set_role_terminal_title(paths, container_name);
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    admission_lease.ensure_current(paths)?;
    if crate::runtime::host_attach::host_attach_enabled(paths) {
        let result = crate::runtime::host_attach::run_host_attach_session(
            paths,
            &container,
            Some(SpawnRequest::Shell),
            None,
            &[],
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease.ensure_current(paths)?;
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session_with_handle(
            paths,
            container_name,
            &admission_lease,
            docker,
            runner,
            &container,
        )
        .await;
    }
    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container.id(),
        container_paths::CAPSULE_BIN,
        "new",
    ];
    insert_run_as_user(&mut args, run_as_user);
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "shell_session_exec",
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let result = runner
        .run(
            "docker",
            &args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "shell_session_exec",
        if result.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if result.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from shell session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    admission_lease.ensure_current(paths)?;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Spawning a single agent session requires every caller-supplied \
              parameter (paths, container_name, requested_instance_id, agent, \
              env_overrides, git config, docker, runner, ...) to flow through to \
              the container bring-up path; bundling into a config struct would be \
              a parallel pass that requires restructuring the spawn path. Named- \
              arg reads match the per-input propagation idiom."
)]
pub async fn spawn_agent_session(
    paths: &JackinPaths,
    container_name: &str,
    requested_instance_id: Option<&str>,
    agent: jackin_core::Agent,
    env_overrides: &[(String, String)],
    git_coauthor_trailer: bool,
    git_dco: bool,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !env_overrides
            .iter()
            .any(|(name, _)| jackin_env::is_account_env(name)),
        "account credential and routing overrides are not allowed; select an assigned account and recreate the container"
    );
    let container = require_container_running(
        paths,
        container_name,
        docker,
        "restart or recover it before using `--new`",
    )
    .await?;
    let admission_lease = crate::runtime::launch::AccountConfigRevision::acquire(paths)?;

    let (live_manifest, admitted_instance_id) = require_current_instance_admission(
        paths,
        container_name,
        agent,
        requested_instance_id,
        &admission_lease,
    )?;
    let workdir = live_manifest.workdir.as_str();
    let spawn_target = admitted_instance_id
        .as_deref()
        .unwrap_or_else(|| agent.slug());

    // Instance selection travels as `jackin-capsule new <instance-id>` argv; the
    // git policy toggles are session env consumed by the spawned entrypoint.
    // Each transport encodes them only on the path that consumes it.
    set_role_terminal_title(paths, container_name);
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    if crate::runtime::host_attach::host_attach_enabled(paths) {
        let mut session_env_overrides: Vec<(String, String)> =
            git_policy_env_pairs(git_coauthor_trailer, git_dco)
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect();
        session_env_overrides.extend(env_overrides.iter().cloned());
        let spawn_request = SpawnRequest::instance(spawn_target)?;
        admission_lease.ensure_current(paths)?;
        let result = crate::runtime::host_attach::run_host_attach_session(
            paths,
            &container,
            Some(spawn_request),
            None,
            &session_env_overrides,
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease.ensure_current(paths)?;
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session_with_handle(
            paths,
            container_name,
            &admission_lease,
            docker,
            runner,
            &container,
        )
        .await;
    }

    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut exec_args = vec!["exec", "--workdir", workdir, "-it"];
    insert_run_as_user(&mut exec_args, run_as_user);
    // Git policy and non-account session environment outlive `exec_args`.
    let env_flags: Vec<String> = git_policy_env_pairs(git_coauthor_trailer, git_dco)
        .into_iter()
        .map(|(name, value)| format!("-e={name}={value}"))
        .chain(env_overrides.iter().map(|(k, v)| format!("-e={k}={v}")))
        .collect();
    for flag in &env_flags {
        exec_args.push(flag.as_str());
    }
    exec_args.push(container.id());
    exec_args.extend_from_slice(&[container_paths::CAPSULE_BIN, "new", spawn_target]);
    if let Some(flag) = host_alt_screen_exec_flag() {
        exec_args.insert(1, flag);
    }
    let timing_name = format!("new_{}_session_exec", agent.slug());
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        &timing_name,
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let result = runner
        .run(
            "docker",
            &exec_args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        &timing_name,
        if result.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if result.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from agent session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    admission_lease.ensure_current(paths)?;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}
