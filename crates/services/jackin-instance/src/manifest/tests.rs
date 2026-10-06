// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance/manifest`.

use super::*;

use jackin_core::Agent;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
