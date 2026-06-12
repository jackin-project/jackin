//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! `wait_for_dind` is already shared with `attach`; this module is the
//! single-file counterpart that creates the network and starts the sidecar.

use crate::runtime::attach::wait_for_dind;
use crate::runtime::naming::{LABEL_KIND_DIND, LABEL_MANAGED};
use jackin_core::ContainerSpec;
use jackin_docker::docker_client::DockerApi;

pub const DIND_IMAGE: &str = "docker:dind";
const PREWARM_CONTAINER_BASE: &str = "jk-prewarm-dind";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DindSidecarPrewarm {
    pub dind: String,
    pub network: String,
    pub certs_volume: String,
    pub ready_ms: u128,
    pub kept: bool,
}

/// Create the Docker network and start the `DinD` sidecar container.
///
/// This lets fresh launches overlap sidecar startup with other foreground
/// requirements, such as workspace materialization, while keeping the same
/// DockerApi calls and diagnostics.
pub(super) async fn run_dind_sidecar_headless(
    container_name: &str,
    network: &str,
    dind: &str,
    certs_volume: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    // Create Docker network
    let role_label = format!("jackin.role={container_name}");
    let network_labels = [LABEL_MANAGED, role_label.as_str()]
        .iter()
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect();
    jackin_diagnostics::active_timing_started("sidecar", "create_network", Some(network));
    let create_network_result = docker.create_network(network, network_labels).await;
    jackin_diagnostics::active_timing_done(
        "sidecar",
        "create_network",
        if create_network_result.is_ok() {
            Some("created_or_exists")
        } else {
            Some("error")
        },
    );
    create_network_result?;

    jackin_diagnostics::active_timing_started("sidecar", "dind_image_lookup", Some(DIND_IMAGE));
    let dind_image_tags = docker.list_image_tags(DIND_IMAGE).await;
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
        jackin_diagnostics::active_timing_started("sidecar", "pull_dind_image", Some(DIND_IMAGE));
        let pull_dind_image = docker.pull_image(DIND_IMAGE);
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
    let labels = [LABEL_MANAGED, LABEL_KIND_DIND, role_label.as_str()]
        .iter()
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect();
    let spec = ContainerSpec {
        image: DIND_IMAGE.to_owned(),
        hostname: None,
        env: vec!["DOCKER_TLS_CERTDIR=/certs".to_owned(), dind_tls_san],
        labels,
        network: network.to_owned(),
        binds: vec![certs_dind_mount],
        entrypoint: None,
        privileged: true,
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

/// Start a disposable DinD sidecar through the same path fresh launches use,
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
    let result = run_dind_sidecar_headless(&base, &network, &dind, &certs_volume, docker).await;
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[tokio::test]
    async fn sidecar_container_prewarm_records_prewarm_plan() {
        let temp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(temp.path());
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
