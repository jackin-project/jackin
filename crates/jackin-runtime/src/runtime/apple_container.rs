// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Apple Container backend launch, attach, reconnect, eject, and purge.
//!
//! All lifecycle operations shell out to the `container` CLI via
//! the shared process transport, unlike the Docker backend which uses bollard.
//!
//! # Prerequisites
//!
//! - macOS 26 ARM with `apple/container` installed
//! - `JACKIN_CAPSULE_FORCE_DAEMON=1` injected at `container run` time
//!   (NOT a static Dockerfile ENV — that breaks the Docker backend)
//!
//! # `DinD` gating
//!
//! `DinD` inside the VM (rootless `DinD` via `--cap-add`) requires Phase 0
//! empirical validation. `inner_docker_enabled` defaults to `false` until
//! Phase 0 results confirm `DinD` works inside apple/container VMs.

use crate::apple_container_client::AppleContainerMount;
use anyhow::{Context as _, Result, bail};

use crate::apple_container_client::AppleContainerApi as _;
use crate::instance::{
    AppleContainerResources, BackendResources, DockerResources, InstanceManifest,
    NewInstanceManifest,
};
use jackin_core::JackinPaths;
use jackin_core::container_paths;

const ATTACH_MAX_WAIT_MS: u64 = 60_000;
const ATTACH_POLL_MS: u64 = 500;

/// Print the session contract — the security boundary summary shown to the
/// operator before the interactive attach begins, so they see the isolation
/// model and residual risks before the session starts.
#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn print_session_contract(
    container_name: &str,
    image: &str,
    provider_version: &str,
    mounts: &[AppleContainerMount],
    debug: bool,
) {
    eprintln!();
    eprintln!("[jackin] session contract");
    eprintln!("  backend:              apple-container");
    eprintln!("  provider:             apple/container {provider_version}");
    eprintln!("  container:            {container_name}");
    eprintln!("  image:                {image}");
    eprintln!("  isolation:            own Linux kernel via Virtualization.framework");
    eprintln!("  host filesystem:      explicit bind mounts only");
    eprintln!("  host Docker socket:   not mounted");
    eprintln!("  inner Docker (DinD):  disabled — pending Phase 0 DinD validation");
    eprintln!(
        "  force_daemon:         JACKIN_CAPSULE_FORCE_DAEMON=1 (capsule PID 2+ under vminitd)"
    );
    eprintln!("  mounts ({}):", mounts.len());
    if mounts.is_empty() {
        eprintln!("    (none)");
    } else {
        for mount in mounts {
            let suffix = if mount.readonly { ":ro" } else { "" };
            eprintln!(
                "    {}:{}{suffix}",
                mount.source.display(),
                mount.target.display()
            );
        }
    }
    eprintln!("  network:              per-container IP via vmnet (no port mapping)");
    eprintln!("  dns:                  may hiccup after macOS sleep/wake — reconnect if affected");
    eprintln!("  residual risk:");
    eprintln!("    DinD not enabled; Docker workflows inside the VM require Phase 0 validation.");
    eprintln!("    apple/container vminitd is PID 1; signal forwarding relies on gRPC/vsock.");
    eprintln!("    Build-time Docker (image build) still runs on host Docker engine.");
    if debug {
        eprintln!("  debug mode:           on (--debug)");
    }
    eprintln!();
}

/// DNS health check — an `nslookup` probe run after attach returns. macOS
/// sleep/wake can drop DNS inside the VM; surface a "reconnect" hint if affected.
#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub async fn check_dns(container_name: &str) {
    let result = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
        "container",
        [
            "exec",
            "--user",
            crate::runtime::identity::CAPSULE_SUPERVISOR_USER,
            container_name,
            "sh",
            "-c",
            "nslookup github.com >/dev/null 2>&1 && echo ok || echo hiccup",
        ],
    ))
    .await;

    match result {
        Ok(o) if o.success => {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_owned();
            if out == "hiccup" {
                eprintln!(
                    "[jackin] apple-container: DNS hiccup detected after sleep/wake. \
                     If the agent cannot reach the network, run `jackin hardline` to reconnect."
                );
            }
        }
        _ => {}
    }
}

/// Wait until `/jackin/run/jackin.sock` is answering status queries inside
/// the apple/container container.
pub async fn wait_for_capsule(container_name: &str) -> Result<()> {
    let check_cmd = "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule status";
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_millis(ATTACH_MAX_WAIT_MS);

    loop {
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "timed out waiting for jackin-capsule daemon in container {container_name}; \
                 check `container logs {container_name}` for startup errors"
            );
        }

        let output = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
            "container",
            [
                "exec",
                "--user",
                crate::runtime::identity::CAPSULE_SUPERVISOR_USER,
                container_name,
                "sh",
                "-c",
                check_cmd,
            ],
        ))
        .await;

        match output {
            Ok(o) if o.success => return Ok(()),
            _ => {
                tokio::time::sleep(tokio::time::Duration::from_millis(ATTACH_POLL_MS)).await;
            }
        }
    }
}

