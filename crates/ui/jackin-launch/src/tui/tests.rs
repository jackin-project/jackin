// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch surface product composition tests (migrated out of jackin-runtime).

use crate::tui::components::build_log_dialog::{
    BUILD_LOG_WRAP_PREFIX, build_log_scroll_metrics, refresh_build_log_layout,
    render_build_log_dialog, wrap_build_log_lines,
};

use crate::tui::components::chrome::bottom_chrome_areas;

use crate::tui::components::failure_dialog::{
    failure_copy_payload, failure_copy_target_at, failure_popup_rect_for_rows, failure_popup_rows,
    failure_popup_value_rect,
};

use crate::tui::components::footer::StatusFooterHover;

use crate::tui::components::progress_rail::{
    LABEL_VIEW_WIDTH, PROGRESS_RAIL_WIDTH, faded_color, label_edge_fade_factor, labels_line,
};

use crate::tui::components::prompts::{
    PromptConfirm, PromptError, PromptText, draw_confirm, draw_error_popup, draw_text_prompt,
};

use crate::tui::view::render_launch_frame as render_launch_frame_view;

use crate::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, LaunchView,
    StageStatus, StageView, initial_view, update_stage,
};

use ratatui::backend::TestBackend;

use ratatui::{Frame, layout::Rect, style::Color};

mod support;
use support::*;
mod case_01;
mod case_02;
