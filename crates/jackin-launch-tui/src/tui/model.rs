//! Launch cockpit model types shared with runtime orchestration.

use jackin_tui::components::StatusFooterHover;
use ratatui::text::Line;

pub use jackin_core::launch_progress::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind,
    PromptContextLine, StageLabelTransition, StageStatus, StageView,
};

#[derive(Debug, Clone)]
#[allow(
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
    /// Operator dismissed the failure popup (Enter/Esc). The render task owns
    /// input, so it sets this flag; `LaunchProgress::stage_failed` awaits it
    /// rather than reading stdin itself.
    pub failure_ack: bool,
    pub frame: usize,
    /// Operator opened the live docker-build log overlay.
    pub build_log_open: bool,
    /// Lines scrolled up from the tail of the build log (0 = follow newest).
    pub build_log_scroll: jackin_tui::scroll::TailScroll,
    /// Pointer drag is currently bound to the build-log scrollbar track.
    pub build_log_scroll_dragging: bool,
    /// Render-safe snapshot of retained docker-build output.
    pub build_log_lines: Vec<String>,
    /// Wrapped docker-build output for the current dialog viewport width.
    pub build_log_wrapped_lines: Vec<Line<'static>>,
    pub build_log_wrapped_width: usize,
    pub build_log_viewport_height: usize,
    pub build_log_filled: usize,
    /// Whether docker-build capture is currently active.
    pub build_log_active: bool,
    /// Pointer hover state for clickable footer spans.
    pub footer_hover: StatusFooterHover,
    pub label_transition: Option<StageLabelTransition>,
    /// Pointer is hovering a copyable value in the failure popup.
    pub failure_copy_hover: Option<FailureCopyTarget>,
    /// Last failure-popup value copied via OSC 52. Drives visible feedback.
    pub failure_copied: Option<FailureCopyTarget>,
    /// Last failure-popup file path revealed through the host file manager.
    pub failure_revealed: Option<FailureCopyTarget>,
    /// Last failure-popup file path opened through the host file manager.
    pub failure_opened: Option<FailureCopyTarget>,
    /// Scroll offsets for the failure popup body. Long diagnostics or next-step
    /// rows can exceed the viewport-safe popup height; the offset persists here
    /// so the body scrolls instead of silently clipping the overflow.
    pub failure_scroll: jackin_tui::components::DialogBodyScroll,
    /// Operator opened the shared container info dialog from the footer chip.
    pub container_info_open: bool,
    /// Last copied row in the container info dialog.
    pub container_info_copied: Option<usize>,
    /// Row in the container info dialog the pointer is hovering (a copyable
    /// value), driving the link hover-colour change.
    pub container_info_hover: Option<usize>,
    /// Scroll offsets for the container info dialog body. The state is rebuilt
    /// each frame, so the offset persists here and is threaded into the rebuilt
    /// `ContainerInfoState` — long paths scroll instead of clipping.
    pub container_info_scroll: jackin_tui::components::DialogBodyScroll,
    /// Operator pressed Ctrl+Q: the "Exit jackin❯?" confirmation overlays the
    /// cockpit and owns input until answered. `None` = not confirming. Ctrl+C
    /// bypasses this entirely (immediate hard cancel).
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

// Re-exported from `jackin_core` (Workstream 1 — architecture/boundaries:
// the type was here because `jackin_launch` was a TUI, not the launch
// orchestrator. Lower crates (`jackin_env::env_resolver`) needed
// `PromptResult` and had to depend upward on this crate purely for the
// type. Both directions now point inward through `jackin_core`.)
pub use jackin_core::PromptResult;

// G0 contract wiring — `View<LaunchView>` is the shared TEA view half
// (D5). The render function (`render_launch_frame`) is the long-standing
// implementation; this wrapper carries the run-context that the shared
// `View::render` signature cannot reach. The trait impl is a thin
// delegation so the contract is satisfied at the type level without
// disturbing the existing render path. The `RichRenderer::render` call
// site in `tui/run.rs` continues to call `render_launch_frame` directly;
// migrating it to the trait dispatch is a follow-up tracked in G3.
#[derive(Debug)]
pub struct LaunchRenderContext<'a> {
    pub run_id: &'a str,
    pub run_log_path: &'a str,
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
            self.context.run_log_path,
            self.context.no_motion,
            self.context.rain,
            self.context.debug_mode,
            self.context.jackin_version,
        );
    }
}
