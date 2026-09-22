// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! `wait_for_dind` is already shared with `attach`; this module is the
//! single-file counterpart that creates the network and starts the sidecar.

use crate::runtime::attach::wait_for_dind;
use crate::runtime::naming::{
    LABEL_KIND_DIND, LABEL_KIND_PREWARM_DIND, LABEL_MANAGED, LABEL_PREWARM,
};
use anyhow::Context as _;
use fs4::FileExt;
use jackin_core::JackinPaths;
use jackin_core::{ContainerHandle, ContainerSpec};
use jackin_docker::docker_client::{ContainerState, DockerApi};
use serde::{Deserialize, Serialize};

pub const DIND_IMAGE: &str = crate::runtime::docker_profile::DIND_PRIVILEGED_IMAGE;
const PREWARM_CONTAINER_BASE: &str = "jk-prewarm-dind";
const PREWARM_STATE_FILE: &str = "prewarm-dind.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DindSidecarPrewarm {
    pub dind: String,
    /// Immutable daemon ID captured when the retained sidecar was created.
    pub dind_id: String,
    pub network: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DindSidecarPrewarmState {
    pub schema_version: u8,
    pub dind: String,
    pub dind_id: String,
    pub network: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
    pub created_at_ms: u128,
}

pub(super) struct AdoptedDindSidecar {
    pub sidecar: DindSidecarPrewarm,
    pub dind_handle: ContainerHandle,
    _lock: std::fs::File,
}

enum DindSidecarOwner<'a> {
    Role(&'a str),
    Prewarm,
}

impl DindSidecarOwner<'_> {
    fn labels(&self, kind: Option<&'static str>) -> std::collections::HashMap<String, String> {
        let labels: Vec<String> = match self {
            Self::Role(container_name) => kind.map_or_else(
                || {
                    vec![
                        LABEL_MANAGED.to_owned(),
                        format!("jackin.role={container_name}"),
                    ]
                },
                |kind| {
                    vec![
                        LABEL_MANAGED.to_owned(),
                        kind.to_owned(),
                        format!("jackin.role={container_name}"),
                    ]
                },
            ),
            Self::Prewarm => vec![
                LABEL_MANAGED.to_owned(),
                LABEL_KIND_PREWARM_DIND.to_owned(),
                LABEL_PREWARM.to_owned(),
            ],
        };
        labels
            .iter()
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                (k.to_owned(), v.to_owned())
            })
            .collect()
    }
}

/// Create the Docker network and start the `DinD` sidecar container.
///
/// This lets fresh launches overlap sidecar startup with other foreground
/// requirements, such as workspace materialization, while keeping the same
/// `DockerApi` calls and diagnostics.
pub(super) async fn run_dind_sidecar_headless(
    container_name: &str,
    network: &str,
    dind: &str,
    certs_volume: &str,
    grant: crate::runtime::docker_profile::DindGrant,
    dind_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Role(container_name),
        network,
        dind,
        certs_volume,
        grant,
        Some(dind_handle_slot),
        docker,
    )
    .await
}

/// `docker.create_network` wrapped in the shared `sidecar`/`create_network`
/// timing span. `create_network` is idempotent, hence the `created_or_exists`
/// success label.
async fn create_network_timed(
    network: &str,
    labels: std::collections::HashMap<String, String>,
    internal: bool,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "create_network",
        Some(network),
    );
    let result = docker.create_network(network, labels, internal).await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "create_network",
        if result.is_ok() {
            Some("created_or_exists")
        } else {
            Some("error")
        },
    );
    result
}

pub(crate) async fn create_role_network(
    container_name: &str,
    network: &str,
    internal: bool,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let labels = DindSidecarOwner::Role(container_name).labels(None);
    create_network_timed(network, labels, internal, docker).await
}

