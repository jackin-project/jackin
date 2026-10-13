// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use std::time::Duration;

use super::{LaunchProgress, failure_acknowledged};

use crate::LaunchDiagnostics;

use crate::tui::components::progress_rail::{
    LABEL_SLIDE_FRAMES, animated_label_center, display_stage_statuses, label_strip, labels_line,
};

use crate::{
    LaunchFailure, LaunchStage, StageStatus, active_stage_index, initial_view, update_stage,
};

use jackin_diagnostics::RunDiagnostics;

mod support;
use support::*;
mod case_01;
