// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub use jackin_core::{
    DindGrant, DockerGrants, DockerSecurityProfile, NetworkGrant, ParseProfileError,
};

mod allowlist;
mod capabilities;
mod drops;
mod effective;
mod exec;
mod flags;
mod grants;
mod grants_validate;
mod labels;
mod probe;
mod profile;
mod sizes;
mod tmpfs;
pub mod workflow_grant;

pub use allowlist::{allowlist_hosts, default_allowed_hosts_for_agent, github_allowlist_hosts};
pub use capabilities::{MINIMUM_CAPABILITIES, VALID_CAPABILITIES};
pub use drops::{
    DIND_PRIVILEGED_IMAGE, DIND_ROOTLESS_IMAGE, dind_enabled, dind_image_and_privileged,
    dind_privileged, drops_all_caps, network_disabled, validate_dind_grant_for_cgroup,
};
pub use effective::{EffectiveGrants, apply_grants, fold_role_grants, profile_base_grants};
pub use exec::{CAPSULE_BIN_PATH, firewall_post_run_argv, sudo_provision_post_run_argv};
pub use flags::{capability_flags, resource_flags};
pub use grants::validate_grants;
pub use grants_validate::GrantValidationError;
pub use labels::{format_session_contract, network_enforcement_label, network_grant_label};
pub use probe::{
    parse_apparmor_from_docker_info, probe_cgroup_version, role_network_internal,
    validate_cgroup_for_profile,
};
pub use profile::{
    ProfileSource, profile_meets_floor, resolve_effective_grants, resolve_profile,
    validate_effective_grants,
};
pub use sizes::parse_memory_bytes;
pub use tmpfs::{readonly_home_env, readonly_root_flags, tmpfs_paths};
pub use workflow_grant::{
    DiskPolicy, GrantMismatch, NetworkEnforcementLevel, WorkflowGrant, WorkflowGrantError,
    admit_disk_policy, admit_network_enforcement, apply_workflow_grant, compare_grant_vs_actual,
};

pub(crate) use flags::{TMPFS_PATHS_HARDENED_EXTRA, TMPFS_PATHS_MINIMAL};
pub(crate) use grants::normalize_cap;

pub(crate) use sizes::{GB, format_bytes, parse_size_field};
