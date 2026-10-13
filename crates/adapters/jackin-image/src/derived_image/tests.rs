// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `derived_image`.

use super::*;

use jackin_core::Agent;

#[cfg(unix)]
use std::os::unix::fs::symlink;

use std::process::Command;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
