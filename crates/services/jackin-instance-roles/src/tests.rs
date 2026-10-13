// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance` (role-state orchestration + auth provisioning).

use super::*;

use jackin_config::{AuthForwardMode, GithubAuthMode};

use jackin_core::JackinPaths;

use jackin_manifest::{RoleManifest, load_role_manifest};

use std::path::{Path, PathBuf};

use tempfile::tempdir;

#[cfg(unix)]
use jackin_instance_credentials::auth_directory::{
    FailurePoint, TreeEntryKind, classify_tree_entry_for_removal, inject_failure,
    lock_source_dir_for_test, set_hermes_snapshot_hook, set_source_open_hook,
    target_lock_key_for_test,
};

use jackin_instance_agents::{
    capture_selected_source, validate_sync_source_dir, validate_sync_source_dir_for_provider,
};
use jackin_instance_credentials::{
    PermissionRepairFailure, inject_permission_repair_failure, repair_permissions,
};

use jackin_config::{AiProvider, ProfileSelector};

use jackin_core::Agent;

use jackin_instance_agents::{copy_host_claude_json, parse_gh_hosts_yml};
use jackin_instance_agents::{
    provision_amp_auth, provision_amp_auth_from_source_dir, provision_claude_auth_from_config_dir,
    provision_codex_auth, provision_codex_auth_from_source_dir, provision_github_auth,
    provision_grok_auth_from_source_dir, provision_hermes_auth_from_source_dir,
    provision_kimi_auth, provision_kimi_auth_from_source_dir, provision_omp_auth_from_source_dir,
    provision_opencode_auth, provision_opencode_auth_from_source_dir,
};
use jackin_instance_credentials::MAX_AUTH_SOURCE_FILE_BYTES;

use jackin_instance_credentials::{
    GithubAuthContext, GithubProvisionKind, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason,
};

mod selected_source;

mod support;
use support::*;
mod auth_support;
use auth_support::*;
mod auth_case_01;
mod auth_case_02;
mod auth_case_03;
mod auth_case_04;
mod auth_case_05;
mod auth_case_06;
mod auth_case_07;
mod auth_case_08;
mod auth_case_09;
mod auth_case_10;
mod auth_case_11;
mod auth_case_12;
mod case_01;
mod case_02;
mod case_03;
