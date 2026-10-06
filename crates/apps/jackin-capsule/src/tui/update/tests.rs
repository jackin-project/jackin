// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.

use super::{
    ActionFramePlan, DialogActionFramePlan, HoverFramePlan, action_frame_plan,
    dialog_action_frame_plan, dialog_change_redraw_reason, drag_resize_ratio,
    drag_resize_redraw_reason, explicit_redraw_reason, first_attach_redraw_reason,
    focus_change_redraw_reason, hover_frame_plan, palette_route_frame_plan,
    pane_data_redraw_reason, prefix_full_redraw_reason, selection_change_redraw_reason,
    selection_start_redraw_reason, session_exit_redraw_reason, status_change_redraw_reason,
    wheel_scrollback_redraw_reason,
};

use crate::tui::components::dialog::{ConfirmKind, DialogAction, PickerIntent, SplitDirection};

use crate::tui::components::palette::PaletteCommand;

use crate::tui::input::{ArrowDir, PrefixCommand};

use crate::tui::layout::{Rect, SplitOrient};

use crate::tui::message::{Action, PaletteCommandRoute};

use crate::tui::update::FullRedrawReason;

mod support;
use support::*;
mod case_01;
