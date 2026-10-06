// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance/auth` — tests.

#[cfg(unix)]
use super::auth_directory::{
    FailurePoint, TreeEntryKind, classify_tree_entry_for_removal, inject_failure,
    lock_source_dir_for_test, set_hermes_snapshot_hook, set_source_open_hook,
    target_lock_key_for_test,
};

use super::{
    PermissionRepairFailure, capture_selected_source, inject_permission_repair_failure,
    repair_permissions, validate_sync_source_dir, validate_sync_source_dir_for_provider,
};

use crate::{AuthProvisionOutcome, PrepareResolvers, RoleState};

use jackin_config::{AiProvider, AuthForwardMode, GithubAuthMode, ProfileSelector};

use jackin_core::{Agent, JackinPaths};

use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::{MAX_AUTH_SOURCE_FILE_BYTES, copy_host_claude_json, parse_gh_hosts_yml};

use crate::{
    GithubAuthContext, GithubProvisionKind, GithubProvisionOutcome, GithubTokenSource,
    HostMissingReason,
};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
