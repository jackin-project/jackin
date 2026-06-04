//! Apple Container backend launch, attach, reconnect, eject, and purge.
//!
//! Implements Phase 2–3 of the apple-container-backend roadmap item.
//! All lifecycle operations shell out to the `container` CLI via
//! `tokio::process::Command` — unlike the Docker backend which uses bollard.
//!
//! # Prerequisites
//!
//! - macOS 26 ARM with `apple/container` installed
//! - `JACKIN_CAPSULE_FORCE_DAEMON=1` injected at `container run` time
//!   (NOT a static Dockerfile ENV — that breaks the Docker backend)
//!
//! # DinD gating
//!
//! DinD inside the VM (rootless DinD via `--cap-add`) requires Phase 0
//! empirical validation. `inner_docker_enabled` defaults to `false` until
//! Phase 0 results confirm DinD works inside apple/container VMs.

use anyhow::{Context as _, Result, bail};
use std::path::PathBuf;

use crate::apple_container_client::AppleContainerApi as _;
use crate::instance::{
    AppleContainerResources, BackendResources, InstanceManifest, NewInstanceManifest,
};
use crate::paths::JackinPaths;

const ATTACH_MAX_WAIT_MS: u64 = 60_000;
const ATTACH_POLL_MS: u64 = 500;

/// Print the session contract — the security boundary summary shown to the
/// operator before the interactive attach begins (Phase 4).
///
/// Equivalent to the `docker profile: hardened` output block in the Docker
/// hardening contract, but for the apple-container backend.
pub fn print_session_contract(
    container_name: &str,
    image: &str,
    provider_version: &str,
    mount_pairs: &[(std::path::PathBuf, std::path::PathBuf)],
    debug: bool,
) {
    let mounts_summary: String = if mount_pairs.is_empty() {
        "none".to_string()
    } else {
        mount_pairs
            .iter()
            .map(|(h, g)| format!("  {}:{}", h.display(), g.display()))
            .collect::<Vec<_>>()
            .join("\n")
    };

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
    eprintln!("  mounts ({}):", mount_pairs.len());
    if mount_pairs.is_empty() {
        eprintln!("    (none)");
    } else {
        for (h, g) in mount_pairs {
            eprintln!("    {}:{}", h.display(), g.display());
        }
    }
    eprintln!("  network:              per-container IP via vmnet (no port mapping)");
    eprintln!("  dns:                  may hiccup after macOS sleep/wake — reconnect if affected");
    eprintln!("  residual risk:");
    eprintln!("    DinD not enabled; Docker workflows inside the VM require Phase 0 validation.");
    eprintln!("    apple/container vminitd is PID 1; signal forwarding relies on gRPC/vsock.");
    eprintln!("    Build-time Docker (image build) still runs on host Docker engine.");
    if debug {
        eprintln!("  debug mode:           on (JACKIN_DEBUG=1)");
    }
    eprintln!();
    let _ = mounts_summary; // used above inline
}

/// DNS health check — called after attach returns to detect sleep/wake hiccup.
/// Phase 4: "Detect DNS failure in capsule; surface 'reconnect required' to operator."
pub async fn check_dns(container_name: &str) {
    let result = tokio::process::Command::new("container")
        .args([
            "exec",
            container_name,
            "sh",
            "-c",
            "nslookup github.com >/dev/null 2>&1 && echo ok || echo hiccup",
        ])
        .output()
        .await;

    match result {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            crate::debug_log!("apple-container", "dns_check result={out}");
            if out == "hiccup" {
                eprintln!(
                    "[jackin] apple-container: DNS hiccup detected after sleep/wake. \
                     If the agent cannot reach the network, run `jackin hardline` to reconnect."
                );
            }
        }
        _ => {
            crate::debug_log!("apple-container", "dns_check result=unavailable");
        }
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

        let output = tokio::process::Command::new("container")
            .args(["exec", container_name, "sh", "-c", check_cmd])
            .output()
            .await;

        match output {
            Ok(o) if o.status.success() => return Ok(()),
            _ => {
                tokio::time::sleep(tokio::time::Duration::from_millis(ATTACH_POLL_MS)).await;
            }
        }
    }
}

