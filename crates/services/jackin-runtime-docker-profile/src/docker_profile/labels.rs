// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Network labels and session contract formatting.

use super::{
    DockerSecurityProfile, EffectiveGrants, MINIMUM_CAPABILITIES, NetworkGrant, dind_enabled,
    drops_all_caps, format_bytes, tmpfs_paths,
};

/// The `JACKIN_NETWORK_MODE` / contract label for a network tier. Delegates to
/// [`NetworkGrant::as_str`] so the label tracks the serde vocabulary.
pub fn network_grant_label(network: NetworkGrant) -> &'static str {
    network.as_str()
}

/// Returns the network enforcement quality label.
///
/// Used for session contract output and `JACKIN_NETWORK_ENFORCEMENT`. Shared
/// between `format_session_contract` and `launch_role_runtime` so both surfaces
/// stay in sync.
pub fn network_enforcement_label(grants: &EffectiveGrants) -> &'static str {
    if !matches!(grants.network, NetworkGrant::Allowlist) {
        return "n/a";
    }
    if grants.sudo || grants.user == "root" {
        "partial (sudo grants iptables access)"
    } else if dind_enabled(grants) {
        "partial (DinD inner containers bypass host iptables)"
    } else {
        "full"
    }
}

/// Format a human-readable session contract table for the active grants.
///
/// Surfaced to the operator in `--debug` mode as a factual summary of what the
/// container can do; it is operator output, not exported telemetry.
// Eight contract dimensions are one flat argument list by design; bundling them
// into a struct would just move the same fields without aiding any caller.
#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn format_session_contract(
    profile: DockerSecurityProfile,
    profile_source: &str,
    grants: &EffectiveGrants,
    apparmor_available: bool,
    apparmor_layer: &str,
    cgroup_version: &str,
    agent_auth_mode: &str,
    gh_auth_forwarded: bool,
) -> String {
    let extra_caps = if grants.capabilities_add.is_empty() {
        String::new()
    } else {
        format!(" + {}", grants.capabilities_add.join(","))
    };
    let caps_line = if drops_all_caps(profile) {
        format!("drop-all + {}{extra_caps}", MINIMUM_CAPABILITIES.join(","))
    } else {
        format!("docker-default (14 caps){extra_caps}")
    };
    let network_mode = match grants.network {
        NetworkGrant::None => "none (--network none)".to_owned(),
        // `allowed_hosts` is only the operator/role-configured set; the launch
        // path also injects the agent's API endpoint(s) and (when forwarded)
        // GitHub into JACKIN_ALLOWED_HOSTS. Report the configured count honestly
        // rather than guessing the injected total with a fixed `+1`.
        NetworkGrant::Allowlist => format!(
            "allowlist ({} configured hosts + agent/GitHub endpoints)",
            grants.allowed_hosts.len()
        ),
        NetworkGrant::Open => "open".to_owned(),
    };
    let network_enforcement = network_enforcement_label(grants);
    let memory_line = grants
        .memory_bytes
        .map_or_else(|| "unlimited".to_owned(), format_bytes);
    let cpus_line = grants
        .cpus
        .map_or_else(|| "unlimited".to_owned(), |c| c.to_string());
    let pids_line = grants
        .pids
        .map_or_else(|| "unlimited".to_owned(), |p| p.to_string());
    let gh_line = if gh_auth_forwarded {
        "forwarded"
    } else {
        "not forwarded"
    };
    let residual_base = "shared host kernel; writable workspace mounts can still be changed";
    let residual = if dind_enabled(grants) {
        format!("{residual_base}; DinD sidecar has kernel access")
    } else if grants.system_writes {
        format!("{residual_base}; writable container root")
    } else {
        residual_base.to_owned()
    };

    format!(
        "Docker profile: {} (source: {})\n\
         Role container:\n  \
           seccomp: docker-default\n  \
           apparmor: {} (layer: {})\n  \
           no-new-privileges: {}\n  \
           capabilities: {}\n  \
           root filesystem: {}\n  \
           writable tmpfs: {}\n\
         DinD:\n  status: {}\n\
         Network:\n  mode: {}\n  enforcement: {}\n\
         cgroup: {}\n\
         Resources:\n  memory: {}\n  cpus: {}\n  pids: {}\n\
         Credentials:\n  agent: {}\n  GitHub CLI: {}\n\
         Residual risk:\n  {}",
        profile,
        profile_source,
        if apparmor_available {
            "docker-default"
        } else {
            "unavailable"
        },
        apparmor_layer,
        if grants.no_new_privileges {
            "enforced"
        } else {
            "not applied"
        },
        caps_line,
        if grants.system_writes {
            "writable"
        } else {
            "read-only"
        },
        if grants.system_writes {
            "none (writable root)".to_owned()
        } else {
            tmpfs_paths(profile).join(",")
        },
        grants.dind, // Display impl emits "none"/"rootless"/"privileged"
        network_mode,
        network_enforcement,
        cgroup_version,
        memory_line,
        cpus_line,
        pids_line,
        agent_auth_mode,
        gh_line,
        residual,
    )
}
