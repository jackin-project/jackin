// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `view`.

use super::{
    CapsuleRatatuiFrame, pane_limit_failure_message, render_capsule_ratatui_frame,
    spawn_failure_agent_label, spawn_failure_message, spawn_request_failure_message,
    tab_limit_failure_message,
};

use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;

use crate::tui::components::status_bar::{PrefixMode, STATUS_BAR_ROWS};

use crate::tui::layout::Tab;

use crate::tui::layout::available_content_rows;

use crate::tui::model::HoverTarget;

use ratatui::{Terminal, backend::TestBackend};

mod support;
use support::*;
mod case_01;
mod case_02;
