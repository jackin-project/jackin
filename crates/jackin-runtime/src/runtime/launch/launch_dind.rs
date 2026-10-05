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

/// A durable lifetime reservation established before polling any Docker create.
/// Namespace writer admission must remain held through every call using it.
#[derive(Debug)]
pub(crate) struct SharedDockerCreationCustody {
    paths: JackinPaths,
    lifetime: std::sync::Mutex<crate::instance::SharedDockerLifetime>,
}

impl SharedDockerCreationCustody {
    pub(crate) fn reserve(paths: &JackinPaths, lifetime: crate::instance::SharedDockerLifetime) -> anyhow::Result<std::sync::Arc<Self>> {
        lifetime.save_pending(paths)?;
        Ok(std::sync::Arc::new(Self { paths: paths.clone(), lifetime: std::sync::Mutex::new(lifetime) }))
    }

    pub(crate) fn adopt(paths: &JackinPaths, lifetime: crate::instance::SharedDockerLifetime) -> anyhow::Result<std::sync::Arc<Self>> {
        lifetime.save(paths)?;
        Ok(std::sync::Arc::new(Self { paths: paths.clone(), lifetime: std::sync::Mutex::new(lifetime) }))
    }

    pub(crate) fn snapshot(&self) -> crate::instance::SharedDockerLifetime {
        self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    pub(crate) fn admitted_snapshot(&self) -> anyhow::Result<crate::instance::SharedDockerLifetime> {
        let captured = self.snapshot();
        let durable = crate::instance::SharedDockerLifetime::load(&self.paths, captured.daemon_server_id(), captured.owner())?
            .context("shared Docker lifetime custody is missing")?;
        anyhow::ensure!(durable == captured, "shared Docker lifetime capture is not durably committed");
        Ok(captured)
    }

    async fn verify_daemon(&self, docker: &impl DockerApi) -> anyhow::Result<()> {
        let actual = docker.daemon_server_id().await?;
        anyhow::ensure!(&actual == self.snapshot().daemon_server_id(), "Docker daemon server identity changed during shared resource lifetime");
        Ok(())
    }

    pub(crate) fn labels(&self) -> std::collections::HashMap<String, String> {
        let lifetime = self.snapshot();
        std::collections::HashMap::from([
            ("jackin.shared-generation".to_owned(), lifetime.generation().to_owned()),
            ("jackin.shared-owner".to_owned(), lifetime.namespace_owner().to_owned()),
            ("jackin.managed".to_owned(), "true".to_owned()),
        ])
    }

    fn capture_network(&self, id: jackin_core::NetworkId) -> anyhow::Result<()> {
        let mut lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        lifetime.capture_network(id)?;
        lifetime.save(&self.paths)
    }

    fn capture_certs_volume(&self) -> anyhow::Result<()> {
        let mut lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        lifetime.capture_certs_volume()?;
        lifetime.save(&self.paths)
    }

    pub(crate) fn capture_container(&self, dind: bool, id: &str) -> anyhow::Result<()> {
        let mut lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        lifetime.capture_container(dind, id)?;
        lifetime.save(&self.paths)
    }

    pub(crate) fn replace_snapshot(&self, replacement: crate::instance::SharedDockerLifetime) -> anyhow::Result<()> {
        let mut lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        anyhow::ensure!(replacement.daemon_server_id() == lifetime.daemon_server_id()
            && replacement.generation() == lifetime.generation()
            && replacement.owner() == lifetime.owner(),
            "shared Docker lifetime snapshot identity changed");
        anyhow::ensure!(crate::instance::SharedDockerLifetime::load_for_cleanup(
            &self.paths, replacement.daemon_server_id(), replacement.owner()
        )?.as_ref() == Some(&replacement), "replacement custody snapshot is not durable");
        *lifetime = replacement;
        Ok(())
    }

    fn begin_retirement(&self) -> anyhow::Result<crate::instance::SharedDockerLifetime> {
        let mut lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *lifetime = lifetime.begin_retirement(&self.paths)?;
        Ok(lifetime.clone())
    }

    fn retire(&self) -> anyhow::Result<()> {
        let lifetime = self.lifetime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        lifetime.retire(&self.paths)
    }
}

pub const DIND_IMAGE: &str = crate::runtime::docker_profile::DIND_PRIVILEGED_IMAGE;
const PREWARM_CONTAINER_BASE: &str = "jk-prewarm-dind";
const PREWARM_STATE_FILE: &str = "prewarm-dind.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DindSidecarPrewarm {
    pub dind: String,
    /// Immutable daemon ID captured when the retained sidecar was created.
    pub dind_id: String,
    pub network: String,
    pub network_id: jackin_core::NetworkId,
    pub lifetime_owner: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DindSidecarPrewarmState {
    pub schema_version: u8,
    pub dind: String,
    #[serde(default)]
    pub dind_id: Option<String>,
    pub network: String,
    #[serde(default)]
    pub network_id: Option<jackin_core::NetworkId>,
    #[serde(default)]
    pub lifetime_owner: Option<String>,
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
    network_id_slot: std::sync::Arc<std::sync::Mutex<Option<jackin_core::NetworkId>>>,
    shared_custody: std::sync::Arc<SharedDockerCreationCustody>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Role(container_name),
        network,
        dind,
        certs_volume,
        grant,
        Some(dind_handle_slot),
        network_id_slot,
        shared_custody,
        docker,
    )
    .await
}

