// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use std::os::unix::fs::{MetadataExt as _, symlink};

use std::os::unix::fs::PermissionsExt as _;

mod support;
use support::*;
mod case_01;
