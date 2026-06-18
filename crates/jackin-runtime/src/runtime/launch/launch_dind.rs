//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! `wait_for_dind` is already shared with `attach`; this module is the
//! single-file counterpart that creates the network and starts the sidecar.

use crate::runtime::attach::wait_for_dind;
use crate::runtime::docker_profile::{probe_cgroup_version, DindGrant};
use crate::runtime::naming::{LABEL_KIND_DIND, LABEL_MANAGED};
use crate::runtime::progress::LaunchStage;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;

use super::StepCounter;

/// Create the Docker network and start the `DinD` sidecar container.
///
/// Called from `launch_role_runtime` before the role container itself
/// is started. `wait_for_dind` blocks until the `DinD` daemon reports
/// ready so subsequent `docker build` and `docker run` calls inside the
/// sidecar succeed.
///
/// `dind_grant` selects the tier: `Rootless` uses `docker:dind-rootless` and
/// drops `--privileged`; `Privileged` keeps the original `docker:dind` path.
/// `None` is never passed here — the caller skips this function when `DinD` is off.
#[expect(
    clippy::too_many_arguments,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) async fn run_dind_sidecar(
    container_name: &str,
    network: &str,
    dind: &str,
    certs_volume: &str,
    dind_grant: DindGrant,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    steps: &mut StepCounter,
    docker_run_opts: &RunOptions,
) -> anyhow::Result<()> {
    // Rootless DinD requires cgroup v2; fail-closed rather than silently
    // falling back to privileged (which would defeat the security tier choice).
    if dind_grant == DindGrant::Rootless && probe_cgroup_version() == "v1" {
        anyhow::bail!(
            "rootless DinD requires cgroup v2, but the host reports cgroup v1.\n\
             Switch to a cgroup v2 host or change the DinD grant to 'privileged'."
        );
    }

    // DinD network is never internal — it must reach the outer Docker daemon.
    create_role_network(container_name, network, false, docker, steps).await?;

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
    let role_label = format!("jackin.role={container_name}");

    let (dind_image, use_privileged) = match dind_grant {
        DindGrant::Rootless => ("docker:dind-rootless", false),
        _ => ("docker:dind", true),
    };

    let mut dind_args: Vec<&str> = vec!["run", "-d", "--name", dind, "--network", network];
    if use_privileged {
        dind_args.push("--privileged");
    }
    dind_args.extend_from_slice(&[
        "--label",
        LABEL_MANAGED,
        "--label",
        LABEL_KIND_DIND,
        "--label",
        &role_label,
        "-e",
        "DOCKER_TLS_CERTDIR=/certs",
        "-e",
        &dind_tls_san,
        "-v",
        &certs_dind_mount,
        dind_image,
    ]);

    let run_dind = runner.run("docker", &dind_args, None, docker_run_opts);
    if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(run_dind).await?;
    } else {
        run_dind.await?;
    }

    let dind_ready = wait_for_dind(dind, certs_volume, docker);
    if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(dind_ready).await?;
    } else {
        dind_ready.await?;
    }
    Ok(())
}

pub(super) async fn create_role_network(
    container_name: &str,
    network: &str,
    internal: bool,
    docker: &impl DockerApi,
    steps: &mut StepCounter,
) -> anyhow::Result<()> {
    let role_label = format!("jackin.role={container_name}");
    let network_labels = [LABEL_MANAGED, role_label.as_str()]
        .iter()
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect();
    docker.create_network(network, network_labels, internal).await?;
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(LaunchStage::Network, "isolated");
    }
    Ok(())
}