/// `docker.create_network` wrapped in the shared `sidecar`/`create_network`
/// timing span. Ownership is captured only from successful creation.
async fn create_network_timed(
    network: &str,
    labels: std::collections::HashMap<String, String>,
    internal: bool,
    docker: &impl DockerApi,
) -> anyhow::Result<jackin_core::NetworkId> {
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
            Some("created")
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
    network_id_slot: std::sync::Arc<std::sync::Mutex<Option<jackin_core::NetworkId>>>,
    shared_custody: std::sync::Arc<SharedDockerCreationCustody>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    shared_custody.verify_daemon(docker).await?;
    let mut labels = DindSidecarOwner::Role(container_name).labels(None);
    labels.extend(shared_custody.labels());
    anyhow::ensure!(shared_custody.snapshot().network_name() == Some(network), "network creation differs from durable lifetime reservation");
    let id = create_network_timed(network, labels, internal, docker).await?;
    *network_id_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(id.clone());
    shared_custody.verify_daemon(docker).await?;
    shared_custody.capture_network(id)?;
    Ok(())
}

async fn run_dind_sidecar_headless_with_owner(
    owner: DindSidecarOwner<'_>,
    network: &str,
    dind: &str,
    certs_volume: &str,
    grant: crate::runtime::docker_profile::DindGrant,
    dind_handle_slot: Option<std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>>,
    network_id_slot: std::sync::Arc<std::sync::Mutex<Option<jackin_core::NetworkId>>>,
    shared_custody: std::sync::Arc<SharedDockerCreationCustody>,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    // WP4 Part B: image + privileged flag are tier-aware. `rootless` uses the
    // rootless DinD image without `--privileged`; `privileged` keeps the
    // standard DinD image + `--privileged` path.
    let (dind_image, dind_privileged) =
        crate::runtime::docker_profile::dind_image_and_privileged(grant);
    shared_custody.verify_daemon(docker).await?;
    // Create Docker network (sidecar networks are never internal).
    anyhow::ensure!(shared_custody.snapshot().network_name() == Some(network), "sidecar network differs from durable lifetime reservation");
    anyhow::ensure!(shared_custody.snapshot().certs_volume_name() == Some(certs_volume), "sidecar certificate volume differs from durable lifetime reservation");
    let mut network_labels = owner.labels(None);
    network_labels.extend(shared_custody.labels());
    let network_id = create_network_timed(network, network_labels, false, docker).await?;
    *network_id_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(network_id.clone());
    shared_custody.verify_daemon(docker).await?;
    shared_custody.capture_network(network_id)?;
    docker.create_volume(certs_volume, shared_custody.labels()).await?;
    shared_custody.verify_daemon(docker).await?;
    shared_custody.capture_certs_volume()?;

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
    let mut labels = owner.labels(Some(LABEL_KIND_DIND));
    labels.extend(shared_custody.labels());
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
    shared_custody.verify_daemon(docker).await?;
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
    shared_custody.verify_daemon(docker).await?;
    shared_custody.capture_container(true, dind_handle.id())?;
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
    shared_custody.verify_daemon(docker).await?;
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
    shared_custody.verify_daemon(docker).await?;

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
    shared_custody.verify_daemon(docker).await?;
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
    let _lock = try_lock_prewarmed_dind(paths)
        .await
        .context("another prewarm or adoption operation owns the prewarm writer")?;
    prewarm_dind_sidecar_container_under_lock(paths, docker, keep).await
}

