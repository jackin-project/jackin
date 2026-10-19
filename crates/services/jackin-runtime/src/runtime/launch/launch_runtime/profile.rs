// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::LaunchContext;
use jackin_core::CommandRunner;

pub(super) struct ProfileFacts {
    pub(super) cgroup_version: &'static str,
    pub(super) apparmor_available: bool,
    pub(super) apparmor_layer: &'static str,
}

pub(super) async fn prepare(
    ctx: &LaunchContext<'_, '_>,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<ProfileFacts> {
    let LaunchContext {
        workspace,
        debug,
        profile,
        grants,
        ..
    } = ctx;
    emit_isolation_decision(workspace, grants);

    let cgroup_version = crate::runtime::docker_profile::probe_cgroup_version();
    if let Some(warning) =
        crate::runtime::docker_profile::validate_cgroup_for_profile(*profile, cgroup_version)
            .map_err(|msg| anyhow::anyhow!(msg))?
    {
        jackin_diagnostics::emit_compact_line("warning", warning);
    }
    crate::runtime::docker_profile::validate_dind_grant_for_cgroup(grants.dind, cgroup_version)
        .map_err(|msg| anyhow::anyhow!(msg))?;
    if crate::runtime::docker_profile::dind_privileged(grants)
        && crate::runtime::docker_profile::drops_all_caps(*profile)
    {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "privileged DinD under `{profile}` profile defeats capability and network isolation (partial enforcement); prefer `dind = \"rootless\"`"
            ),
        );
    }
    let (apparmor_available, apparmor_layer) = if *debug {
        match runner
            .capture(
                "docker",
                &["info", "--format", "{{.SecurityOptions}}"],
                None,
            )
            .await
        {
            Ok(info) => crate::runtime::docker_profile::parse_apparmor_from_docker_info(&info),
            Err(err) => {
                let _warning = jackin_telemetry::record_recovered_degradation();
                jackin_diagnostics::emit_debug_line(
                    "launch",
                    &format!("apparmor probe failed: {err:#}"),
                );
                (false, "unknown")
            }
        }
    } else {
        (false, "host")
    };

    Ok(ProfileFacts {
        cgroup_version,
        apparmor_available,
        apparmor_layer,
    })
}

pub(super) fn emit_debug(ctx: &LaunchContext<'_, '_>, facts: &ProfileFacts) {
    let ProfileFacts {
        cgroup_version,
        apparmor_available,
        apparmor_layer,
    } = *facts;
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(ctx.grants);
    let LaunchContext {
        profile,
        profile_source,
        grants,
        ..
    } = ctx;
    if !ctx.debug {
        return;
    }
    {
        let yes_no = |enabled: bool| if enabled { "yes" } else { "no" };
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!("profile_selected profile={profile} source={profile_source}"),
        );
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "cap_drop_all={} cap_add={}",
                yes_no(crate::runtime::docker_profile::drops_all_caps(*profile)),
                if grants.capabilities_add.is_empty() {
                    "-".to_owned()
                } else {
                    grants.capabilities_add.join(",")
                },
            ),
        );
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "no_new_privileges enforced={}",
                yes_no(grants.no_new_privileges),
            ),
        );
        jackin_diagnostics::emit_debug_line("launch", "seccomp profile=docker-default");
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "apparmor available={} profile=docker-default layer={apparmor_layer}",
                yes_no(apparmor_available),
            ),
        );
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "read_only_root enforced={} tmpfs={}",
                yes_no(!grants.system_writes),
                if grants.system_writes {
                    "-".to_owned()
                } else {
                    crate::runtime::docker_profile::tmpfs_paths(*profile).join(",")
                },
            ),
        );
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!("cgroup_version v={cgroup_version}"),
        );
        for (kind, value) in [
            ("memory", grants.memory_bytes.map(|b| b.to_string())),
            ("cpus", grants.cpus.map(|c| c.to_string())),
            ("pids", grants.pids.map(|p| p.to_string())),
        ] {
            if let Some(value) = value {
                jackin_diagnostics::emit_debug_line(
                    "launch",
                    &format!("resource_limit kind={kind} value={value}"),
                );
            }
        }
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!("dind enabled={dind_enabled} mode={}", grants.dind),
        );
        jackin_diagnostics::emit_debug_line("launch", "host_socket_check passed=yes");
        jackin_diagnostics::emit_debug_line(
            "launch",
            &format!(
                "network mode={} enforcement={}",
                crate::runtime::docker_profile::network_grant_label(grants.network),
                crate::runtime::docker_profile::network_enforcement_label(grants),
            ),
        );
    }
}

