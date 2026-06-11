//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! `wait_for_dind` is already shared with `attach`; this module is the
//! single-file counterpart that creates the network and starts the sidecar.

use crate::runtime::attach::wait_for_dind;
use crate::runtime::naming::{LABEL_KIND_DIND, LABEL_MANAGED};
use jackin_core::ContainerSpec;
use jackin_docker::docker_client::DockerApi;

pub const DIND_IMAGE: &str = "docker:dind";

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