pub(crate) async fn prewarm_dind_sidecar_container_under_lock(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    keep: bool,
) -> anyhow::Result<DindSidecarPrewarm> {
    crate::runtime::launch::ensure_controller_transport_supported(docker.controller_endpoint())?;
    let daemon_server_id = docker.daemon_server_id().await?;
    let state = read_prewarmed_dind_state(paths).map_err(anyhow::Error::msg)?;
    let owner = state.as_ref().and_then(|state| state.lifetime_owner.as_deref())
        .unwrap_or(PREWARM_CONTAINER_BASE);
    if let Some(lifetime) = crate::instance::SharedDockerLifetime::load_for_cleanup(paths, &daemon_server_id, owner)? {
        retire_prewarm_lifetime(paths, docker, lifetime, state.as_ref()).await?;
    } else if state.is_some() {
        anyhow::bail!("prewarm state has no shared lifetime journal; retaining all resources");
    }
    let owner = if keep { PREWARM_CONTAINER_BASE.to_owned() } else {
        let allocation = crate::instance::SharedDockerLifetime::fresh_prewarm(&daemon_server_id, PREWARM_CONTAINER_BASE)?;
        format!("{PREWARM_CONTAINER_BASE}-{}", allocation.generation())
    };
    let lifetime = crate::instance::SharedDockerLifetime::fresh_prewarm(&daemon_server_id, &owner)?;
    let shared_custody = SharedDockerCreationCustody::reserve(paths, lifetime)?;
    let warmed = prewarm_dind_sidecar_container_inner(paths, docker, keep, shared_custody).await?;
    if warmed.kept {
        write_prewarmed_dind_state(paths, &warmed)?;
    }
    Ok(warmed)
}

async fn retire_prewarm_lifetime(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    lifetime: crate::instance::SharedDockerLifetime,
    state: Option<&DindSidecarPrewarmState>,
) -> anyhow::Result<()> {
    anyhow::ensure!(lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Prewarm
        && lifetime.namespace_owner_kind() == crate::instance::SharedDockerOwnerKind::Prewarm,
        "prewarm journal has transferred to a role owner; refusing prewarm teardown");
    if let Some(state) = state {
        anyhow::ensure!(state.schema_version == 2 && state.kept,
            "prewarm projection is not a current retained state");
        anyhow::ensure!(state.lifetime_owner.as_deref() == Some(lifetime.owner())
            && state.dind == crate::instance::naming::dind_container_name(lifetime.namespace_owner())
            && lifetime.network_name() == Some(state.network.as_str())
            && lifetime.certs_volume_name() == Some(state.certs_volume.as_str()),
            "prewarm projection differs from durable lifetime");
        anyhow::ensure!(state.network_id.as_ref() == lifetime.network_id(),
            "prewarm network ID differs from durable lifetime");
        anyhow::ensure!(matches!(lifetime.dind_container(),
            crate::instance::SharedContainerCustody::Owned { name, id }
                if name == &state.dind && state.dind_id.as_deref() == Some(id.as_str())),
            "prewarm container ID differs from durable lifetime");
    }

    let plan = crate::runtime::cleanup::reconcile_shared_lifetime(paths, lifetime, docker).await?;
    crate::runtime::cleanup::ensure_shared_plan_current(paths, &plan, docker).await?;
    let retiring = plan.lifetime.begin_retirement(paths)?;
    let mut failures = Vec::new();
    if let Some(container) = plan.dind.as_ref() {
        if let Err(error) = docker.remove_container_by_id(container).await {
            failures.push(format!("DinD container: {error:#}"));
        }
    }
    if let Some(volume) = plan.volume.as_deref() {
        if let Err(error) = docker.remove_volume(volume).await {
            failures.push(format!("certificate volume: {error:#}"));
        } else if docker.inspect_volume_by_name(volume).await?.is_some() {
            failures.push(format!("certificate volume {volume} remains after removal"));
        }
    }
    if let Some(network) = plan.network.as_ref()
        && let Err(error) = docker.remove_network_by_id(network).await
    {
        failures.push(format!("network {}: {error:#}", network.as_str()));
    }
    if !failures.is_empty() {
        anyhow::bail!("prewarm cleanup failed; retiring custody retained: {}", failures.join("; "));
    }
    remove_prewarmed_dind_state_checked(paths)?;
    retiring.retire(paths)
}

