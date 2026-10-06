// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `config`.

use super::*;

use crate::{
    GithubAuthMode, MountConfig, MountEntry, resolve_github_mode, validate_workspace_config,
};

use jackin_core::JackinPaths;

use jackin_core::WorkspaceName;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