async fn run_dind_sidecar_headless_with_owner(
    owner: DindSidecarOwner<'_>,
    network: &str,
    dind: &str,
    certs_volume: &str,
    grant: crate::runtime::docker_profile::DindGrant,
    dind_handle_slot: Option<std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    // WP4 Part B: image + privileged flag are tier-aware. `rootless` uses the
    // rootless DinD image without `--privileged`; `privileged` keeps the
    // standard DinD image + `--privileged` path.
    let (dind_image, dind_privileged) =
        crate::runtime::docker_profile::dind_image_and_privileged(grant);
    // Create Docker network (sidecar networks are never internal).
    create_network_timed(network, owner.labels(None), false, docker).await?;

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "dind_image_lookup",
        Some(dind_image),
    );
    let dind_image_tags = docker.list_image_tags(dind_image).await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "dind_image_lookup",
        match &dind_image_tags {
            Ok(tags) if tags.is_empty() => Some("missing"),
            Ok(_) => Some("present"),
            Err(_) => Some("error"),
        },
    );
    if dind_image_tags?.is_empty() {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "pull_dind_image",
            Some(dind_image),
        );
        let pull_dind_image = docker.pull_image(dind_image);
        let pull_dind_image_result = pull_dind_image.await;
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "pull_dind_image",
            if pull_dind_image_result.is_ok() {
                Some("pulled")
            } else {
                Some("error")
            },
        );
        pull_dind_image_result?;
    }

    // Start Docker-in-Docker with TLS.
    //
    // `DOCKER_TLS_SAN` is read by docker:dind's `dockerd-entrypoint.sh` and
    // appended to the auto-generated server cert's Subject Alternative Names.
    // Without it, the cert only covers the short container ID, `docker`, and
    // `localhost` — so roles connecting via `tcp://{dind}:2376` get a TLS
    // hostname-mismatch error.
    //
    // The entrypoint concatenates `DOCKER_TLS_SAN` into the openssl config
    // verbatim (no type prefix added), so the value must already be in the
    // `DNS:<name>` form that openssl's `subjectAltName` section expects.
    // Without the prefix, openssl aborts with `v2i_GENERAL_NAME_ex: missing
    // value` and `DinD` never comes up.
    let certs_dind_mount = format!("{certs_volume}:/certs/client");
    let dind_tls_san = format!("DOCKER_TLS_SAN=DNS:{dind}");
    let labels = owner.labels(Some(LABEL_KIND_DIND));
    let spec = ContainerSpec {
        image: dind_image.to_owned(),
        hostname: None,
        env: vec!["DOCKER_TLS_CERTDIR=/certs".to_owned(), dind_tls_san],
        labels,
        network: network.to_owned(),
        binds: vec![certs_dind_mount],
        entrypoint: None,
        privileged: dind_privileged,
        workdir: None,
        ..Default::default()
    };
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "docker_create_dind",
        Some(dind),
    );
    let create_dind = docker.create_container(dind, spec);
    let create_dind_result = create_dind.await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "docker_create_dind",
        if create_dind_result.is_ok() {
            Some("created")
        } else {
            Some("error")
        },
    );
    let dind_handle = create_dind_result?;
    if let Some(slot) = &dind_handle_slot {
        *slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dind_handle.clone());
    }

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "docker_start_dind",
        Some(dind),
    );
    let start_dind = docker.start_container_by_id(&dind_handle);
    let start_dind_result = start_dind.await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "docker_start_dind",
        if start_dind_result.is_ok() {
            Some("started")
        } else {
            Some("error")
        },
    );
    start_dind_result?;

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "wait_dind_ready",
        Some(dind),
    );
    let dind_ready = wait_for_dind(&dind_handle, certs_volume, docker);
    let dind_ready_result = dind_ready.await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "wait_dind_ready",
        if dind_ready_result.is_ok() {
            Some("ready")
        } else {
            Some("error")
        },
    );
    dind_ready_result?;
    Ok(())
}

/// Prewarm a sidecar after validating any persisted retained-sidecar identity.
/// A same-name replacement must stop this operation before stale cleanup can
/// inspect or remove it.
pub async fn prewarm_dind_sidecar_container_with_paths(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    keep: bool,
) -> anyhow::Result<DindSidecarPrewarm> {
    let stale_dind_handle = if keep {
        ensure_prewarm_state_identity(paths, docker).await?
    } else {
        let base = format!("{PREWARM_CONTAINER_BASE}-{}", std::process::id());
        let dind = crate::instance::naming::dind_container_name(&base);
        crate::runtime::cleanup::resolve_optional_container_handle(docker, &dind).await?
    };
    prewarm_dind_sidecar_container_inner(docker, keep, stale_dind_handle).await
}