async fn prewarm_dind_sidecar_container_inner(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    keep: bool,
    shared_custody: std::sync::Arc<SharedDockerCreationCustody>,
) -> anyhow::Result<DindSidecarPrewarm> {
    let lifetime = shared_custody.snapshot();
    let dind = crate::instance::naming::dind_container_name(lifetime.owner());
    let network = lifetime.network_name().context("prewarm network was disabled")?.to_owned();
    let certs_volume = lifetime.certs_volume_name().context("prewarm certificate volume was disabled")?.to_owned();

    super::emit_prewarm_launch_plan(if keep {
        "sidecar_container_prewarm:keep"
    } else {
        "sidecar_container_prewarm"
    });

    let started = std::time::Instant::now();
    // Prewarm warms the privileged DinD path (the only one a prewarmed sidecar
    // can be adopted into today); a rootless launch starts its own sidecar.
    let dind_handle_slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let network_id_slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let result = run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Prewarm,
        &network,
        &dind,
        &certs_volume,
        crate::runtime::docker_profile::DindGrant::Privileged,
        Some(std::sync::Arc::clone(&dind_handle_slot)),
        std::sync::Arc::clone(&network_id_slot),
        std::sync::Arc::clone(&shared_custody),
        docker,
    )
    .await;
    let ready_ms = started.elapsed().as_millis();

    if result.is_err() || !keep {
        let plan = crate::runtime::cleanup::reconcile_shared_lifetime(
            paths,
            shared_custody.snapshot(),
            docker,
        )
        .await?;
        shared_custody.replace_snapshot(plan.lifetime.clone())?;
        crate::runtime::cleanup::ensure_shared_plan_current(paths, &plan, docker).await?;
        shared_custody.verify_daemon(docker).await?;
        shared_custody.begin_retirement()?;
        let mut cleanup_errors = Vec::new();
        if let Some(container) = plan.dind.as_ref()
            && let Err(error) = docker.remove_container_by_id(container).await
        {
            cleanup_errors.push(format!("DinD container: {error:#}"));
        }
        if let Some(volume) = plan.volume.as_deref() {
            if let Err(error) = docker.remove_volume(volume).await {
                cleanup_errors.push(format!("certificate volume: {error:#}"));
            } else if docker.inspect_volume_by_name(volume).await?.is_some() {
                cleanup_errors.push(format!("certificate volume {volume} remains after removal"));
            }
        }
        if let Some(network) = plan.network.as_ref()
            && let Err(error) = docker.remove_network_by_id(network).await
        {
            cleanup_errors.push(format!("network {}: {error:#}", network.as_str()));
        }
        if !cleanup_errors.is_empty() {
            if let Err(error) = result {
                anyhow::bail!("{error:#}; prewarm rollback also failed: {}", cleanup_errors.join("; "));
            }
            anyhow::bail!("prewarm rollback failed: {}", cleanup_errors.join("; "));
        }
        shared_custody.retire()?;
        result?;
    } else {
        result?;
    }

    let final_lifetime = shared_custody.snapshot();
    let dind_id = match final_lifetime.dind_container() {
        crate::instance::SharedContainerCustody::Owned { id, .. } => id.clone(),
        _ => anyhow::bail!("DinD create returned no immutable identity"),
    };
    let network_id = final_lifetime.network_id().cloned()
        .ok_or_else(|| anyhow::anyhow!("network creation returned no immutable identity"))?;
    Ok(DindSidecarPrewarm {
        dind,
        dind_id,
        network,
        network_id,
        lifetime_owner: final_lifetime.owner().to_owned(),
        certs_volume,
        ready_ms,
        kept: keep,
    })
}

