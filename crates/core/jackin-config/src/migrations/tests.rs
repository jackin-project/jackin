// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `migrations`.

use super::*;

use crate::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
