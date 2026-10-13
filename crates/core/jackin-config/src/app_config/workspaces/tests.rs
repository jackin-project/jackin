// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `workspaces`.

use super::*;

use crate::MountConfig;

use crate::{CURRENT_WORKSPACE_VERSION, KeepAwakeConfig};

use jackin_core::WorkspaceName;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
