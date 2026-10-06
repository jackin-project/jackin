// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `validate`.

use super::*;

use crate::manifest::load_role_manifest;

use tempfile::tempdir;

mod case_01;
mod case_02;
mod case_03;
