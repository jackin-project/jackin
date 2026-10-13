// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Capsule/post-run exec argv builders.

use jackin_core::container_paths;

use super::{EffectiveGrants, NetworkGrant};

/// In-container path of the capsule binary, used for post-run `docker exec`.
pub const CAPSULE_BIN_PATH: &str = container_paths::CAPSULE_BIN;

/// `docker exec --user root <container> <capsule> <subcommand>` argv.
///
/// Root via `exec` needs no setuid, so it composes with `no-new-privileges`.
/// Shared by the post-run privileged capsule steps (firewall, sudo).
pub(crate) fn capsule_root_exec_argv<'a>(
    container_ref: &'a str,
    subcommand: &'a str,
) -> [&'a str; 6] {
    [
        "exec",
        "--user",
        "root",
        container_ref,
        CAPSULE_BIN_PATH,
        subcommand,
    ]
}

/// WP1: the post-run `docker exec` argv that installs the egress allowlist, or
/// `None` when the profile does not enforce one (`open`/`none` install no
/// firewall). Fail-closed at the call site.
pub fn firewall_post_run_argv<'a>(
    grants: &EffectiveGrants,
    container_ref: &'a str,
) -> Option<[&'a str; 6]> {
    (grants.network == NetworkGrant::Allowlist)
        .then(|| capsule_root_exec_argv(container_ref, "firewall-apply"))
}

/// WP-SUDO: the post-run `docker exec` argv that provisions sudo. Only run when
/// the profile grants sudo (`compat`, or an explicit `sudo = true`); the base
/// image bakes no sudoers, so non-sudo profiles have nothing to provision.
pub fn sudo_provision_post_run_argv(container_ref: &str) -> [&str; 6] {
    capsule_root_exec_argv(container_ref, "sudo-provision")
}