async fn prewarm_dind_sidecar_container_inner(
    docker: &impl DockerApi,
    keep: bool,
    stale_dind_handle: Option<ContainerHandle>,
) -> anyhow::Result<DindSidecarPrewarm> {
    let base = if keep {
        PREWARM_CONTAINER_BASE.to_owned()
    } else {
        let suffix = std::process::id();
        format!("{PREWARM_CONTAINER_BASE}-{suffix}")
    };
    let dind = crate::instance::naming::dind_container_name(&base);
    let network = crate::instance::naming::role_network_name(&base);
    let certs_volume = format!("{base}-certs");

    super::emit_prewarm_launch_plan(if keep {
        "sidecar_container_prewarm:keep"
    } else {
        "sidecar_container_prewarm"
    });

    let stale_cleanup_degraded = [
        match stale_dind_handle.as_ref() {
            Some(handle) => docker.remove_container_by_id(handle).await,
            None => Ok(()),
        },
        docker.remove_volume(&certs_volume).await,
        docker.remove_network(&network).await,
    ]
    .into_iter()
    .any(|result| result.is_err());
    if stale_cleanup_degraded {
        let _warning = jackin_telemetry::record_recovered_degradation();
    }

    let started = std::time::Instant::now();
    // Prewarm warms the privileged DinD path (the only one a prewarmed sidecar
    // can be adopted into today); a rootless launch starts its own sidecar.
    let dind_handle_slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let result = run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Prewarm,
        &network,
        &dind,
        &certs_volume,
        crate::runtime::docker_profile::DindGrant::Privileged,
        Some(std::sync::Arc::clone(&dind_handle_slot)),
        docker,
    )
    .await;
    let ready_ms = started.elapsed().as_millis();

    if result.is_err() || !keep {
        let dind_handle = dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let remove_container = match dind_handle.as_ref() {
            Some(container) => docker.remove_container_by_id(container).await,
            None => Err(anyhow::anyhow!(
                "DinD identity unavailable; aborting prewarm cleanup before container teardown for {dind}"
            )),
        };
        let remove_volume = docker.remove_volume(&certs_volume).await;
        let remove_network = docker.remove_network(&network).await;

        result?;
        remove_container?;
        remove_volume?;
        remove_network?;
    } else {
        result?;
    }

    let dind_id = dind_handle_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .map(|handle| handle.id().to_owned())
        .ok_or_else(|| anyhow::anyhow!("DinD create returned no immutable identity"))?;

    Ok(DindSidecarPrewarm {
        dind,
        dind_id,
        network,
        certs_volume,
        ready_ms,
        kept: keep,
    })
}

async fn ensure_prewarm_state_identity(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let Some(state) = read_prewarmed_dind_state(paths)
        .map_err(|reason| anyhow::anyhow!("cannot validate retained DinD identity: {reason}"))?
    else {
        let prewarm_dind = crate::instance::naming::dind_container_name(PREWARM_CONTAINER_BASE);
        let inspection = docker.inspect_container_by_name(&prewarm_dind).await;
        return match inspection.handle {
            None if matches!(inspection.state, ContainerState::NotFound) => Ok(None),
            Some(actual) => anyhow::bail!(
                "prewarm container {} exists without a verified retained identity {}; cannot authorize prewarm cleanup",
                prewarm_dind,
                actual.id()
            ),
            None => anyhow::bail!(
                "cannot validate prewarm container {}: {}",
                prewarm_dind,
                inspection.state.inspect_label()
            ),
        };
    };
    anyhow::ensure!(
        state.schema_version == 2 && state.kept,
        "retained DinD state is not a current kept identity; cannot authorize prewarm cleanup"
    );
    let expected = prewarmed_dind_expected_handle(&state)
        .ok_or_else(|| anyhow::anyhow!("retained DinD state has no immutable identity"))?;
    let inspection = docker.inspect_container_by_name(&state.dind).await;
    match inspection.handle {
        Some(actual) if actual == expected => Ok(Some(expected)),
        Some(actual) => anyhow::bail!(
            "retained DinD identity changed for {} (expected {}, found {}); cannot authorize prewarm cleanup",
            state.dind,
            expected.id(),
            actual.id()
        ),
        None if matches!(inspection.state, ContainerState::NotFound) => Ok(None),
        None => anyhow::bail!(
            "cannot validate retained DinD identity for {}: {}",
            state.dind,
            inspection.state.inspect_label()
        ),
    }
}

