// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Foreground reconnect finalization and resource release.

use crate::instance::{InstanceManifest, InstanceStatus};

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use super::{
    mark_reconnect_admission_failure, reconnect_or_create_session_with_container_handle_with_lease,
    require_current_account_admission, validate_current_account_admission,
    validate_recorded_role_handle,
};

pub(crate) async fn finalize_reconnected_foreground_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)?;
    finalize_reconnected_foreground_session_with_lease(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(crate) async fn finalize_reconnected_foreground_session_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container = inspection
        .handle
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve container {container_name}: {}",
                inspection.state.inspect_label()
            )
        })
        .map_err(mark_reconnect_admission_failure)?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

pub(crate) async fn finalize_reconnected_foreground_session_with_handle(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    validate_recorded_role_handle(paths, container_name, container)?;
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "post_attach_outcome_inspect",
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let mut outcome =
        crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
    admission_lease.ensure_current(paths)?;
    let outcome_label = outcome.as_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "post_attach_outcome_inspect",
        Some(&outcome_label),
    );
    crate::runtime::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    // The dirty-exit decision is made in-capsule (the dirty-exit modal) and
    // recorded in exit-action.json; the host only executes it — no host dialog.
    let mut prompt = crate::isolation::finalize::ExitActionPrompt {
        state_dir: paths.data_dir.join(container_name).join("state"),
    };
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "foreground_session_finalize",
        Some(container_name),
    );
    let mut decision = crate::isolation::finalize::finalize_foreground_session(
        crate::isolation::finalize::FinalizeContext {
            container_name,
            container_state_dir: &paths.data_dir.join(container_name),
            outcome,
            is_interactive: interactive,
            dirty_exit_policy: jackin_config::DirtyExitPolicy::Ask,
            prompt: &mut prompt,
            docker,
            runner,
            container: container.clone(),
        },
    )
    .await?;
    admission_lease.ensure_current(paths)?;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "foreground_session_finalize",
        Some(decision.as_str()),
    );

    if matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::ReturnToAgent
    ) {
        admission_lease.ensure_current(paths)?;
        reconnect_or_create_session_with_container_handle_with_lease(
            paths,
            container_name,
            None,
            admission_lease,
            docker,
            runner,
            container,
            None,
        )
        .await?;
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "post_attach_outcome_inspect",
            Some(container_name),
        );
        admission_lease.ensure_current(paths)?;
        outcome = crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
        admission_lease.ensure_current(paths)?;
        let outcome_label = outcome.as_label();
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "post_attach_outcome_inspect",
            Some(&outcome_label),
        );
        crate::runtime::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "foreground_session_finalize",
            Some(container_name),
        );
        decision = crate::isolation::finalize::finalize_foreground_session(
            crate::isolation::finalize::FinalizeContext {
                container_name,
                container_state_dir: &paths.data_dir.join(container_name),
                outcome,
                is_interactive: interactive,
                dirty_exit_policy: jackin_config::DirtyExitPolicy::Ask,
                prompt: &mut prompt,
                docker,
                runner,
                container: container.clone(),
            },
        )
        .await?;
        admission_lease.ensure_current(paths)?;
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "foreground_session_finalize",
            Some(decision.as_str()),
        );
    }

    finalize_reconnected_resources(paths, container_name, outcome, decision, docker, container)
        .await
}

pub(crate) async fn finalize_reconnected_resources(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
    decision: crate::isolation::finalize::FinalizeDecision,
    docker: &impl DockerApi,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    use crate::isolation::finalize::{AttachOutcome, FinalizeDecision};

    let should_teardown = match (outcome, decision) {
        (_, FinalizeDecision::ReturnToAgent) => false,
        (AttachOutcome::Stopped(0), _)
        | (AttachOutcome::StillRunning, FinalizeDecision::Cleaned) => true,
        _ => false,
    };
    if !should_teardown {
        return Ok(());
    }

    let state_dir = paths.data_dir.join(container_name);
    let status = if matches!(decision, FinalizeDecision::Preserved) {
        crate::runtime::launch::preserved_instance_status(&state_dir)?
    } else {
        InstanceStatus::CleanExited
    };
    if let Some(mut manifest) = InstanceManifest::read_optional_lossy(&state_dir) {
        crate::runtime::launch::write_instance_status(paths, &state_dir, &mut manifest, status)?;
    }
    let dind_handle =
        crate::runtime::cleanup::resolve_dind_handle_for_state(paths, container_name, docker)
            .await?;
    crate::runtime::cleanup::eject_docker_role_with_handles(
        paths,
        container_name,
        docker,
        container,
        dind_handle.as_ref(),
    )
    .await
}
