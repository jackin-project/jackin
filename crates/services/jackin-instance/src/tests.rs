// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance`.

use super::*;

use jackin_core::JackinPaths;

use jackin_manifest::load_role_manifest;

use std::path::PathBuf;

use tempfile::tempdir;

mod selected_source;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
