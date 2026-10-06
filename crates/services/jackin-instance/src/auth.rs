// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent credential provisioning: copies or wipes per-agent auth files in the
//! role-state directory before container launch.
//!
//! Implements `RoleState` methods for each supported agent (`Claude`, `Codex`,
//! `Amp`, `Kimi`, `OpenCode`). Each provisioner applies the `AuthForwardMode`
//! policy (`Sync`, `ApiKey`, `OAuthToken`, `Ignore`) to decide whether to
//! copy the host credential file, leave it, or wipe it.
//!
//! Invariant: any symlink at an auth-file path is rejected before branching
//! on mode — a compromised role cannot redirect a provisioning write through
//! a symlink placed between launches.

mod amp;
mod auth_directory;
mod capture;
mod capture_unixless;
mod claude;
mod codex;
mod github;
mod hermes;
mod kimi;
mod mounts;
mod omp;
mod opencode;
mod paths;
mod permissions;
mod single_file;
mod single_file_agents;
mod snapshot;
mod validation;

pub use auth_directory::AuthMountLease;
pub use snapshot::validate_sync_source_dir;

#[cfg(unix)]
pub(crate) use amp::lock_amp_source_dir;
#[cfg(not(unix))]
pub(crate) use amp::{amp_credentials_dir, require_credential_file};
#[cfg(unix)]
pub(crate) use capture::capture_locked_source;
pub(crate) use capture::create_source_snapshot_dir;
#[cfg(not(unix))]
pub(crate) use capture_unixless::{
    capture_unixless_single_file_source, capture_unixless_source, copy_unixless_source_tree,
    copy_unixless_source_tree_inner,
};
pub(crate) use capture_unixless::{
    read_bounded_local_file, snapshot_content_revision, write_snapshot_bytes,
};
#[cfg(unix)]
pub(crate) use claude::locked_claude_credentials;
#[cfg(target_os = "macos")]
pub(crate) use claude::read_claude_keychain;
#[cfg(not(unix))]
pub(crate) use claude::read_host_credentials_from_claude_config_dir;

#[cfg(test)]
pub(crate) use claude::copy_host_claude_json;
#[cfg(test)]
pub(crate) use github::parse_gh_hosts_yml;
pub(crate) use github::{host_home_is_real, wipe_file_if_present};
#[cfg(unix)]
pub(crate) use kimi::validate_kimi_locked_source;
#[cfg(not(unix))]
pub(crate) use kimi::validate_kimi_source_dir_unixless;
#[cfg(test)]
pub(crate) use snapshot::validate_sync_source_dir_for_provider;

pub(crate) use mounts::{admit_auth_mounts, mount_directory_present, mount_file_present};
#[cfg(not(unix))]
pub(crate) use omp::capture_omp_database_snapshot_from_paths;
#[cfg(unix)]
pub(crate) use omp::{capture_omp_database_snapshot, validate_omp_source_selection};
pub(crate) use omp::{private_file_exists, read_source_bytes, read_source_text};
pub(crate) use paths::{
    create_private_file_if_absent, reject_auth_path, write_private_bytes, write_private_file,
};
#[cfg(test)]
pub(crate) use permissions::inject_permission_repair_failure;
pub(crate) use permissions::{
    PermissionRepairFailure, maybe_inject_permission_repair_failure, repair_permissions,
};
pub(crate) use single_file::{
    provision_single_blob_credential, provision_single_blob_credential_from_content,
    provision_single_file_credential, provision_single_file_credential_with_content,
    wipe_agent_file_state, wipe_kimi_state,
};
pub(crate) use snapshot::{
    AuthSourceDescriptor, MAX_AUTH_SOURCE_FILE_BYTES, MAX_AUTH_SOURCE_TREE_BYTES,
    MAX_AUTH_SOURCE_TREE_ENTRIES, SelectedAuthSourceSnapshot, SelectedSourceDirectory,
    capture_selected_source, claude_source_missing_error,
};
#[cfg(unix)]
pub(crate) use validation::validate_locked_sync_source_dir;
#[cfg(not(unix))]
pub(crate) use validation::validate_opencode_source_dir;
pub(crate) use validation::{select_opencode_auth_entry, validate_store_source_dir};

#[cfg(test)]
mod tests;
