// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::container_paths;

use super::{EffectiveGrants, NetworkGrant};

pub const CAPSULE_BIN_PATH: &str = container_paths::CAPSULE_BIN;

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

pub fn firewall_post_run_argv<'a>(
    grants: &EffectiveGrants,
    container_ref: &'a str,
) -> Option<[&'a str; 6]> {
    (grants.network == NetworkGrant::Allowlist)
        .then(|| capsule_root_exec_argv(container_ref, "firewall-apply"))
}

pub fn sudo_provision_post_run_argv(container_ref: &str) -> [&str; 6] {
    capsule_root_exec_argv(container_ref, "sudo-provision")
}
