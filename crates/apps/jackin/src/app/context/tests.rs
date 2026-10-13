// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `context`.

use super::*;

use crate::workspace;

use jackin_config::find_saved_workspace_for_cwd;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
