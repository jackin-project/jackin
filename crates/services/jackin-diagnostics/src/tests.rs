// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use jackin_core::JackinPaths;

use crate::run::RunDiagnostics;

mod support;
use support::*;
mod case_01;
mod secret_scrub;