pub fn write_prewarmed_dind_state(
    paths: &JackinPaths,
    warmed: &DindSidecarPrewarm,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("creating {}", paths.data_dir.display()))?;
    let state = DindSidecarPrewarmState {
        schema_version: 2,
        dind: warmed.dind.clone(),
        dind_id: warmed.dind_id.clone(),
        network: warmed.network.clone(),
        certs_volume: warmed.certs_volume.clone(),
        ready_ms: warmed.ready_ms,
        kept: warmed.kept,
        created_at_ms: current_time_ms(),
    };
    let path = prewarmed_dind_state_path(paths);
    let json = serde_json::to_vec_pretty(&state)?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn read_prewarmed_dind_state(
    paths: &JackinPaths,
) -> Result<Option<DindSidecarPrewarmState>, &'static str> {
    let path = prewarmed_dind_state_path(paths);
    let json = match std::fs::read_to_string(&path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => {
            record_recovered_degradation();
            return Err("state-read-error");
        }
    };
    serde_json::from_str(&json).map(Some).map_err(|_error| {
        record_recovered_degradation();
        "state-parse-error"
    })
}

fn remove_prewarmed_dind_state(paths: &JackinPaths) {
    let path = prewarmed_dind_state_path(paths);
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        record_recovered_degradation();
    }
}

fn prewarmed_dind_state_path(paths: &JackinPaths) -> std::path::PathBuf {
    paths.data_dir.join(PREWARM_STATE_FILE)
}

fn current_time_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn prewarmed_dind_state_age_ms(state: &DindSidecarPrewarmState) -> u128 {
    current_time_ms().saturating_sub(state.created_at_ms)
}

fn prewarmed_dind_state_detail(reason: &str, state: &DindSidecarPrewarmState) -> String {
    format!(
        "{reason};source=state;state_age_ms={};prewarm_ready_ms={}",
        prewarmed_dind_state_age_ms(state),
        state.ready_ms
    )
}

fn prewarmed_dind_expected_handle(state: &DindSidecarPrewarmState) -> Option<ContainerHandle> {
    ContainerHandle::new(state.dind.clone(), state.dind_id.clone()).ok()
}

#[cfg(not(test))]
pub(crate) async fn prewarmed_dind_state_is_live(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> bool {
    let Ok(Some(state)) = read_prewarmed_dind_state(paths) else {
        return false;
    };
    if state.schema_version != 2 || !state.kept {
        return false;
    }
    let Some(expected_handle) = prewarmed_dind_expected_handle(&state) else {
        return false;
    };
    let inspection = docker.inspect_container_by_name(&state.dind).await;
    let Some(dind_handle) = inspection.handle else {
        return false;
    };
    if dind_handle != expected_handle {
        return false;
    }
    if !matches!(
        docker.inspect_container_by_id(&dind_handle).await,
        ContainerState::Running
    ) {
        return false;
    }
    let Ok(Some(network_row)) = docker.inspect_network(&state.network).await else {
        return false;
    };
    if network_row.labels.get("jackin.kind").map(String::as_str) != Some("prewarm-dind") {
        return false;
    }
    wait_for_dind(&dind_handle, &state.certs_volume, docker)
        .await
        .is_ok()
}

pub(crate) fn prewarmed_dind_state_container_name(paths: &JackinPaths) -> Option<String> {
    let Ok(Some(state)) = read_prewarmed_dind_state(paths) else {
        return None;
    };
    (state.schema_version == 2 && state.kept).then_some(state.dind)
}

fn record_prewarm_adoption_skip(reason: &str) {
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "adopt_prewarmed_dind",
        Some(&format!("skip:{reason}")),
    );
    emit_prewarmed_dind_adoption("skipped", reason);
}

