//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! `wait_for_dind` is already shared with `attach`; this module is the
//! single-file counterpart that creates the network and starts the sidecar.

use crate::runtime::attach::wait_for_dind;
use crate::runtime::naming::{
    LABEL_KIND_DIND, LABEL_KIND_PREWARM_DIND, LABEL_MANAGED, LABEL_PREWARM,
};
use anyhow::Context as _;
use fs2::FileExt;
use jackin_core::ContainerSpec;
use jackin_core::paths::JackinPaths;
use jackin_docker::docker_client::{ContainerState, DockerApi};
use serde::{Deserialize, Serialize};

pub const DIND_IMAGE: &str = "docker:dind";
const PREWARM_CONTAINER_BASE: &str = "jk-prewarm-dind";
const PREWARM_STATE_FILE: &str = "prewarm-dind.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DindSidecarPrewarm {
    pub dind: String,
    pub network: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DindSidecarPrewarmState {
    pub schema_version: u8,
    pub dind: String,
    pub network: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
    pub created_at_ms: u128,
}

pub(super) struct AdoptedDindSidecar {
    pub sidecar: DindSidecarPrewarm,
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
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Role(container_name),
        network,
        dind,
        certs_volume,
        grant,
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
    jackin_diagnostics::active_timing_started("sidecar", "create_network", Some(network));
    let result = docker.create_network(network, labels, internal).await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
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
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    // WP4 Part B: image + privileged flag are tier-aware. `rootless` runs
    // `docker:dind-rootless` without `--privileged`; `privileged` keeps the
    // classic `docker:dind` + `--privileged` path.
    let (dind_image, dind_privileged) =
        crate::runtime::docker_profile::dind_image_and_privileged(grant);
    // Create Docker network (sidecar networks are never internal).
    create_network_timed(network, owner.labels(None), false, docker).await?;

    jackin_diagnostics::active_timing_started("sidecar", "dind_image_lookup", Some(dind_image));
    let dind_image_tags = docker.list_image_tags(dind_image).await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
        "dind_image_lookup",
        match &dind_image_tags {
            Ok(tags) if tags.is_empty() => Some("missing"),
            Ok(_) => Some("present"),
            Err(_) => Some("error"),
        },
    );
    if dind_image_tags?.is_empty() {
        jackin_diagnostics::active_timing_started("sidecar", "pull_dind_image", Some(dind_image));
        let pull_dind_image = docker.pull_image(dind_image);
        let pull_dind_image_result = pull_dind_image.await;
        jackin_diagnostics::active_timing_done(
            "sidecar",
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
    };
    jackin_diagnostics::active_timing_started("sidecar", "docker_create_dind", Some(dind));
    let create_dind = docker.create_container(dind, spec);
    let create_dind_result = create_dind.await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
        "docker_create_dind",
        if create_dind_result.is_ok() {
            Some("created")
        } else {
            Some("error")
        },
    );
    create_dind_result?;

    jackin_diagnostics::active_timing_started("sidecar", "docker_start_dind", Some(dind));
    let start_dind = docker.start_container(dind);
    let start_dind_result = start_dind.await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
        "docker_start_dind",
        if start_dind_result.is_ok() {
            Some("started")
        } else {
            Some("error")
        },
    );
    start_dind_result?;

    jackin_diagnostics::active_timing_started("sidecar", "wait_dind_ready", Some(dind));
    let dind_ready = wait_for_dind(dind, certs_volume, docker);
    let dind_ready_result = dind_ready.await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
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