#[cfg(test)]
pub(super) async fn ensure_prewarm_state_identity(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let Some(state) = load_prewarmed_dind_state_with_identity(paths, docker).await? else {
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

async fn load_prewarmed_dind_state_with_identity(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<DindSidecarPrewarmState>> {
    let Some(mut state) = read_prewarmed_dind_state(paths)
        .map_err(|reason| anyhow::anyhow!("cannot validate retained DinD identity: {reason}"))?
    else {
        return Ok(None);
    };
    if state.schema_version != 1 || !state.kept {
        return Ok(Some(state));
    }

    // Schema 1 stored only mutable names. Recover the daemon ID only when the
    // exact named row still carries all three Jackin prewarm ownership labels.
    let rows = docker
        .list_containers(&[LABEL_MANAGED, LABEL_KIND_PREWARM_DIND], true)
        .await
        .context("listing legacy retained DinD containers")?;
    let matching = rows
        .iter()
        .filter(|row| row.name == state.dind)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matching.len() == 1,
        "legacy retained DinD identity is ambiguous or missing; state preserved"
    );
    let row = matching[0];
    for label in [LABEL_MANAGED, LABEL_KIND_PREWARM_DIND, LABEL_PREWARM] {
        let (key, value) = label.split_once('=').unwrap_or((label, ""));
        anyhow::ensure!(
            row.labels.get(key).map(String::as_str) == Some(value),
            "legacy retained DinD ownership labels do not match; state preserved"
        );
    }
    let handle = row.handle().context("legacy retained DinD has no ID")?;
    let state_by_id = docker.inspect_container_by_id(&handle).await;
    anyhow::ensure!(
        !matches!(
            state_by_id,
            ContainerState::NotFound | ContainerState::InspectUnavailable(_)
        ),
        "legacy retained DinD identity cannot be inspected; state preserved"
    );
    state.schema_version = 2;
    state.dind_id = Some(handle.id().to_owned());
    persist_prewarmed_dind_state(paths, &state)?;
    Ok(Some(state))
}

pub fn write_prewarmed_dind_state(
    paths: &JackinPaths,
    warmed: &DindSidecarPrewarm,
) -> anyhow::Result<()> {
    let state = DindSidecarPrewarmState {
        schema_version: 2,
        dind: warmed.dind.clone(),
        dind_id: Some(warmed.dind_id.clone()),
        network: warmed.network.clone(),
        network_id: Some(warmed.network_id.clone()),
        lifetime_owner: Some(warmed.lifetime_owner.clone()),
        certs_volume: warmed.certs_volume.clone(),
        ready_ms: warmed.ready_ms,
        kept: warmed.kept,
        created_at_ms: current_time_ms(),
    };
    persist_prewarmed_dind_state(paths, &state)
}

fn persist_prewarmed_dind_state(
    paths: &JackinPaths,
    state: &DindSidecarPrewarmState,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("creating {}", paths.data_dir.display()))?;
    let path = prewarmed_dind_state_path(paths);
    let temp = path.with_extension("json.migrating");
    let json = serde_json::to_vec_pretty(state)?;
    std::fs::write(&temp, json).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, &path)
        .with_context(|| format!("atomically replacing {}", path.display()))
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

fn remove_prewarmed_dind_state_checked(paths: &JackinPaths) -> anyhow::Result<()> {
    let path = prewarmed_dind_state_path(paths);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("removing prewarm state at {}", path.display()));
        }
    }
    anyhow::ensure!(!path.exists(), "prewarm state retirement failed");
    Ok(())
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
    ContainerHandle::new(state.dind.clone(), state.dind_id.clone()?).ok()
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
    let Some(network_id) = state.network_id.as_ref() else { return false; };
    let Ok(Some(network_row)) = docker.inspect_network_by_id(network_id).await else {
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
    ((state.schema_version == 1 || state.schema_version == 2) && state.kept).then_some(state.dind)
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
    let Ok(daemon_server_id) = docker.daemon_server_id().await else {
        record_prewarm_adoption_skip("daemon:identity-unavailable");
        return None;
    };
    let Some(lock) = try_lock_prewarmed_dind(paths).await else {
        record_prewarm_adoption_skip("locked");
        return None;
    };
    let state = match load_prewarmed_dind_state_with_identity(paths, docker).await {
        Ok(Some(state)) if state.schema_version == 2 && state.kept => state,
        Ok(Some(_)) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some("skip:state-invalid"),
            );
            emit_prewarmed_dind_adoption("skipped", "state-invalid");
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
        Err(error) => {
            let reason = error.to_string();
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Sidecar,
                "adopt_prewarmed_dind",
                Some(&format!("skip:{reason}")),
            );
            emit_prewarmed_dind_adoption("skipped", &reason);
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
        return None;
    }

    let Some(network_id) = state.network_id.as_ref() else {
        record_prewarm_adoption_skip("network:identity-unavailable");
        return None;
    };
    let network_row = match docker.inspect_network_by_id(network_id).await {
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
        return None;
    }

    let Some(lifetime_owner) = state.lifetime_owner.as_deref() else {
        record_prewarm_adoption_skip("lifetime:owner-unavailable");
        return None;
    };
    let Ok(Some(lifetime)) = crate::instance::SharedDockerLifetime::load(paths, &daemon_server_id, lifetime_owner) else {
        record_prewarm_adoption_skip("lifetime:unavailable");
        return None;
    };
    let shared_labels = std::collections::HashMap::from([
        ("jackin.shared-generation".to_owned(), lifetime.generation().to_owned()),
        ("jackin.shared-owner".to_owned(), lifetime.namespace_owner().to_owned()),
        ("jackin.managed".to_owned(), "true".to_owned()),
    ]);
    if lifetime.network_id() != Some(network_id)
        || lifetime.network_name() != Some(network.as_str())
        || network_row.name != network
        || !shared_labels.iter().all(|(key, value)| network_row.labels.get(key) == Some(value))
        || !matches!(lifetime.certs_volume(), crate::instance::SharedCertsVolumeCustody::Owned { .. })
        || lifetime.certs_volume_name() != Some(certs_volume.as_str()) {
        record_prewarm_adoption_skip("lifetime:identity-mismatch");
        return None;
    }
    let Ok(Some(volume)) = docker.inspect_volume_by_name(&certs_volume).await else {
        record_prewarm_adoption_skip("volume:identity-unavailable");
        return None;
    };
    if volume.name != certs_volume || volume.labels != shared_labels || volume.driver != "local" {
        record_prewarm_adoption_skip("volume:identity-mismatch");
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
        return None;
    }
    if docker.daemon_server_id().await.ok().as_ref() != Some(&daemon_server_id) {
        record_prewarm_adoption_skip("daemon:identity-changed");
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
    Some(AdoptedDindSidecar {
        sidecar: DindSidecarPrewarm {
            dind,
            dind_id: dind_handle.id().to_owned(),
            network,
            network_id: state.network_id.clone()?,
            lifetime_owner: state.lifetime_owner.clone()?,
            certs_volume,
            ready_ms,
            kept: true,
        },
        dind_handle,
        _lock: lock,
    })
}