fn emit_isolation_decision(
    workspace: &crate::isolation::materialize::MaterializedWorkspace,
    grants: &crate::runtime::docker_profile::EffectiveGrants,
) {
    use jackin_telemetry::schema::enums::{DindMode, NetworkMode, WorkspaceIsolationMode};

    let workspace = match workspace
        .mounts
        .first()
        .map_or(jackin_core::MountIsolation::Shared, |mount| mount.isolation)
    {
        jackin_core::MountIsolation::Shared => WorkspaceIsolationMode::Shared,
        jackin_core::MountIsolation::Worktree => WorkspaceIsolationMode::Worktree,
        jackin_core::MountIsolation::Clone => WorkspaceIsolationMode::Clone,
    };
    let network = match grants.network {
        crate::runtime::docker_profile::NetworkGrant::None => NetworkMode::None,
        crate::runtime::docker_profile::NetworkGrant::Allowlist => NetworkMode::Allowlist,
        crate::runtime::docker_profile::NetworkGrant::Open => NetworkMode::Open,
    };
    let dind = match grants.dind {
        crate::runtime::docker_profile::DindGrant::None => DindMode::None,
        crate::runtime::docker_profile::DindGrant::Rootless => DindMode::Rootless,
        crate::runtime::docker_profile::DindGrant::Privileged => DindMode::Privileged,
    };
    jackin_diagnostics::operation::isolation_decision(workspace, network, dind);
}

pub(super) struct ContainerSecurity {
    pub(super) cap_add: Vec<String>,
    pub(super) cap_drop: Vec<String>,
    pub(super) tmpfs: Vec<String>,
    pub(super) security_opt: Vec<String>,
}

pub(super) fn container_security(ctx: &LaunchContext<'_, '_>) -> ContainerSecurity {
    let LaunchContext {
        profile, grants, ..
    } = ctx;
    let cap_drop = if crate::runtime::docker_profile::drops_all_caps(*profile) {
        vec!["ALL".to_owned()]
    } else {
        Vec::new()
    };
    let mut cap_add = if crate::runtime::docker_profile::drops_all_caps(*profile) {
        crate::runtime::docker_profile::MINIMUM_CAPABILITIES
            .iter()
            .map(ToString::to_string)
            .collect()
    } else {
        Vec::new()
    };
    cap_add.extend(
        grants
            .capabilities_add
            .iter()
            .map(|cap| cap.trim_start_matches("CAP_").to_ascii_uppercase()),
    );
    let tmpfs = if grants.system_writes {
        Vec::new()
    } else {
        let mut mounts = crate::runtime::docker_profile::tmpfs_paths(*profile)
            .into_iter()
            .map(|path| format!("{path}:rw,nosuid,nodev"))
            .collect::<Vec<_>>();
        if grants.sudo {
            mounts.push(jackin_core::container_paths::SUDOERS_TMPFS_SPEC.to_owned());
        }
        mounts
    };
    let security_opt = if grants.no_new_privileges {
        vec!["no-new-privileges".to_owned()]
    } else {
        Vec::new()
    };
    ContainerSecurity {
        cap_add,
        cap_drop,
        tmpfs,
        security_opt,
    }
}
