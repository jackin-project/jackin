// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Apple Container backend launch, attach, reconnect, eject, and purge.
//!
//! All lifecycle operations shell out to the `container` CLI via
//! the shared process transport, unlike the Docker backend which uses bollard.
//!
//! The running-state probe lives in
//! `jackin_runtime_apple_container_running` (S7 split 108),
//! imported below.
//!
//! The post-attach outcome recorder lives in
//! `jackin_runtime_apple_container_attach_outcome` (S7 split 109),
//! imported below.
//!
//! The interactive attach step lives in
//! `jackin_runtime_apple_container_attach` (S7 split 110),
//! imported below.
//!
//! The capsule readiness wait lives in
//! `jackin_runtime_apple_container_wait` (S7 split 111),
//! imported below.
//!
//! The `container` CLI version probe lives in
//! `jackin_runtime_apple_container_probe_version` (S7 split 112),
//! imported below.
//!
//! The started-entry activation lives in
//! `jackin_runtime_apple_container_activate_entry` (S7 split 113),
//! imported below.
//!
//! The supervisor env builder lives in
//! `jackin_runtime_apple_container_supervisor_env` (S7 split 114),
//! imported below.
//!
//! The DNS health check lives in
//! `jackin_runtime_apple_container_check_dns` (S7 split 118),
//! imported below.
//!
//! The session contract printer lives in
//! `jackin_runtime_apple_container_session_contract` (S7 split 119),
//! imported below.
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

// Moved to jackin_runtime_apple_container_session_contract::session_contract
// (S7 split 119); the private import keeps the in-file call site
// (`launch`) compiling unchanged.
use jackin_runtime_apple_container_session_contract::session_contract::print_session_contract;

// Moved to jackin_runtime_apple_container_check_dns::check_dns (S7
// split 118); the private import keeps the in-file call site
// (`launch`) compiling unchanged.
use jackin_runtime_apple_container_check_dns::check_dns::check_dns;

// Moved to jackin_runtime_apple_container_wait::wait
// (S7 split 111); the private import keeps both in-file call sites
// (`launch`, `reconnect`) compiling unchanged.
use jackin_runtime_apple_container_wait::wait::wait_for_capsule;

// Moved to jackin_runtime_apple_container_attach::attach
// (S7 split 110); the private import keeps both in-file call sites
// (`launch`, `reconnect`) compiling unchanged.
use jackin_runtime_apple_container_attach::attach::attach;

// Moved to jackin_runtime_apple_container_attach_outcome::attach_outcome
// (S7 split 109); the private import keeps both in-file call sites
// (`launch`, `reconnect`) compiling unchanged.
use jackin_runtime_apple_container_attach_outcome::attach_outcome::record_attach_outcome;

// Moved to jackin_runtime_apple_container_probe_version::probe_version
// (S7 split 112); the private import keeps the in-file call site
// (`launch`) compiling unchanged.
use jackin_runtime_apple_container_probe_version::probe_version::probe_version;

// Moved to jackin_runtime_apple_container_activate_entry::activate_entry
// (S7 split 113); the private import keeps the in-file call site
// (`launch`) compiling unchanged.
use jackin_runtime_apple_container_activate_entry::activate_entry::activate_started_entry;

// Moved to jackin_runtime_apple_container_supervisor_env::supervisor_env
// (S7 split 114); the private import keeps the in-file call site
// (`launch`) compiling unchanged.
use jackin_runtime_apple_container_supervisor_env::supervisor_env::apple_supervisor_env;

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
    pub credential_scope: &'a jackin_protocol::usage_broker::UsageCredentialScope,
    pub debug: bool,
    pub entry_claim: Option<&'a super::universe::EntryClaim>,
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
        credential_scope,
        debug,
        entry_claim,
    } = args;

    anyhow::ensure!(
        state.provider_config_mounts.is_empty(),
        "generated provider configuration requires read-only file overlays; the apple-container backend rejects single-file bind mounts; use the docker backend"
    );
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
    let mut env = apple_supervisor_env(debug);
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
                credential_scope,
            ),
        })
        .await
        .context("starting scoped usage relay")?;
    prepared_usage_relay.apply_to_launch_config(&mut capsule_config)?;
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

    let run_result = with_admitted_final_apple_spec(paths, state, spec, |spec| async move {
        crate::apple_container_client::AppleContainerClient::new()
            .run_container(container_name, &spec)
            .await
    })?
    .await;
    drop(host_env_file);
    activate_started_entry(run_result, entry_claim).await?;
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

// Moved to jackin_runtime_apple_container_running::running (S7 split
// 108); the private import keeps both in-file call sites
// (`reconnect`, `record_attach_outcome`) compiling unchanged.
use jackin_runtime_apple_container_running::running::is_container_running;

/// Reconnect to a stopped or running apple/container container.
pub async fn reconnect(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    entry_claim: Option<&super::universe::EntryClaim>,
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

    if let Some(claim) = entry_claim {
        claim
            .activate()
            .await
            .context("activating running launch entry")?;
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

#[cfg(test)]
mod tests;

/// Final complete-spec admission boundary, immediately before Apple creation.
fn with_admitted_final_apple_spec<T>(
    paths: &JackinPaths,
    state: &crate::instance::RoleState,
    spec: crate::apple_container_client::AppleContainerSpec,
    submit: impl FnOnce(crate::apple_container_client::AppleContainerSpec) -> T,
) -> Result<T> {
    super::launch::ensure_apple_provider_authority_not_exposed(
        state,
        &spec.mounts,
        &[paths.home_dir.join(".jackin-coordination")],
    )?;
    Ok(submit(spec))
}