/// Opportunistically consume the explicit kept sidecar prewarm as a one-shot
/// launch resource. The warmed resource names are recorded in the instance
/// manifest and normal session/eject cleanup owns them after launch succeeds.
#[expect(
    clippy::too_many_lines,
    reason = "identity validation, readiness, and adoption must remain one atomic ownership handoff"
)]
pub(super) async fn adopt_prewarmed_dind_sidecar(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> Option<AdoptedDindSidecar> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "adopt_prewarmed_dind",
        Some(PREWARM_STATE_FILE),
    );
    let Some(lock) = try_lock_prewarmed_dind(paths) else {
        record_prewarm_adoption_skip("locked");
        return None;
    };
    let state = match read_prewarmed_dind_state(paths) {
        Ok(Some(state)) if state.schema_version == 2 && state.kept => state,
        Ok(Some(_)) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some("skip:state-invalid"),
            );
            emit_prewarmed_dind_adoption("skipped", "state-invalid");
            remove_prewarmed_dind_state(paths);
            return None;
        }
        Ok(None) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some("skip:state-missing"),
            );
            emit_prewarmed_dind_adoption("skipped", "state-missing");
            return None;
        }
        Err(reason) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some(&format!("skip:{reason}")),
            );
            emit_prewarmed_dind_adoption("skipped", reason);
            remove_prewarmed_dind_state(paths);
            return None;
        }
    };
    let dind = state.dind.clone();
    let network = state.network.clone();
    let certs_volume = state.certs_volume.clone();
    let Some(expected_handle) = prewarmed_dind_expected_handle(&state) else {
        record_prewarm_adoption_skip("state-invalid-identity");
        return None;
    };

    let inspection = docker.inspect_container_by_name(&dind).await;
    let Some(inspected_handle) = inspection.handle else {
        let reason = format!("container:{}", inspection.state.short_label());
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "adopt_prewarmed_dind",
            Some(&format!("skip:{reason}")),
        );
        emit_prewarmed_dind_adoption("skipped", &prewarmed_dind_state_detail(&reason, &state));
        remove_prewarmed_dind_state(paths);
        return None;
    };
    if inspected_handle != expected_handle {
        // Keep the state file so GC continues to reserve this name. Removing
        // it would let a later best-effort prewarm cleanup mistake a
        // same-name replacement for the retained sidecar.
        record_prewarm_adoption_skip("container:identity-mismatch");
        return None;
    }
    let dind_handle = expected_handle;
    let dind_state = docker.inspect_container_by_id(&dind_handle).await;
    if !matches!(dind_state, ContainerState::Running) {
        let reason = format!("container:{}", dind_state.short_label());
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "adopt_prewarmed_dind",
            Some(&format!("skip:{reason}")),
        );
        emit_prewarmed_dind_adoption("skipped", &prewarmed_dind_state_detail(&reason, &state));
        remove_prewarmed_dind_state(paths);
        return None;
    }

    let network_row = match docker.inspect_network(&network).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some("skip:network-missing"),
            );
            emit_prewarmed_dind_adoption(
                "skipped",
                &prewarmed_dind_state_detail("network-missing", &state),
            );
            remove_prewarmed_dind_state(paths);
            return None;
        }
        Err(_error) => {
            record_recovered_degradation();
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some("skip:network-inspect-error"),
            );
            emit_prewarmed_dind_adoption(
                "skipped",
                &prewarmed_dind_state_detail("network-inspect-error", &state),
            );
            remove_prewarmed_dind_state(paths);
            return None;
        }
    };
    if network_row.labels.get("jackin.kind").map(String::as_str) != Some("prewarm-dind") {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "adopt_prewarmed_dind",
            Some("skip:network-label-mismatch"),
        );
        emit_prewarmed_dind_adoption(
            "skipped",
            &prewarmed_dind_state_detail("network-label-mismatch", &state),
        );
        remove_prewarmed_dind_state(paths);
        return None;
    }

    let started = std::time::Instant::now();
    if let Err(_error) = wait_for_dind(&dind_handle, &certs_volume, docker).await {
        record_recovered_degradation();
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "adopt_prewarmed_dind",
            Some("skip:not-ready"),
        );
        emit_prewarmed_dind_adoption("skipped", &prewarmed_dind_state_detail("not-ready", &state));
        remove_prewarmed_dind_state(paths);
        return None;
    }
    let ready_ms = started.elapsed().as_millis();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Sidecar,
        "adopt_prewarmed_dind",
        Some("adopted"),
    );
    emit_prewarmed_dind_adoption(
        "adopted",
        &format!(
            "ready_ms={ready_ms};source=state;state_age_ms={};prewarm_ready_ms={}",
            prewarmed_dind_state_age_ms(&state),
            state.ready_ms
        ),
    );
    remove_prewarmed_dind_state(paths);
    Some(AdoptedDindSidecar {
        sidecar: DindSidecarPrewarm {
            dind,
            dind_id: dind_handle.id().to_owned(),
            network,
            certs_volume,
            ready_ms,
            kept: true,
        },
        dind_handle,
        _lock: lock,
    })
}

