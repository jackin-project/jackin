// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_config::AuthForwardMode;

use jackin_config::{AiProvider, ProfileSelector};

use jackin_core::Agent;

use jackin_core::JackinPaths;

use jackin_manifest::{RoleManifest, load_role_manifest};

use std::path::{Path, PathBuf};

use tempfile::tempdir;

#[cfg(unix)]
use jackin_instance_credentials::auth_directory::{TreeEntryKind, classify_tree_entry_for_removal};

use jackin_instance_agents::{
    provision_kimi_auth, provision_kimi_auth_from_source_dir, provision_omp_auth_from_source_dir,
};

use jackin_instance_credentials::GithubAuthContext;

mod support;
use support::*;
mod auth_support;
use auth_support::*;
mod auth_case_11;
mod auth_case_12;
mod auth_case_14;
mod role_state;
