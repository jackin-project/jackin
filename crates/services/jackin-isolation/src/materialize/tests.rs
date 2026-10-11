// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `materialize`.

use super::*;

use jackin_core::{WorkspaceLabel, WorkspaceName};

use std::path::PathBuf;

use jackin_test_support::FakeRunner;

use std::collections::VecDeque;

use jackin_config::MountConfig;

use crate::state::{CleanupStatus, read_records};

use jackin_config::{MountHealReport, ResolvedWorkspace};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