/// Attach interactively to a running apple/container container.
/// Uses `container exec -it <name> jackin-capsule` which provides
/// a proper PTY with SIGWINCH forwarding via vminitd's gRPC/vsock layer.
pub async fn attach(container_name: &str, focus_session: Option<u64>) -> Result<()> {
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container_name,
        "/jackin/runtime/jackin-capsule",
    ];

    let focus_str;
    if let Some(id) = focus_session {
        focus_str = id.to_string();
        args.push("--focus");
        args.push(&focus_str);
    }

    crate::debug_log!(
        "apple-container",
        "attach transport=container-exec name={container_name} pty=yes"
    );

    let status = tokio::process::Command::new("container")
        .args(&args)
        .status()
        .await
        .context("container exec failed — is apple/container installed?")?;

    crate::tui::reassert_alt_screen();

    // Non-zero exit from the capsule (operator detached, role exited) — treat
    // as clean so the caller can proceed to cleanup.
    let _ = status;
    Ok(())
}

/// Full launch path for the `apple-container` backend.
///
/// Called from `load_role_with` after the image build step when the resolved
/// backend is `"apple-container"`.
#[allow(clippy::too_many_arguments)]
pub async fn launch(
    paths: &JackinPaths,
    container_name: &str,
    image: &str,
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
    role_key: &str,
    role_display_name: &str,
    agent: crate::agent::Agent,
    role_source_git: &str,
    role_source_ref: Option<&str>,
    image_tag: &str,
    env_pairs: &[(String, String)],
    mount_pairs: &[(PathBuf, PathBuf)],
    host_workdir_fingerprint: &str,
    capsule_config: &jackin_protocol::CapsuleConfig,
    debug: bool,
) -> Result<()> {
    crate::debug_log!(
        "apple-container",
        "container_run name={container_name} image={image} force_daemon=yes inner_docker=no"
    );

    // Probe container CLI availability.
    let version = probe_version().await;
    crate::debug_log!(
        "apple-container",
        "container_version version={}",
        version.as_deref().unwrap_or("not found")
    );
    if version.is_none() {
        bail!(
            "apple/container CLI (`container`) not found. \
             Install from https://github.com/apple/container or via Homebrew."
        );
    }

    let mut run_args: Vec<String> = vec![
        "run".to_string(),
        "--name".to_string(),
        container_name.to_string(),
        "-d".to_string(),
    ];

    // JACKIN_CAPSULE_FORCE_DAEMON=1 — daemon mode without PID 1 (vminitd is PID 1).
    run_args.push("-e".to_string());
    run_args.push("JACKIN_CAPSULE_FORCE_DAEMON=1".to_string());

    if debug {
        run_args.push("-e".to_string());
        run_args.push("JACKIN_DEBUG=1".to_string());
    }

    // Inject env vars (skip ones we already inject above).
    for (k, v) in env_pairs {
        if k == "JACKIN_CAPSULE_FORCE_DAEMON" || k == "JACKIN_DEBUG" {
            continue;
        }
        run_args.push("-e".to_string());
        run_args.push(format!("{k}={v}"));
    }

    // Bind mounts.
    for (host, guest) in mount_pairs {
        crate::debug_log!(
            "apple-container",
            "mount source={} guest={} mode=rw",
            host.display(),
            guest.display()
        );
        run_args.push("-v".to_string());
        run_args.push(format!("{}:{}", host.display(), guest.display()));
    }

    // socket dir bind-mount so /jackin/run/host.sock is reachable inside.
    let socket_dir = paths.jackin_home.join("sockets").join(container_name);
    std::fs::create_dir_all(&socket_dir)?;
    run_args.push("-v".to_string());
    run_args.push(format!("{}:/jackin/run", socket_dir.display()));

    run_args.push(image.to_string());
    run_args.push("/jackin/runtime/jackin-capsule".to_string());

    let output = tokio::process::Command::new("container")
        .args(&run_args)
        .output()
        .await
        .context("container run failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Fail-closed: if `container run` fails (e.g. unsupported --cap-add,
        // container CLI not installed, image not found), surface the exact error.
        // Do not silently fall back to Docker or swallow the failure.
        bail!(
            "container run failed — required capabilities or image may be unavailable: {}",
            stderr.trim()
        );
    }
    crate::debug_log!("apple-container", "container_run name={container_name} ok");

    // Compact launch telemetry line (always-on tier, equivalent to clog! in capsule).
    crate::debug_log!(
        "apple-container",
        "apple-container launch name={container_name} image={} inner_docker=none caps=0 mounts={}",
        image,
        mount_pairs.len()
    );

    // Write instance manifest.
    let container_state = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::new(NewInstanceManifest {
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
        backend: BackendResources::AppleContainer(AppleContainerResources {
            container_name: container_name.to_string(),
            role_image_ref: image_tag.to_string(),
            inner_docker_enabled: false, // gated on Phase 0 DinD validation
        }),
    });
    manifest.write(&container_state)?;
    crate::debug_log!(
        "apple-container",
        "manifest written container={container_name}"
    );

    // Start host.sock credential resolver before the blocking attach call.
    let host_sock_path = paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join("host.sock");
    let allowed_bindings: Vec<crate::exec_host::ExecCredRef> = capsule_config
        .exec_bindings
        .iter()
        .map(|b| crate::exec_host::ExecCredRef {
            name: b.name.clone(),
            kind: b.kind.clone(),
            source: b.source.clone(),
        })
        .collect();
    let _exec_host_handle = crate::exec_host::start(host_sock_path, allowed_bindings);

    // Wait for capsule daemon readiness.
    wait_for_capsule(container_name).await?;
    crate::debug_log!("apple-container", "capsule ready name={container_name}");

    // Phase 4: Session contract output.
    // Printed once after the container starts, before the interactive attach.
    // Shows the security boundary, isolation model, and residual risks so the
    // operator knows exactly what is enforced before their session begins.
    print_session_contract(
        container_name,
        image,
        version.as_deref().unwrap_or("unknown"),
        mount_pairs,
        debug,
    );

    // Interactive attach — blocks until operator detaches.
    attach(container_name, None).await?;

    // Phase 4: DNS check after attach returns. Catches sleep/wake DNS hiccup.
    check_dns(container_name).await;

    Ok(())
}

