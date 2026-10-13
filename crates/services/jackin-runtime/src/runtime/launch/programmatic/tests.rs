// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LoadOptions` programmatic-launch validation. No Docker: every case here
//! is decided before the pipeline touches a daemon.

use super::*;

use crate::runtime::LoadOptions;

use jackin_config::RoleSource;

use jackin_protocol::{ExecBinding, ExecKind};

mod support;
use support::*;
mod case_01;
mod case_02;
