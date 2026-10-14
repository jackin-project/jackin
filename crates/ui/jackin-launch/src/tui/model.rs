// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::footer::StatusFooterHover;
use ratatui::text::Line;

pub use jackin_core::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind,
    PromptContextLine, StageLabelTransition, StageStatus, StageView,
};

#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal launch-cockpit state flags (failure_ack, \
              build_log_open, build_log_scroll_dragging, build_log_active, \
              container_info_open) — each tracks an independent UI state \
              (popup dismissal, overlay visibility, drag binding, capture \
              activity, dialog visibility) consumed individually by render + \
              subscription paths. Named-field reads match the direct UI-event \
              idiom these flags back."
)]
pub struct LaunchView {
    pub identity: Option<LaunchIdentity>,
    pub stages: Vec<StageView>,
    pub status: String,
    pub failure: Option<LaunchFailure>,
    pub failure_ack: bool,
    pub frame: usize,
    pub build_log_open: bool,
    pub build_log_scroll: termrock::scroll::TailScroll,
    pub build_log_scroll_dragging: bool,
    pub build_log_lines: Vec<String>,
    pub build_log_wrapped_lines: Vec<Line<'static>>,
    pub build_log_wrapped_width: usize,
    pub build_log_viewport_height: usize,
    pub build_log_filled: usize,
    pub build_log_active: bool,
    pub footer_hover: StatusFooterHover,
    pub label_transition: Option<StageLabelTransition>,
    pub failure_copy_hover: Option<FailureCopyTarget>,
    pub failure_copied: Option<FailureCopyTarget>,
    pub failure_revealed: Option<FailureCopyTarget>,
    pub failure_opened: Option<FailureCopyTarget>,
    pub failure_scroll: termrock::scroll::DialogScroll,
    pub container_info_open: bool,
    pub container_info_copied: Option<usize>,
    pub container_info_hover: Option<usize>,
    pub container_info_scroll: termrock::scroll::DialogScroll,
    pub last_dialog_mouse_cell: Option<(u16, u16)>,
    pub quit_confirm: Option<crate::tui::components::prompts::PromptConfirm>,
}

pub use jackin_core::PromptResult;

#[derive(Debug)]
pub struct LaunchRenderContext<'a> {
    pub run_id: &'a str,
    pub no_motion: bool,
    pub rain: Option<&'a crate::tui::components::rain::RainState>,
    pub debug_mode: bool,
    pub jackin_version: &'static str,
}

#[derive(Debug)]
pub struct LaunchViewView<'a> {
    pub context: LaunchRenderContext<'a>,
}

impl jackin_tui::runtime::View<LaunchView> for LaunchViewView<'_> {
    fn render(
        &self,
        model: &LaunchView,
        frame: &mut ratatui::Frame<'_>,
        _area: ratatui::layout::Rect,
    ) {
        crate::tui::view::render_launch_frame(
            frame,
            model,
            self.context.run_id,
            self.context.no_motion,
            self.context.rain,
            self.context.debug_mode,
            self.context.jackin_version,
        );
    }
}