pub(crate) fn try_lock_prewarmed_dind(paths: &JackinPaths) -> Option<std::fs::File> {
    if let Err(_error) = std::fs::create_dir_all(&paths.data_dir) {
        record_recovered_degradation();
        return None;
    }
    let lock_path = paths.data_dir.join("prewarm-dind-adoption.lock");
    let lock = match std::fs::File::create(&lock_path) {
        Ok(lock) => lock,
        Err(_error) => {
            record_recovered_degradation();
            return None;
        }
    };
    if let Err(_error) = FileExt::try_lock(&lock) {
        record_recovered_degradation();
        return None;
    }
    Some(lock)
}

fn record_recovered_degradation() {
    let _warning = jackin_telemetry::record_recovered_degradation();
}

fn emit_prewarmed_dind_adoption(outcome: &str, detail: &str) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "prewarmed_dind_adoption",
            jackin_diagnostics::DiagnosticStage::Sidecar,
            outcome,
            Some(detail),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_core::JackinPaths;
    use jackin_test_support::FakeDockerClient;
    use tempfile::tempdir;

    #[tokio::test]
    async fn retained_sidecar_rejects_same_name_replacement() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        write_prewarmed_dind_state(
            &paths,
            &DindSidecarPrewarm {
                dind: "jk-prewarm-dind-dind".to_owned(),
                dind_id: "original-dind-id".to_owned(),
                network: "jk-prewarm-dind-net".to_owned(),
                certs_volume: "jk-prewarm-dind-certs".to_owned(),
                ready_ms: 1,
                kept: true,
            },
        )
        .unwrap();

        let docker = FakeDockerClient::default();
        docker.container_id_by_name.borrow_mut().insert(
            "jk-prewarm-dind-dind".to_owned(),
            "replacement-dind-id".to_owned(),
        );
        docker
            .inspect_state_by_name
            .borrow_mut()
            .insert("jk-prewarm-dind-dind".to_owned(), ContainerState::Running);

        assert!(
            adopt_prewarmed_dind_sidecar(&paths, &docker)
                .await
                .is_none()
        );
        assert!(
            paths.data_dir.join(PREWARM_STATE_FILE).exists(),
            "mismatched state must remain reserved so GC cannot remove the replacement"
        );
        assert!(
            docker.bound_operations.borrow().is_empty(),
            "identity mismatch must not issue ID-bound lifecycle operations"
        );
        let error = prewarm_dind_sidecar_container_with_paths(&paths, &docker, true)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot authorize prewarm cleanup"),
            "replacement must block stale prewarm cleanup: {error:#}"
        );
        assert!(
            !docker
                .recorded
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("docker rm")),
            "replacement guard must issue no destructive Docker operation"
        );
    }
}
