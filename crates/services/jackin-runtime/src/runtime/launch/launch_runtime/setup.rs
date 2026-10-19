// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::LaunchContext;
use anyhow::Context;

pub(super) struct CapsuleSetup {
    pub(super) config: jackin_protocol::CapsuleConfig,
    pub(super) relay: jackin_runtime_usage_relay::usage_relay::PreparedUsageRelay,
    pub(super) socket_dir: std::path::PathBuf,
    pub(super) socket_mount: String,
    pub(super) extrausers_mounts: Vec<String>,
}

pub(super) async fn prepare(ctx: &LaunchContext<'_, '_>) -> anyhow::Result<CapsuleSetup> {
    let LaunchContext {
        container_name,
        state,
        capsule_config,
        resolved_env,
        credential_scope,
        paths,
        sibling_auth_prewarm,
        ..
    } = ctx;
    let mut capsule_config = (*capsule_config).clone();
    let socket_dir = paths.jackin_home.join("sockets").join(*container_name);
    let prepared_usage_relay =
        crate::usage_relay::prepare_for_stdio_tunnel(crate::usage_relay::UsageRelayLaunch {
            paths,
            workspace_name: (!sibling_auth_prewarm.workspace_name.is_empty())
                .then_some(sibling_auth_prewarm.workspace_name),
            role_key: sibling_auth_prewarm.role_key,
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
    let capsule_config_contents = super::super::capsule_config_contents(&capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    let extrausers_passwd = paths.jackin_home.join("extrausers").join("passwd");
    let extrausers_group = paths.jackin_home.join("extrausers").join("group");
    let extrausers_entries = extrausers_entries(&capsule_config)?;
    write_socket_files(
        ctx,
        &socket_dir,
        &capsule_config_contents,
        &extrausers_passwd,
        &extrausers_group,
        extrausers_entries.clone(),
    )
    .await?;
    if !ctx.capsule_config.exec_bindings.is_empty() {
        drop(crate::exec_host::start_for_container(
            &ctx.paths.jackin_home,
            ctx.container_name,
            &ctx.capsule_config.exec_bindings,
        ));
    }
    let socket_mount = crate::usage_relay::docker_runtime_mount(&socket_dir)?;
    let extrausers_mounts = if extrausers_entries.is_some() {
        let passwd_mount = extrausers_passwd
            .to_str()
            .map(|p| format!("{p}:/var/lib/extrausers/passwd:ro"));
        let group_mount = extrausers_group
            .to_str()
            .map(|p| format!("{p}:/var/lib/extrausers/group:ro"));
        passwd_mount.into_iter().chain(group_mount).collect()
    } else {
        Vec::new()
    };
    Ok(CapsuleSetup {
        config: capsule_config,
        relay: prepared_usage_relay,
        socket_dir,
        socket_mount,
        extrausers_mounts,
    })
}

async fn write_socket_files(
    ctx: &LaunchContext<'_, '_>,
    socket_dir: &std::path::Path,
    capsule_config_contents: &str,
    extrausers_passwd: &std::path::Path,
    extrausers_group: &std::path::Path,
    extrausers_entries: Option<(String, String)>,
) -> anyhow::Result<()> {
    let LaunchContext { container_name, .. } = ctx;
    let extrausers_tmp = extrausers_passwd.with_file_name(format!("passwd.{container_name}.tmp"));
    let extrausers_group_tmp =
        extrausers_group.with_file_name(format!("group.{container_name}.tmp"));
    let socket_dir_for_mkdir = socket_dir.to_owned();
    let capsule_config_contents_for_write = capsule_config_contents.to_owned();
    let extrausers_passwd_for_write = extrausers_passwd.to_owned();
    let extrausers_group_for_write = extrausers_group.to_owned();
    let extrausers_entries_for_write = extrausers_entries;
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "prepare_socket_dir",
        Some(container_name),
    );
    let prepare_socket_dir_result =
        jackin_telemetry::spawn::joined_blocking(move || -> std::io::Result<()> {
            super::super::prepare_socket_dir(
                &socket_dir_for_mkdir,
                &capsule_config_contents_for_write,
            )?;
            if let Some((passwd_line, group_line)) = extrausers_entries_for_write {
                if let Some(parent) = extrausers_passwd_for_write.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                super::super::write_if_changed_atomic(
                    &extrausers_passwd_for_write,
                    &extrausers_tmp,
                    passwd_line.as_bytes(),
                )?;
                super::super::write_if_changed_atomic(
                    &extrausers_group_for_write,
                    &extrausers_group_tmp,
                    group_line.as_bytes(),
                )?;
            }
            Ok(())
        })
        .await
        .context("socket dir mkdir worker join")
        .and_then(|result| {
            result.with_context(|| {
                format!(
                    "creating host-side socket dir {} for container {container_name}",
                    socket_dir.display(),
                )
            })
        });
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "prepare_socket_dir",
        if prepare_socket_dir_result.is_ok() {
            Some("prepared")
        } else {
            Some("error")
        },
    );
    prepare_socket_dir_result
}

fn extrausers_entries(
    capsule_config: &jackin_protocol::CapsuleConfig,
) -> anyhow::Result<Option<(String, String)>> {
    let entries = if capsule_config.instance_identities.is_empty()
        && capsule_config.shell_identity.is_none()
    {
        None
    } else {
        use std::fmt::Write as _;
        let mut passwd = String::new();
        let mut group = String::new();
        for (index, identity) in capsule_config.instance_identities.values().enumerate() {
            writeln!(
                passwd,
                "jackin-slot-{index}:x:{}:{}:jackin slot {index}:/home/agent:/bin/zsh",
                identity.uid, identity.gid
            )?;
            writeln!(group, "jackin-slot-{index}:x:{}:", identity.gid)?;
        }
        if let Some(identity) = capsule_config.shell_identity {
            writeln!(
                passwd,
                "jackin-shell:x:{}:{}:jackin shell:/home/agent:/bin/zsh",
                identity.uid, identity.gid
            )?;
            writeln!(group, "jackin-shell:x:{}:", identity.gid)?;
        }
        Some((passwd, group))
    };
    Ok(entries)
}