/// Remove the prewarm projection only after the single canonical lifetime owner changed durably.
pub(crate) fn commit_prewarm_state_transfer(paths: &JackinPaths) -> anyhow::Result<()> {
    remove_prewarmed_dind_state_checked(paths)
}

pub(crate) fn retire_prewarm_projection(
    paths: &JackinPaths,
    lifetime: &crate::instance::SharedDockerLifetime,
) -> anyhow::Result<()> {
    let Some(state) = read_prewarmed_dind_state(paths).map_err(anyhow::Error::msg)? else {
        return Ok(());
    };
    anyhow::ensure!(lifetime.owner_kind() == crate::instance::SharedDockerOwnerKind::Prewarm
        && state.schema_version == 2 && state.kept
        && state.lifetime_owner.as_deref() == Some(lifetime.owner())
        && state.network == lifetime.network_name().unwrap_or_default()
        && state.network_id.as_ref() == lifetime.network_id()
        && state.certs_volume == lifetime.certs_volume_name().unwrap_or_default(),
        "prewarm state projection differs from retiring lifetime");
    anyhow::ensure!(matches!(lifetime.dind_container(),
        crate::instance::SharedContainerCustody::Owned { name, id }
            if state.dind == *name && state.dind_id.as_deref() == Some(id.as_str())),
        "prewarm state container identity differs from retiring lifetime");
    remove_prewarmed_dind_state_checked(paths)
}

