// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `repo`.

use super::*;

use jackin_core::JackinPaths;

use jackin_core::RoleSelector;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