/// Attach interactively to a running apple/container container.
/// Uses `container exec -it <name> /jackin/runtime/jackin-capsule` which
/// provides a proper PTY with SIGWINCH forwarding via the vminitd gRPC/vsock layer.
///
/// Returns the capsule's exit code (`None` if it was signalled) so the caller
/// can record an attach outcome — a non-zero exit distinguishes a crash from a
/// clean detach.
pub async fn attach(container_name: &str, focus_session: Option<u64>) -> Result<Option<i32>> {
    let mut args: Vec<&str> = vec![
        "exec",
        "--user",
        crate::runtime::identity::CAPSULE_SUPERVISOR_USER,
        "-it",
        container_name,
        container_paths::CAPSULE_BIN,
    ];

    let focus_str;
    if let Some(id) = focus_session {
        focus_str = id.to_string();
        args.push("--focus");
        args.push(&focus_str);
    }

    let request = jackin_process::ExecRequest::new("container", &args)
        .stdin_mode(jackin_process::StdioMode::Inherit)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit);
    let status = crate::process_telemetry::exec_async(&request)
        .await
        .context("container exec failed — is apple/container installed?")?;

    jackin_diagnostics::reassert_alt_screen();
    Ok(status.code)
}

/// Record the post-attach outcome into the instance manifest so
/// `jackin --inspect` can show whether a role crashed. This records the outcome
/// only; unlike the Docker reconnect path it does not run session
/// finalization/teardown — apple-container finalization is not yet wired.
/// Best-effort: a missing/corrupt manifest is a no-op (logged downstream).
async fn record_attach_outcome(paths: &JackinPaths, container_name: &str, exit_code: Option<i32>) {
    use crate::isolation::finalize::AttachOutcome;
    let outcome = if is_container_running(container_name).await {
        AttachOutcome::still_running()
    } else {
        AttachOutcome::stopped(exit_code.unwrap_or(-1))
    };
    drop(super::launch::record_instance_attach_outcome(
        paths,
        container_name,
        outcome,
    ));
}

/// Inputs for the apple-container launch path. Grouped into a struct so the
/// many backend-specific parameters travel together from the `load_role_with`
/// call site instead of as a long positional argument list.
#[derive(Debug)]
pub struct AppleContainerLaunch<'a> {
    pub paths: &'a JackinPaths,
    pub container_name: &'a str,
    pub image: &'a str,
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub role_key: &'a str,
    pub role_display_name: &'a str,
    pub agent: jackin_core::Agent,
    pub role_source_git: &'a str,
    pub role_source_ref: Option<&'a str>,
    pub image_tag: &'a str,
    pub env_pairs: &'a [(String, String)],
    pub mounts: &'a [AppleContainerMount],
    pub host_workdir_fingerprint: &'a str,
    pub capsule_config: &'a jackin_protocol::CapsuleConfig,
    pub state: &'a crate::instance::RoleState,
    pub resolved_env: &'a jackin_env::ResolvedEnv,
    pub debug: bool,
}

fn validate_exec_bindings(bindings: &[jackin_protocol::ExecBinding]) -> Result<()> {
    if bindings.is_empty() {
        return Ok(());
    }

    crate::exec_host::ensure_caller_auth_supported()
        .context("apple-container does not support on-demand credential bindings")
}