pub(crate) async fn try_lock_prewarmed_dind(paths: &JackinPaths) -> Option<std::fs::File> {
    let paths = paths.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        let lock = crate::runtime::coordination::open_lock(&paths, "prewarm-dind-adoption")?;
        FileExt::try_lock(&lock).map_err(std::io::Error::from)?;
        Ok::<_, std::io::Error>(lock)
    })
    .await;
    match result {
        Ok(Ok(lock)) => Some(lock),
        Ok(Err(_)) | Err(_) => {
            record_recovered_degradation();
            None
        }
    }
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
                network_id: jackin_core::NetworkId::parse(&"a".repeat(64)).unwrap(),
                lifetime_owner: "jk-prewarm-dind".to_owned(),
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

    #[tokio::test]
    async fn role_network_creation_uses_durable_fresh_lifetime() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = crate::instance::SharedDockerLifetime::fresh(&jackin_core::DaemonServerId::parse("jackin-test-daemon")?, "jk-owned-role", true, false)?;
        let network = lifetime.network_name().unwrap().to_owned();
        let custody = SharedDockerCreationCustody::reserve(&paths, lifetime)?;
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let docker = FakeDockerClient::default();
        create_role_network("jk-owned-role", &network, false, std::sync::Arc::clone(&slot), std::sync::Arc::clone(&custody), &docker).await?;
        let admitted = custody.admitted_snapshot()?;
        assert_eq!(admitted.network_id(), slot.lock().unwrap().as_ref());
        assert_eq!(docker.created_networks.borrow()[0].0, network);
        assert_eq!(docker.created_networks.borrow()[0].1.get("jackin.shared-generation").map(String::as_str), Some(admitted.generation()));
        Ok(())
    }

    #[tokio::test]
    async fn mismatched_role_network_reservation_has_no_docker_effects() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = crate::instance::SharedDockerLifetime::fresh(&jackin_core::DaemonServerId::parse("jackin-test-daemon")?, "jk-owned-role", true, false)?;
        let custody = SharedDockerCreationCustody::reserve(&paths, lifetime)?;
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let docker = FakeDockerClient::default();
        assert!(create_role_network("jk-owned-role", "jk-other-net", false, slot, custody, &docker).await.is_err());
        assert!(docker.recorded.borrow().iter().all(|operation| operation == "docker info"));
        Ok(())
    }


    #[tokio::test]
    async fn failed_network_capture_save_cannot_authorize_rollback() -> anyhow::Result<()> {
        fn find_record(directory: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Ok(path) = find_record(&entry.path()) { return Ok(path); }
                } else if entry.path().extension().is_some_and(|extension| extension == "json") {
                    return Ok(entry.path());
                }
            }
            anyhow::bail!("test lifetime record is missing")
        }
        let temp = tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = crate::instance::SharedDockerLifetime::fresh(&jackin_core::DaemonServerId::parse("jackin-test-daemon")?, "jk-failed-capture", true, false)?;
        let custody = SharedDockerCreationCustody::reserve(&paths, lifetime)?;
        let record = find_record(&paths.jackin_home.join("shared-docker-lifetimes"))?;
        let retained = record.with_extension("retained");
        std::fs::rename(&record, &retained)?;
        std::fs::create_dir(&record)?;
        let network_id = jackin_core::NetworkId::parse(&"b".repeat(64))?;
        assert!(custody.capture_network(network_id.clone()).is_err());
        assert!(custody.admitted_snapshot().is_err());
        let socket_dir = paths.jackin_home.join("sockets/jk-failed-capture");
        std::fs::create_dir_all(&socket_dir)?;
        std::fs::write(socket_dir.join("retained"), b"evidence")?;
        let cleanup = super::super::LoadCleanup::new(&paths, "jk-failed-capture".into(), "jk-failed-capture-dind".into(), "disabled-certs".into())?;
        cleanup.set_role_handle(jackin_core::ContainerHandle::new("jk-failed-capture", "captured-role-id")?);
        cleanup.set_network_id(network_id);
        cleanup.bind_shared_custody(custody);
        let docker = FakeDockerClient::default();
        cleanup.run(&docker).await;
        assert!(!docker.recorded.borrow().iter().any(|operation| operation.starts_with("docker rm") || operation.starts_with("docker volume rm") || operation.starts_with("docker network rm")));
        assert!(retained.is_file());
        assert!(record.is_dir());
        assert!(socket_dir.join("retained").is_file());
        Ok(())
    }


    #[tokio::test]
    async fn another_daemon_cannot_consume_reserved_network_lifetime() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let owner_daemon = jackin_core::DaemonServerId::parse("daemon-A")?;
        let lifetime = crate::instance::SharedDockerLifetime::fresh(&owner_daemon, "jk-daemon-bound", true, false)?;
        let network = lifetime.network_name().unwrap().to_owned();
        let custody = SharedDockerCreationCustody::reserve(&paths, lifetime)?;
        let docker = FakeDockerClient::default();
        docker.set_daemon_server_id(jackin_core::DaemonServerId::parse("daemon-B")?);
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        assert!(create_role_network("jk-daemon-bound", &network, false, slot, std::sync::Arc::clone(&custody), &docker).await.is_err());
        assert!(docker.created_networks.borrow().is_empty());
        assert!(matches!(custody.admitted_snapshot()?.network(), crate::instance::SharedNetworkCustody::Pending { .. }));
        Ok(())
    }

}
