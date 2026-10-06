// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `sidebar_layout`.

use super::*;

use jackin_config::{
    AppConfig, EnvValue, GlobalMountRow, MountConfig, MountIsolation, RoleSource, WorkspaceConfig,
};

use crate::tui::screens::workspaces::model::ManagerListRow;

mod case_01;
