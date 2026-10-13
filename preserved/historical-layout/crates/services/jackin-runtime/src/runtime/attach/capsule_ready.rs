// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Capsule daemon readiness waits and `dind` warmup waits.

use anyhow::Context as _;

use jackin_core::ContainerHandle;
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

pub(crate) async fn wait_for_capsule_daemon_with_handle(
    paths: &JackinPaths,
    container: &ContainerHandle,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    pub(crate) const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
    pub(crate) const INITIAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
    pub(crate) const MAX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "wait_capsule_socket",
        Some(container.name()),
    );
    let wait_result = wait_for_capsule_daemon_ready(
        paths,
        container,
        docker,
        MAX_WAIT,
        INITIAL_INTERVAL,
        MAX_INTERVAL,
    )
    .await
    .with_context(|| format!("waiting for jackin-capsule daemon in {}", container.name()));
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "wait_capsule_socket",
        if wait_result.is_ok() {
            Some("ready")
        } else {
            Some("error")
        },
    );
    if wait_result.is_err() {
        let _error = jackin_telemetry::record_error(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        );
        jackin_diagnostics::emit_operator_notice("container readiness wait failed");
    }
    wait_result
}

pub(crate) async fn wait_for_capsule_daemon_ready(
    paths: &JackinPaths,
    container: &ContainerHandle,
    docker: &impl DockerApi,
    max_wait: std::time::Duration,
    initial_interval: std::time::Duration,
    max_interval: std::time::Duration,
) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    let mut interval = initial_interval;

    loop {
        if capsule_daemon_socket_connects(paths, container.name()) {
            return Ok(());
        }

        let protocol_check = format!(
            "exec /jackin/runtime/jackin-capsule protocol-check --expected-major {}",
            jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR
        );
        let Err(exec_error) = docker
            .exec_capture_by_id(container, &["sh", "-c", &protocol_check])
            .await
        else {
            return Ok(());
        };

        if started.elapsed() >= max_wait {
            return Err(exec_error).with_context(|| {
                format!("timed out after {max_wait:?} waiting for capsule daemon readiness")
            });
        }

        tokio::time::sleep(interval).await;
        interval = (interval * 2).min(max_interval);
    }
}

pub(crate) fn capsule_daemon_socket_connects(paths: &JackinPaths, container_name: &str) -> bool {
    let socket_path = crate::runtime::snapshot::socket_path(paths, container_name);
    socket_path.exists() && capsule_socket_negotiates(&socket_path).is_ok()
}

pub(crate) fn capsule_socket_negotiates(socket_path: &std::path::Path) -> anyhow::Result<()> {
    let mut stream = jackin_diagnostics::operation::connection_attempt_sync(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleAttach,
        || std::os::unix::net::UnixStream::connect(socket_path),
    )
    .with_context(|| format!("connecting to Capsule socket {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .context("setting Capsule readiness read timeout")?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
        .context("setting Capsule readiness write timeout")?;
    jackin_protocol::capsule_transport::client_handshake(
        &mut stream,
        std::time::Duration::from_secs(2),
    )
    .context("negotiating Capsule readiness protocol")
}

pub(crate) async fn wait_for_dind(
    dind: &ContainerHandle,
    certs_volume: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    pub(crate) const MAX_ATTEMPTS: u32 = 30;
    pub(crate) const INITIAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
    pub(crate) const MAX_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

    // Shared spinner helper: it suppresses its own stderr output while the
    // rich launch cockpit owns the screen, so the sidecar stage shows only
    // in the rail rather than streaming "Waiting for ..." over the frame.
    crate::spin_wait::spin_wait_ramped(
        "Waiting for Docker-in-Docker to be ready",
        MAX_ATTEMPTS,
        INITIAL_INTERVAL,
        MAX_INTERVAL,
        || async {
            docker
                .exec_capture_by_id(dind, &["docker", "info"])
                .await
                .map(|_| ())
        },
    )
    .await
    .with_context(|| {
        format!(
            "timed out waiting for Docker-in-Docker sidecar {}",
            dind.name()
        )
    })?;

    match docker
        .exec_capture_by_id(dind, &["test", "-f", "/certs/client/ca.pem"])
        .await
    {
        Ok(_) => {}
        Err(e) if e.to_string().contains("exited with code") => {
            anyhow::bail!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            );
        }
        Err(e) => return Err(e.context(format!("checking TLS cert presence in {}", dind.name()))),
    }

    Ok(())
}
