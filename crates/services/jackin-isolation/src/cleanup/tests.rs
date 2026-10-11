// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cleanup authority, positive verification, and partial-failure regressions.

use super::*;

use crate::MountIsolation;

use crate::state::{CleanupStatus, read_records, write_records};

use jackin_test_support::FakeRunner;

use tempfile::TempDir;

mod support;
use support::*;
mod case_01;
mod case_02;
