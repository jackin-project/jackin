// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `message`.

use super::prefix_command_action;

use super::{
    Action, ConfirmedActionRoute, InputDispatchContext, PaletteCommandRoute, PaletteToggleRoute,
    StatusBarClickState, branch_context_bar_click_action, confirmed_action_route,
    input_event_action, mouse_chrome_update_action, mouse_release_action, palette_command_route,
    palette_toggle_route, pane_button_motion_action, status_bar_click_action,
};

use crate::tui::components::branch_context_bar::BranchContextBarHit;

use crate::tui::components::dialog::{ConfirmKind, PaletteCommand};

use crate::tui::components::dialog::{PickerIntent, SplitDirection};

use crate::tui::input::InputEvent;

use crate::tui::input::PrefixCommand;

mod case_01;
