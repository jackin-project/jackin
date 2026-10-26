// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    LaunchContext, environment::append_arguments, profile::ProfileFacts, setup::CapsuleSetup,
};
use anyhow::Context;
use jackin_core::CommandRunner;
#[cfg(test)]
use jackin_core::RunOptions;
use jackin_docker::docker_client::DockerApi;
use jackin_runtime_launch_capsule_setup::capsule_setup::HostEnvTransport;
use jackin_runtime_usage_relay::usage_relay::PreparedUsageRelay;

pub(super) struct PreparedDockerLaunch {
    pub(super) spec: jackin_core::ContainerSpec,
    #[cfg(test)]
    pub(super) observation_args: Vec<String>,
    pub(super) host_env_transport: HostEnvTransport,
    pub(super) relay: PreparedUsageRelay,
}

struct LaunchMounts {
    certs: String,
    github: Option<String>,
    agent: Vec<String>,
    workspace: Vec<String>,
}

pub(super) async fn prepare(
    ctx: &LaunchContext<'_, '_>,
    facts: &ProfileFacts,
) -> anyhow::Result<PreparedDockerLaunch> {
    let certs_volume = ctx
        .ownership
        .resources
        .certs_volume
        .clone()
        .unwrap_or_else(|| crate::instance::naming::dind_certs_volume(ctx.container_name));
    let instance_id = super::environment::instance_id(ctx);
    let agent_specific_mounts = super::super::agent_mounts(ctx.state)?;
    let gh_config_mount = super::super::github_config_mount(ctx.state)?;
    let certs_agent_mount = format!(
        "{certs_volume}:{}:ro",
        jackin_core::container_paths::DIND_CERTS_CLIENT_DIR
    );
    let mut run_args = base_arguments(ctx);
    super::profile::emit_debug(ctx, facts);
    run_args.extend(super::environment::arguments(ctx, facts, &instance_id));
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(ctx.grants);
    if dind_enabled {
        append_arguments(&mut run_args, &["-v", &certs_agent_mount]);
    }
    if let Some(gh_config_mount) = gh_config_mount.as_deref() {
        append_arguments(&mut run_args, &["-v", gh_config_mount]);
    }
    for mount in &agent_specific_mounts {
        run_args.push("-v".to_owned());
        run_args.push(mount.clone());
    }

    let mount_strings = super::super::build_workspace_mount_strings(ctx.workspace);
    for ms in &mount_strings {
        run_args.push("-v".to_owned());
        run_args.push(ms.clone());
    }
    append_arguments(
        &mut run_args,
        &["--label", &format!("jackin.image={}", ctx.image)],
    );
    let capsule = super::setup::prepare(ctx).await?;
    append_arguments(&mut run_args, &["-v", &capsule.socket_mount]);
    for mount in &capsule.extrausers_mounts {
        append_arguments(&mut run_args, &["-v", mount]);
    }
    let mut borrowed: Vec<&str> = run_args.iter().map(String::as_str).collect();
    let host_env_transport = super::super::prepare_host_env_transport(
        &ctx.paths.jackin_home,
        ctx.container_name,
        &mut borrowed,
    )
    .context("creating private host runtime environment")?;
    host_env_transport.append_arguments(&mut borrowed);
    if ctx.debug {
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "prepared private host socket dir {} and redacted Capsule config for bind-mount at /jackin/run",
                capsule.socket_dir.display()
            ),
        );
    }
    borrowed.push(ctx.image);
    borrowed.push(super::initial_daemon_argv(ctx.agent, &capsule.config));
    let mounts = LaunchMounts {
        certs: certs_agent_mount,
        github: gh_config_mount,
        agent: agent_specific_mounts,
        workspace: mount_strings,
    };
    let spec = container_spec(ctx, &capsule, &mounts, &borrowed, &host_env_transport)?;
    #[cfg(test)]
    let observation_args = borrowed.iter().map(|value| (*value).to_owned()).collect();
    Ok(PreparedDockerLaunch {
        spec,
        #[cfg(test)]
        observation_args,
        host_env_transport,
        relay: capsule.relay,
    })
}

fn base_arguments(ctx: &LaunchContext<'_, '_>) -> Vec<String> {
    let network_disabled = crate::runtime::docker_profile::network_disabled(ctx.grants);
    let LaunchContext {
        container_name,
        network,
        workspace,
        profile,
        grants,
        ..
    } = ctx;
    let class_label = format!("jackin.class={}", ctx.selector.key());
    let display_label = format!("jackin.display.name={}", ctx.agent_display_name);
    let mut run_args = Vec::new();
    append_arguments(
        &mut run_args,
        &[
            "run",
            "-d",
            "--name",
            container_name,
            "--hostname",
            container_name,
            "--label",
            crate::runtime::naming::LABEL_MANAGED,
            "--label",
            crate::runtime::naming::LABEL_KIND_ROLE,
            "--label",
            &class_label,
            "--label",
            &display_label,
            "--workdir",
            &workspace.workdir,
        ],
    );

    let network = if network_disabled { "none" } else { network };
    append_arguments(&mut run_args, &["--network", network]);

    if workspace.keep_awake_enabled {
        append_arguments(
            &mut run_args,
            &["--label", crate::runtime::naming::LABEL_KEEP_AWAKE],
        );
    }

    let capability_flags =
        crate::runtime::docker_profile::capability_flags(*profile, &grants.capabilities_add);
    run_args.extend(capability_flags);
    let readonly_flags = crate::runtime::docker_profile::readonly_root_flags(*profile, grants);
    run_args.extend(readonly_flags);
    if grants.no_new_privileges {
        append_arguments(&mut run_args, &["--security-opt", "no-new-privileges"]);
    }
    let resource_flags = crate::runtime::docker_profile::resource_flags(grants);
    run_args.extend(resource_flags);
    run_args
}

