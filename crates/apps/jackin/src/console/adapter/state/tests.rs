// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `state`.

use super::*;

use crate::console::adapter::state::SettingsState;

use crate::console::services::instances::load_instance_refresh_snapshot;

use crate::console::services::instances::overlay_running_instances;

use jackin_config::{CURRENT_WORKSPACE_VERSION, KeepAwakeConfig, MountConfig, WorkspaceConfig};

use jackin_console::mount_diff::{MountDiff, classify_mount_diffs};

use jackin_core::{Agent, JackinPaths};

use jackin_runtime::instance::{
    DockerResources, InstanceIndex, InstanceManifest, InstanceStatus, NewInstanceManifest,
};

use std::path::PathBuf;

mod support;
use support::*;
mod case_01;
mod case_02;
