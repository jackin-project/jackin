// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::CockpitOutcome;
use std::sync::Mutex;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use ratatui::layout::Rect;

use super::{
    BUILD_LOG_SCROLL_STEP, CockpitContext, QuitConfirmOutcome, apply_quit_confirm_key,
    build_log_action_name, cockpit_action_name, cockpit_outcome_for_quit_confirm,
    emit_dialog_mouse_debug_telemetry, handle_cockpit_mouse_down, is_ctrl_c,
    should_emit_dialog_mouse, update_build_log_mouse_scroll,
};

use crate::LaunchHostTerminal;

use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state,
};

use crate::tui::components::failure_dialog::failure_popup_block_rect;

use crate::{LaunchFailure, LaunchStage};

mod support;
use support::*;
mod case_01;