fn labels(
    ctx: &LaunchContext<'_, '_>,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let LaunchContext { workspace, .. } = ctx;
    let class_label = format!("jackin.class={}", ctx.selector.key());
    let display_label = format!("jackin.display.name={}", ctx.agent_display_name);
    let image_label = format!("jackin.image={}", ctx.image);
    let mut container_labels = std::collections::HashMap::new();
    for label in [
        crate::runtime::naming::LABEL_MANAGED,
        crate::runtime::naming::LABEL_KIND_ROLE,
        &class_label,
        &display_label,
        &image_label,
    ] {
        let (key, value) = label
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid Docker label {label:?}"))?;
        container_labels.insert(key.to_owned(), value.to_owned());
    }
    if workspace.keep_awake_enabled {
        let (key, value) = crate::runtime::naming::LABEL_KEEP_AWAKE
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid Docker keep-awake label"))?;
        container_labels.insert(key.to_owned(), value.to_owned());
    }

    Ok(container_labels)
}

fn container_spec(
    ctx: &LaunchContext<'_, '_>,
    capsule: &CapsuleSetup,
    mounts: &LaunchMounts,
    run_args: &[&str],
    transport: &HostEnvTransport,
) -> anyhow::Result<jackin_core::ContainerSpec> {
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(ctx.grants);
    let LaunchContext {
        container_name,
        image,
        workspace,
        agent,
        grants,
        ..
    } = ctx;
    let mut container_env = transport.environment().to_vec();
    container_env.extend(
        run_args
            .windows(2)
            .filter(|pair| pair[0] == "-e")
            .map(|pair| pair[1].to_owned()),
    );
    let extra_hosts = run_args
        .windows(2)
        .filter(|pair| pair[0] == "--add-host")
        .map(|pair| pair[1].to_owned())
        .collect();
    let super::profile::ContainerSecurity {
        cap_add,
        cap_drop,
        tmpfs,
        security_opt,
    } = super::profile::container_security(ctx);
    let mut container_binds = Vec::new();
    if dind_enabled {
        container_binds.push(mounts.certs.clone());
    }
    if let Some(mount) = mounts.github.as_ref() {
        container_binds.push(mount.clone());
    }
    container_binds.extend(mounts.agent.iter().cloned());
    container_binds.extend(mounts.workspace.iter().cloned());
    container_binds.push(capsule.socket_mount.clone());
    container_binds.extend(capsule.extrausers_mounts.iter().cloned());
    let container_spec = jackin_core::ContainerSpec {
        image: (*image).to_owned(),
        hostname: Some((*container_name).to_owned()),
        env: container_env,
        labels: labels(ctx)?,
        network: if crate::runtime::docker_profile::network_disabled(grants) {
            "none"
        } else {
            ctx.network
        }
        .to_owned(),
        binds: container_binds,
        entrypoint: None,
        privileged: false,
        workdir: Some(workspace.workdir.clone()),
        user: Some("0:0".to_owned()),
        command: Some(vec![
            super::initial_daemon_argv(*agent, &capsule.config).to_owned(),
        ]),
        cap_add,
        cap_drop,
        readonly_rootfs: !grants.system_writes,
        security_opt,
        tmpfs,
        extra_hosts,
        memory_bytes: grants.memory_bytes,
        memory_reservation_bytes: grants.memory_reservation_bytes,
        nano_cpus: grants.cpus.map(|cpus| (cpus * 1_000_000_000.0) as i64),
        pids_limit: grants.pids,
        nofile: grants.nofile,
    };
    Ok(container_spec)
}

pub(super) async fn start(
    ctx: &LaunchContext<'_, '_>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    prepared: PreparedDockerLaunch,
) -> anyhow::Result<(jackin_core::ContainerHandle, PreparedUsageRelay)> {
    #[cfg(test)]
    let docker_run_opts = RunOptions {
        quiet: !ctx.debug,
        ..RunOptions::default()
    };
    let LaunchContext {
        container_name,
        ownership,
        role_handle_slot,
        state,
        paths,
        account_revision,
        entry_claim,
        ..
    } = ctx;
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "docker_run_role",
        Some(container_name),
    );
    account_revision.ensure_current(paths)?;
    let run_role_result = {
        let created =
            super::with_admitted_final_docker_spec(paths, state, prepared.spec, |spec| {
                docker.create_container(container_name, spec)
            })?
            .await;
        match created {
            Ok(container) => {
                *role_handle_slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container.clone());
                ownership.persist(&container)?;
                async {
                    docker.start_container_by_id(&container).await?;
                    if let Some(claim) = entry_claim {
                        claim.activate().await?;
                    }
                    Ok(container)
                }
                .await
            }
            Err(error) => Err(error),
        }
    };

    #[cfg(test)]
    let run_role_observation = runner
        .observe(
            "docker",
            &prepared
                .observation_args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            None,
            &docker_run_opts,
        )
        .await;
    #[cfg(not(test))]
    let _ = runner;
    #[cfg(not(test))]
    let run_role_observation: anyhow::Result<()> = Ok(());
    drop(prepared.host_env_transport);
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "docker_run_role",
        if run_role_result.is_ok() {
            Some("started")
        } else {
            Some("error")
        },
    );
    if run_role_result.is_err() {
        let _error = jackin_telemetry::record_error(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        );
        jackin_diagnostics::emit_operator_notice("role container start failed");
    }
    if let Err(error) = run_role_result {
        return Err(error);
    }
    run_role_observation?;
    let role_container = run_role_result?;
    super::super::ensure_current_or_remove_stale_container(
        account_revision,
        paths,
        &role_container,
        docker,
    )
    .await?;

    Ok((role_container, prepared.relay))
}