/// Start a disposable `DinD` sidecar through the same path fresh launches use,
/// wait until Docker/TLS is ready, then tear it down. This warms the local
/// Docker daemon/container/cert-generation path without preserving a live
/// sidecar for a future role instance.
pub async fn prewarm_dind_sidecar_container(
    docker: &impl DockerApi,
    keep: bool,
) -> anyhow::Result<DindSidecarPrewarm> {
    let base = if keep {
        PREWARM_CONTAINER_BASE.to_owned()
    } else {
        let suffix = std::process::id();
        format!("{PREWARM_CONTAINER_BASE}-{suffix}")
    };
    let dind = format!("{base}-dind");
    let network = format!("{base}-net");
    let certs_volume = format!("{base}-certs");

    super::emit_prewarm_launch_plan(if keep {
        "sidecar_container_prewarm:keep"
    } else {
        "sidecar_container_prewarm"
    });

    let _remove_stale_dind = docker.remove_container(&dind).await;
    let _remove_stale_volume = docker.remove_volume(&certs_volume).await;
    let _remove_stale_network = docker.remove_network(&network).await;

    let started = std::time::Instant::now();
    // Prewarm warms the privileged DinD path (the only one a prewarmed sidecar
    // can be adopted into today); a rootless launch starts its own sidecar.
    let result = run_dind_sidecar_headless_with_owner(
        DindSidecarOwner::Prewarm,
        &network,
        &dind,
        &certs_volume,
        crate::runtime::docker_profile::DindGrant::Privileged,
        docker,
    )
    .await;
    let ready_ms = started.elapsed().as_millis();

    if result.is_err() || !keep {
        let remove_container = docker.remove_container(&dind).await;
        let remove_volume = docker.remove_volume(&certs_volume).await;
        let remove_network = docker.remove_network(&network).await;

        result?;
        remove_container?;
        remove_volume?;
        remove_network?;
    } else {
        result?;
    }

    Ok(DindSidecarPrewarm {
        dind,
        network,
        certs_volume,
        ready_ms,
        kept: keep,
    })
}

pub fn write_prewarmed_dind_state(
    paths: &JackinPaths,
    warmed: &DindSidecarPrewarm,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("creating {}", paths.data_dir.display()))?;
    let state = DindSidecarPrewarmState {
        schema_version: 1,
        dind: warmed.dind.clone(),
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
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "sidecar",
                "could not read prewarm sidecar state {}: {error:#}",
                path.display()
            );
            return Err("state-read-error");
        }
    };
    serde_json::from_str(&json).map(Some).map_err(|error| {
        jackin_diagnostics::debug_log!(
            "sidecar",
            "could not parse prewarm sidecar state {}: {error:#}",
            path.display()
        );
        "state-parse-error"
    })
}