/// Full launch path for the `apple-container` backend.
///
/// Called from `load_role_with` after the image build step when the resolved
/// backend is `"apple-container"`.
pub async fn launch(args: AppleContainerLaunch<'_>) -> Result<()> {
    let AppleContainerLaunch {
        paths,
        container_name,
        image,
        workspace_name,
        workspace_label,
        workdir,
        role_key,
        role_display_name,
        agent,
        role_source_git,
        role_source_ref,
        image_tag,
        env_pairs,
        mounts,
        host_workdir_fingerprint,
        capsule_config,
        state,
        resolved_env,
        debug,
    } = args;

    validate_exec_bindings(&capsule_config.exec_bindings)?;

    // Probe container CLI availability.
    let version = probe_version().await;
    if version.is_none() {
        bail!(
            "apple/container CLI (`container`) not found. \
             Install from https://github.com/apple/container or via Homebrew."
        );
    }

    // Build AppleContainerSpec — delegates all arg formatting to the client.
    // JACKIN_CAPSULE_FORCE_DAEMON=1 enables daemon mode without PID 1 (vminitd
    // is PID 1 inside apple/container VMs; capsule runs as entrypoint at PID 2+).
    let mut env: Vec<(String, String)> = vec![
        ("JACKIN_CAPSULE_FORCE_DAEMON".to_owned(), "1".to_owned()),
        (
            // vminitd is PID 1; Capsule entrypoint is launched after it. This
            // is the Apple launch contract, not a runtime probe of the live PID.
            jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV.to_owned(),
            jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID.to_string(),
        ),
    ];
    if debug {
        env.push(("JACKIN_TELEMETRY_LEVEL".to_owned(), "debug".to_owned()));
    }
    let host_env_entries = env_pairs
        .iter()
        .filter(|(key, _)| {
            key != "JACKIN_CAPSULE_FORCE_DAEMON"
                && key != jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV
                && key != "JACKIN_DEBUG"
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut capsule_config = capsule_config.clone();
    // Mirror the Docker path: list on-demand credential var names so the
    // in-container MCP tool advertises which commands need jackin-exec.
    let names = super::launch::exec_binding_names(&capsule_config.exec_bindings);
    if !names.is_empty() {
        env.push(("JACKIN_EXEC_BINDINGS".to_owned(), names));
    }

    // Apple Container's UDS relay does not preserve guest peer credentials, so
    // usage traffic uses the same Capsule-local stdio proxy as Docker. Only
    // the launch config is file-mounted; mounting a host socket directory is
    // not a valid Apple Container transport for Unix sockets.
    let socket_dir = paths.jackin_home.join("sockets").join(container_name);
    let prepared_usage_relay =
        crate::usage_relay::prepare_for_stdio_tunnel(crate::usage_relay::UsageRelayLaunch {
            paths,
            workspace_name,
            role_key,
            launch_config: &capsule_config,
            forwarded_sources: crate::usage_relay::forwarded_sources_from_launch_config(
                state,
                resolved_env,
                &capsule_config,
            ),
        })
        .await
        .context("starting scoped usage relay")?;
    prepared_usage_relay.apply_to_launch_config(&mut capsule_config);
    let capsule_config_contents = super::launch::capsule_config_contents(&capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    super::launch::prepare_socket_dir(&socket_dir, &capsule_config_contents)?;
    let mut container_mounts = mounts.to_vec();
    container_mounts.push(crate::usage_relay::apple_runtime_mount(socket_dir.clone()));
    if !capsule_config.exec_bindings.is_empty() {
        drop(crate::exec_host::start_bound_for_container(
            &paths.jackin_home,
            container_name,
            &capsule_config.exec_bindings,
        )?);
        container_mounts.push(AppleContainerMount::new(
            socket_dir.join("host.sock"),
            jackin_protocol::HOST_SOCK_CONTAINER_PATH,
            false,
        ));
    }

    let host_env_file =
        super::launch::create_host_env_file(&paths.jackin_home, container_name, &host_env_entries)
            .context("creating private host runtime environment")?;

    let spec = crate::apple_container_client::AppleContainerSpec {
        image: image.to_owned(),
        user: crate::runtime::identity::CAPSULE_SUPERVISOR_USER.to_owned(),
        env,
        env_file: host_env_file.as_ref().map(|file| file.path().to_path_buf()),
        mounts: container_mounts,
        caps_add: vec![],
    };

    let run_result = crate::apple_container_client::AppleContainerClient::new()
        .run_container(container_name, &spec)
        .await;
    drop(host_env_file);
    run_result
        .context("container run failed — required capabilities or image may be unavailable")?;
    let _usage_relay_guard =
        crate::usage_relay::start_apple_tunnel(container_name, prepared_usage_relay)
            .context("starting scoped usage stdio tunnel")?;

    // Write instance manifest.
    let container_state = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::new_with_backend(
        NewInstanceManifest {
            container_base: container_name,
            workspace_name,
            workspace_label,
            workdir,
            host_workdir_fingerprint,
            role_key,
            role_display_name,
            agent_runtime: agent,
            role_source_git,
            role_source_ref,
            image_tag,
            docker: DockerResources::from_container_name(container_name),
            role_git_sha: None,
            base_image_ref: None,
            base_image_digest: None,
            supported_agents: vec![],
        },
        BackendResources::AppleContainer(AppleContainerResources {
            container_name: container_name.to_owned(),
            role_image_ref: image_tag.to_owned(),
            inner_docker_enabled: false, // gated on Phase 0 DinD validation
        }),
    );
    manifest.write(&container_state)?;

    // No second host.sock resolver start here: the pre-bound
    // `start_bound_for_container` above already owns the session resolver, and
    // starting again would double-bind the same socket path.

    // Wait for capsule daemon readiness.
    wait_for_capsule(container_name).await?;
    // Printed once after the container starts, before the interactive attach,
    // so the operator sees the security boundary, isolation model, and residual
    // risks before their session begins.
    print_session_contract(
        container_name,
        image,
        version.as_deref().unwrap_or("unknown"),
        mounts,
        debug,
    );

    // Interactive attach — blocks until operator detaches.
    let exit_code = attach(container_name, None).await?;
    record_attach_outcome(paths, container_name, exit_code).await;

    // Catches a sleep/wake DNS hiccup once the operator detaches.
    check_dns(container_name).await;

    Ok(())
}

/// Check whether an apple/container container is currently running.
/// Delegates to `AppleContainerClient::list_containers` which owns all
/// JSON parsing for `container ps` output.
async fn is_container_running(container_name: &str) -> bool {
    match crate::apple_container_client::AppleContainerClient::new()
        .list_containers(container_name)
        .await
    {
        Ok(v) => v.iter().any(|c| c.name == container_name && c.is_running()),
        Err(_) => false,
    }
}

/// Reconnect to a stopped or running apple/container container.
pub async fn reconnect(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
) -> Result<()> {
    let running = is_container_running(container_name).await;

    if !running {
        let start = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
            "container",
            ["start", container_name],
        ))
        .await
        .context("container start failed — is apple/container installed?")?;
        if !start.success {
            bail!("container start exited unsuccessfully");
        }
    }

    wait_for_capsule(container_name).await?;
    let exit_code = attach(container_name, focus_session).await?;
    record_attach_outcome(paths, container_name, exit_code).await;
    Ok(())
}

/// Guard for the purge path: bail if the apple-container VM still exists
/// (running or stopped). Mirrors the Docker `ensure_role_resources_absent_for_purge`
/// guard — purge is the safe path and must refuse while the container is live,
/// directing the operator to eject first; an already-removed container is the
/// success case (so purging a torn instance whose VM is gone is not blocked).
pub async fn ensure_absent_for_purge(container_name: &str) -> Result<()> {
    ensure_absent_for_purge_with(
        &crate::apple_container_client::AppleContainerClient::new(),
        container_name,
    )
    .await
}

pub async fn ensure_absent_for_purge_with(
    client: &impl crate::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    let exists = client
        .list_containers(container_name)
        .await?
        .iter()
        .any(|c| c.name == container_name);
    if exists {
        bail!(
            "cannot purge local state: apple-container `{container_name}` still exists; \
             run `jackin eject {container_name} --purge` to remove the container and state together"
        );
    }
    Ok(())
}

/// Stop the container (eject — preserves manifest).
pub async fn stop(container_name: &str) -> Result<()> {
    stop_with(
        &crate::apple_container_client::AppleContainerClient::new(),
        container_name,
    )
    .await
}

pub async fn stop_with(
    client: &impl crate::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    client.stop_container(container_name).await
}

/// Remove the container (purge).
pub async fn remove(container_name: &str) -> Result<()> {
    remove_with(
        &crate::apple_container_client::AppleContainerClient::new(),
        container_name,
    )
    .await
}

pub async fn remove_with(
    client: &impl crate::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    // Stop first (ignore errors — may already be stopped).
    drop(client.stop_container(container_name).await);
    client.remove_container(container_name).await
}

/// Probe the `container` CLI version. Returns `None` if not installed.
pub async fn probe_version() -> Option<String> {
    let output = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
        "container",
        ["--version"],
    ))
    .await
    .ok()?;
    if output.success {
        let v = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Some(v)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_binding() -> jackin_protocol::ExecBinding {
        jackin_protocol::ExecBinding {
            name: "TOKEN".to_owned(),
            kind: jackin_protocol::ExecKind::Op,
            source: "op://vault/item/field".to_owned(),
        }
    }

    #[test]
    fn empty_exec_bindings_are_supported() {
        validate_exec_bindings(&[]).expect("no credential relay is required");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_exec_bindings_are_supported() {
        validate_exec_bindings(&[test_binding()]).expect("Linux peer auth is available");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_exec_bindings_are_rejected_explicitly() {
        let error = validate_exec_bindings(&[test_binding()])
            .expect_err("non-Linux peer auth is unavailable");
        let message = format!("{error:#}");
        assert!(message.contains("apple-container does not support on-demand credential bindings"));
        assert!(message.contains("peer authentication is unavailable"));
    }
}
