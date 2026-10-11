// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `finalize`.

use super::*;

use jackin_config::DirtyExitPolicy;

use tempfile::TempDir;

use jackin_test_support::FakeRunner;

use crate::MountIsolation;

use crate::state::write_records;

use std::collections::VecDeque;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
