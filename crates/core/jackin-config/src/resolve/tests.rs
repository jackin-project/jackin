// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `resolve`.

use super::*;

use jackin_core::{Agent, MountIsolation, WorkspaceName};

use tempfile::tempdir;

use crate::AppConfig;

use crate::schema::{MountHealReport, RoleSource};

mod support;
use support::*;
mod case_01;
mod case_02;