fn remove_prewarmed_dind_state(paths: &JackinPaths) {
    let path = prewarmed_dind_state_path(paths);
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        jackin_diagnostics::debug_log!(
            "sidecar",
            "could not remove consumed prewarm sidecar state {}: {error:#}",
            path.display()
        );
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

/// Opportunistically consume the explicit kept sidecar prewarm as a one-shot
/// launch resource. The warmed resource names are recorded in the instance
/// manifest and normal session/eject cleanup owns them after launch succeeds.
pub(super) async fn adopt_prewarmed_dind_sidecar(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> Option<AdoptedDindSidecar> {
    jackin_diagnostics::active_timing_started(
        "sidecar",
        "adopt_prewarmed_dind",
        Some(PREWARM_STATE_FILE),
    );
    let Some(lock) = try_lock_prewarmed_dind(paths) else {
        jackin_diagnostics::active_timing_done(
            "sidecar",
            "adopt_prewarmed_dind",
            Some("skip:locked"),
        );
        emit_prewarmed_dind_adoption("skipped", "locked");
        return None;
    };
    let state = match read_prewarmed_dind_state(paths) {
        Ok(Some(state)) if state.schema_version == 1 && state.kept => state,
        Ok(Some(_)) => {
            jackin_diagnostics::active_timing_done(
                "sidecar",
                "adopt_prewarmed_dind",
                Some("skip:state-invalid"),
            );
            emit_prewarmed_dind_adoption("skipped", "state-invalid");
            remove_prewarmed_dind_state(paths);
            return None;
        }
        Ok(None) => {
            jackin_diagnostics::active_timing_done(
                "sidecar",
                "adopt_prewarmed_dind",
                Some("skip:state-missing"),
            );
            emit_prewarmed_dind_adoption("skipped", "state-missing");
            return None;
        }
        Err(reason) => {
            jackin_diagnostics::active_timing_done(
                "sidecar",
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

    match docker.inspect_container_state(&dind).await {
        ContainerState::Running => {}
        docker_state => {
            let reason = format!("container:{}", docker_state.short_label());
            jackin_diagnostics::active_timing_done(
                "sidecar",
                "adopt_prewarmed_dind",
                Some(&format!("skip:{reason}")),
            );
            emit_prewarmed_dind_adoption("skipped", &prewarmed_dind_state_detail(&reason, &state));
            remove_prewarmed_dind_state(paths);
            return None;
        }
    }

    let network_row = match docker.inspect_network(&network).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            jackin_diagnostics::active_timing_done(
                "sidecar",
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
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "sidecar",
                "could not inspect kept prewarm network {network}: {error:#}"
            );
            jackin_diagnostics::active_timing_done(
                "sidecar",
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
            "sidecar",
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
    if let Err(error) = wait_for_dind(&dind, &certs_volume, docker).await {
        jackin_diagnostics::debug_log!(
            "sidecar",
            "kept prewarm dind {dind} was running but not ready: {error:#}"
        );
        jackin_diagnostics::active_timing_done(
            "sidecar",
            "adopt_prewarmed_dind",
            Some("skip:not-ready"),
        );
        emit_prewarmed_dind_adoption("skipped", &prewarmed_dind_state_detail("not-ready", &state));
        remove_prewarmed_dind_state(paths);
        return None;
    }
    let ready_ms = started.elapsed().as_millis();
    jackin_diagnostics::active_timing_done("sidecar", "adopt_prewarmed_dind", Some("adopted"));
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
            network,
            certs_volume,
            ready_ms,
            kept: true,
        },
        _lock: lock,
    })
}

fn try_lock_prewarmed_dind(paths: &JackinPaths) -> Option<std::fs::File> {
    if let Err(error) = std::fs::create_dir_all(&paths.data_dir) {
        jackin_diagnostics::debug_log!(
            "sidecar",
            "could not create prewarm sidecar lock dir {}: {error:#}",
            paths.data_dir.display()
        );
        return None;
    }
    let lock_path = paths.data_dir.join("prewarm-dind-adoption.lock");
    let lock = match std::fs::File::create(&lock_path) {
        Ok(lock) => lock,
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "sidecar",
                "could not open prewarm sidecar lock {}: {error:#}",
                lock_path.display()
            );
            return None;
        }
    };
    if let Err(error) = lock.try_lock_exclusive() {
        jackin_diagnostics::debug_log!(
            "sidecar",
            "prewarm sidecar lock {} is held by another launch: {error:#}",
            lock_path.display()
        );
        return None;
    }
    Some(lock)
}

fn emit_prewarmed_dind_adoption(outcome: &str, detail: &str) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage("prewarmed_dind_adoption", "sidecar", outcome, Some(detail));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn sidecar_container_prewarm_starts_ready_and_cleans_up() {
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![DIND_IMAGE.to_owned()]);
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        let warmed = prewarm_dind_sidecar_container(&docker, false)
            .await
            .unwrap();
        assert!(warmed.dind.starts_with("jk-prewarm-dind-"));
        assert!(!warmed.kept);
        assert!(warmed.ready_ms < 60_000);

        let recorded = docker.recorded.borrow();
        let create_network = format!("docker network create {}", warmed.network);
        let create_dind = format!("create_container:{}", warmed.dind);
        let start_dind = format!("start_container:{}", warmed.dind);
        let docker_info = format!("docker exec {} docker info", warmed.dind);
        let remove_dind = format!("docker rm -f {}", warmed.dind);
        let remove_volume = format!("docker volume rm {}", warmed.certs_volume);
        let remove_network = format!("docker network rm {}", warmed.network);

        for expected in [
            &create_network,
            &create_dind,
            &start_dind,
            &docker_info,
            &remove_dind,
            &remove_volume,
            &remove_network,
        ] {
            assert!(
                recorded.iter().any(|call| call == expected),
                "missing `{expected}` in {recorded:?}"
            );
        }
        assert!(
            recorded
                .iter()
                .position(|call| call == &docker_info)
                .unwrap()
                < recorded
                    .iter()
                    .rposition(|call| call == &remove_dind)
                    .unwrap(),
            "prewarm must wait ready before cleanup: {recorded:?}"
        );
    }

    #[tokio::test]
    async fn sidecar_container_prewarm_can_keep_ready_resources() {
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![DIND_IMAGE.to_owned()]);
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        let warmed = prewarm_dind_sidecar_container(&docker, true).await.unwrap();
        assert_eq!(warmed.dind, "jk-prewarm-dind-dind");
        assert_eq!(warmed.network, "jk-prewarm-dind-net");
        assert_eq!(warmed.certs_volume, "jk-prewarm-dind-certs");
        assert!(warmed.kept);

        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "start_container:jk-prewarm-dind-dind"),
            "persistent sidecar prewarm must start the warmed container: {recorded:?}"
        );
        let start_pos = recorded
            .iter()
            .position(|call| call == "start_container:jk-prewarm-dind-dind")
            .unwrap();
        assert!(
            !recorded
                .iter()
                .skip(start_pos)
                .any(|call| call == "docker rm -f jk-prewarm-dind-dind"),
            "persistent sidecar prewarm may remove stale resources before start, but must not remove ready resources: {recorded:?}"
        );
        let created = docker.created_containers.borrow();
        let (_, spec) = created
            .iter()
            .find(|(name, _)| name == "jk-prewarm-dind-dind")
            .expect("kept prewarm should create dind container");
        assert_eq!(
            spec.labels.get("jackin.kind").map(String::as_str),
            Some("prewarm-dind")
        );
        assert_eq!(
            spec.labels.get("jackin.prewarm").map(String::as_str),
            Some("true")
        );
        assert!(
            !spec.labels.contains_key("jackin.role"),
            "kept prewarm sidecar must not look orphaned by role-sidecar GC: {:?}",
            spec.labels
        );
    }

    #[tokio::test]
    async fn kept_sidecar_prewarm_writes_jackin_owned_state() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let warmed = DindSidecarPrewarm {
            dind: "jk-prewarm-dind-dind".to_owned(),
            network: "jk-prewarm-dind-net".to_owned(),
            certs_volume: "jk-prewarm-dind-certs".to_owned(),
            ready_ms: 42,
            kept: true,
        };

        write_prewarmed_dind_state(&paths, &warmed).unwrap();

        let json = std::fs::read_to_string(prewarmed_dind_state_path(&paths)).unwrap();
        let state: DindSidecarPrewarmState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.schema_version, 1);
        assert_eq!(state.dind, "jk-prewarm-dind-dind");
        assert_eq!(state.network, "jk-prewarm-dind-net");
        assert_eq!(state.certs_volume, "jk-prewarm-dind-certs");
        assert_eq!(state.ready_ms, 42);
        assert!(state.kept);
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_uses_ready_kept_resources() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        write_prewarmed_dind_state(
            &paths,
            &DindSidecarPrewarm {
                dind: "jk-prewarm-custom-dind".to_owned(),
                network: "jk-prewarm-custom-net".to_owned(),
                certs_volume: "jk-prewarm-custom-certs".to_owned(),
                ready_ms: 17,
                kept: true,
            },
        )
        .unwrap();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .inspect_queue
            .borrow_mut()
            .push_back(ContainerState::Running);
        let mut network_labels = HashMap::new();
        network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
        network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
        docker.inspect_network_queue.borrow_mut().push_back(Some(
            jackin_docker::docker_client::NetworkRow {
                name: "jk-prewarm-custom-net".to_owned(),
                labels: network_labels,
            },
        ));
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker)
            .await
            .expect("running ready prewarm sidecar should be adopted");

        assert_eq!(adopted.sidecar.dind, "jk-prewarm-custom-dind");
        assert_eq!(adopted.sidecar.network, "jk-prewarm-custom-net");
        assert_eq!(adopted.sidecar.certs_volume, "jk-prewarm-custom-certs");
        assert!(adopted.sidecar.kept);
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker inspect jk-prewarm-custom-dind"),
            "adoption must inspect state-recorded prewarm dind: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker network inspect jk-prewarm-custom-net"),
            "adoption must verify state-recorded prewarm network labels: {recorded:?}"
        );
        assert!(
            !recorded
                .iter()
                .any(|call| call == "create_container:jk-prewarm-custom-dind"),
            "adoption must not recreate the warmed sidecar: {recorded:?}"
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            jsonl.contains("\"kind\":\"prewarmed_dind_adoption\""),
            "{jsonl}"
        );
        assert!(jsonl.contains("adopted"), "{jsonl}");
        assert!(jsonl.contains("ready_ms="), "{jsonl}");
        assert!(jsonl.contains("source=state"), "{jsonl}");
        assert!(jsonl.contains("state_age_ms="), "{jsonl}");
        assert!(jsonl.contains("prewarm_ready_ms=17"), "{jsonl}");
        assert!(
            !prewarmed_dind_state_path(&paths).exists(),
            "adoption consumes stale daemon-prewarm state"
        );
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_skips_without_state_file() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        let docker = crate::runtime::test_support::FakeDockerClient::default();

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            docker.recorded.borrow().is_empty(),
            "missing daemon-prewarm state must skip before docker probes: {:?}",
            docker.recorded.borrow()
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("state-missing"), "{jsonl}");
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_removes_stale_state_when_container_missing() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        write_prewarmed_dind_state(
            &paths,
            &DindSidecarPrewarm {
                dind: "jk-prewarm-stale-dind".to_owned(),
                network: "jk-prewarm-stale-net".to_owned(),
                certs_volume: "jk-prewarm-stale-certs".to_owned(),
                ready_ms: 17,
                kept: true,
            },
        )
        .unwrap();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .inspect_queue
            .borrow_mut()
            .push_back(ContainerState::NotFound);

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            !prewarmed_dind_state_path(&paths).exists(),
            "stale prewarm state should be removed after a definitive Docker miss"
        );
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker inspect jk-prewarm-stale-dind"),
            "adoption must inspect state-recorded stale dind: {recorded:?}"
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("container:missing"), "{jsonl}");
        assert!(jsonl.contains("source=state"), "{jsonl}");
        assert!(jsonl.contains("state_age_ms="), "{jsonl}");
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_skips_when_lock_is_held() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let held = std::fs::File::create(paths.data_dir.join("prewarm-dind-adoption.lock"))
            .expect("lock file");
        held.try_lock_exclusive().expect("hold adoption lock");
        let docker = crate::runtime::test_support::FakeDockerClient::default();

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            docker.recorded.borrow().is_empty(),
            "contention must skip before docker probes so a second launch starts a private sidecar: {:?}",
            docker.recorded.borrow()
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            jsonl.contains("\"kind\":\"prewarmed_dind_adoption\""),
            "{jsonl}"
        );
        assert!(jsonl.contains("skipped"), "{jsonl}");
        assert!(jsonl.contains("locked"), "{jsonl}");
    }

    #[tokio::test]
    async fn sidecar_container_prewarm_records_prewarm_plan() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
        let _active = run.activate();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![DIND_IMAGE.to_owned()]);
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        prewarm_dind_sidecar_container(&docker, false)
            .await
            .unwrap();

        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"kind\":\"launch_plan\""), "{jsonl}");
        assert!(jsonl.contains("PrewarmOnly"), "{jsonl}");
        assert!(jsonl.contains("sidecar_container_prewarm"), "{jsonl}");
    }
}
