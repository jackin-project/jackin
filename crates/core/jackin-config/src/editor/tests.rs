// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `editor`.

use super::*;

use crate::RoleSource;

use jackin_core::{Agent, WorkspaceName};

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
