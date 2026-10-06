// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{StepCounter, stage_index, telemetry_stage};

use crate::runtime::progress::LaunchProgress;

use jackin_config::{AppConfig, RoleSource};

use jackin_core::RoleSelector;

use jackin_launch::{LaunchCancelled, LaunchDiagnostics};

use std::collections::BTreeMap;

use std::sync::Arc;

mod support;
use support::*;
mod case_01;
