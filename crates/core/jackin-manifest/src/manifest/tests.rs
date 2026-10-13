// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `manifest`.

use super::*;

use crate::repo_contract::MANIFEST_FILENAME;

use tempfile::tempdir;

mod case_01;
mod case_02;
mod case_03;