/// Check whether an apple/container container is currently running.
/// Delegates to `AppleContainerClient::list_containers` which owns all
/// JSON parsing for `container ps` output.
async fn is_container_running(container_name: &str) -> bool {
    crate::apple_container_client::AppleContainerClient::new()
        .list_containers(container_name)
        .await
        .ok()
        .map(|v| v.iter().any(|c| c.name == container_name && c.is_running()))
        .unwrap_or(false)
}

/// Reconnect to a stopped or running apple/container container.
pub async fn reconnect(container_name: &str, focus_session: Option<u64>) -> Result<()> {
    let running = is_container_running(container_name).await;

    if !running {
        crate::debug_log!(
            "apple-container",
            "container_state action=start name={container_name}"
        );
        let start = tokio::process::Command::new("container")
            .args(["start", container_name])
            .output()
            .await
            .context("container start failed — is apple/container installed?")?;
        if !start.status.success() {
            let stderr = String::from_utf8_lossy(&start.stderr);
            bail!("container start failed: {}", stderr.trim());
        }
        crate::debug_log!(
            "apple-container",
            "container_state action=start name={container_name} result=ok"
        );
    }

    wait_for_capsule(container_name).await?;
    attach(container_name, focus_session).await
}

/// Stop the container (eject — preserves manifest).
pub async fn stop(container_name: &str) -> Result<()> {
    crate::debug_log!(
        "apple-container",
        "container_state action=stop name={container_name}"
    );
    let output = tokio::process::Command::new("container")
        .args(["stop", container_name])
        .output()
        .await
        .context("container stop failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        crate::debug_log!(
            "apple-container",
            "container_state action=stop name={container_name} result=failure reason={}",
            stderr.trim()
        );
        bail!("container stop failed: {}", stderr.trim());
    }
    crate::debug_log!(
        "apple-container",
        "container_state action=stop name={container_name} result=ok"
    );
    Ok(())
}

/// Remove the container (purge).
pub async fn remove(container_name: &str) -> Result<()> {
    // Stop first (ignore errors — may already be stopped).
    let _ = stop(container_name).await;

    crate::debug_log!(
        "apple-container",
        "container_state action=rm name={container_name}"
    );
    let output = tokio::process::Command::new("container")
        .args(["rm", container_name])
        .output()
        .await
        .context("container rm failed")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        crate::debug_log!(
            "apple-container",
            "container_state action=rm name={container_name} result=failure reason={}",
            stderr.trim()
        );
        bail!("container rm failed: {}", stderr.trim());
    }
    crate::debug_log!(
        "apple-container",
        "container_state action=rm name={container_name} result=ok"
    );
    Ok(())
}

/// Probe the `container` CLI version. Returns `None` if not installed.
pub async fn probe_version() -> Option<String> {
    let output = tokio::process::Command::new("container")
        .arg("--version")
        .output()
        .await
        .ok()?;
    if output.status.success() {
        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
        crate::debug_log!("apple-container", "container_version version={v}");
        Some(v)
    } else {
        None
    }
}
