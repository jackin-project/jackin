// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::{AppConfig, ConfigEditor};

use jackin_core::JackinPaths;

use std::sync::mpsc;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
