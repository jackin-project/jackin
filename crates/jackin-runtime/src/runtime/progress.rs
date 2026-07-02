//! Re-export of launch TUI progress types plus host-side prelaunch helpers.
//!
//! Not responsible for rendering — the TUI render functions are re-exported
//! only for test use (`#[cfg(test)]`). Production callers use `LaunchProgress`
//! and `LaunchStage` exclusively.

use std::io::Write;
#[cfg(test)]
use std::time::Duration;

pub use jackin_core::launch_progress::LaunchCancelled;
use jackin_core::launch_progress::{LaunchHostTerminal, LaunchOutputSink};
use jackin_launch_tui::LaunchTuiOutputSink;
pub use jackin_launch_tui::progress::LaunchProgress;
#[cfg(test)]
use jackin_launch_tui::tui::components::build_log_dialog::BUILD_LOG_WRAP_PREFIX;
#[cfg(test)]
use jackin_launch_tui::tui::components::build_log_dialog::{
    build_log_scroll_metrics, refresh_build_log_layout, render_build_log_dialog,
    wrap_build_log_lines,
};
#[cfg(test)]
use jackin_launch_tui::tui::components::failure_dialog::failure_popup_hyperlink_overlay;
#[cfg(test)]
use jackin_launch_tui::tui::components::failure_dialog::{
    failure_copy_payload, failure_copy_target_at,
};
#[cfg(test)]
use jackin_launch_tui::tui::components::failure_dialog::{
    failure_popup_rect_for_rows, failure_popup_rows, failure_popup_value_rect,
};
#[cfg(test)]
use jackin_launch_tui::tui::components::progress_rail::{
    LABEL_SLIDE_FRAMES, LABEL_VIEW_WIDTH, PROGRESS_RAIL_WIDTH, animated_label_center,
    display_stage_statuses, faded_color, label_edge_fade_factor, label_strip, labels_line,
};
#[cfg(test)]
use jackin_launch_tui::tui::components::prompts::{
    draw_confirm, draw_error_popup, draw_text_prompt,
};
#[cfg(test)]
use jackin_launch_tui::tui::view::render_launch_frame as render_launch_frame_view;
pub use jackin_launch_tui::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchMessage, LaunchStage, LaunchTargetKind,
    LaunchView, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    active_stage_index, initial_view, update_launch_view, update_stage,
};
#[cfg(test)]
use jackin_tui::components::ConfirmState;
#[cfg(test)]
use jackin_tui::components::{ErrorPopupState, TextInputState};
#[cfg(test)]
use jackin_tui::theme::DANGER_RED;
#[cfg(test)]
use ratatui::Frame;
#[cfg(test)]
use ratatui::layout::Rect;
#[cfg(test)]
use ratatui::style::Color;

struct HostTerminal;

impl LaunchHostTerminal for HostTerminal {
    fn set_rich_surface_active(&self, active: bool) {
        jackin_diagnostics::set_rich_surface_active(active);
    }

    fn host_screen_owned(&self) -> bool {
        jackin_diagnostics::host_screen_owned()
    }

    fn is_debug_mode(&self) -> bool {
        jackin_diagnostics::is_debug_mode()
    }

    fn emit_compact_line(&self, kind: &str, line: &str) {
        jackin_diagnostics::emit_compact_line(kind, line);
        tracing::info!(kind, "{line}");
    }

    fn emit_debug_line(&self, category: &str, line: &str) {
        jackin_diagnostics::emit_debug_line(category, line);
        tracing::debug!(category, "{line}");
    }

    fn set_pointer_shape(&self, pointer: bool) {
        let seq = if pointer {
            jackin_core::POINTER_HAND
        } else {
            jackin_core::POINTER_DEFAULT
        };
        let mut out = std::io::stdout();
        drop(out.write_all(seq.as_bytes()));
        drop(out.flush());
    }

    fn copy_to_clipboard(&self, payload: &str) -> bool {
        let mut out = std::io::stdout();
        out.write_all(&jackin_core::encode_osc52_clipboard_write(payload))
            .and_then(|()| out.flush())
            .is_ok()
    }

    fn reveal_file(&self, path: &std::path::Path) -> bool {
        match jackin_host::host_desktop::reveal_host_file(path) {
            Ok(()) => true,
            Err(err) => {
                jackin_diagnostics::emit_compact_line(
                    "launch-reveal",
                    &format!("failed to reveal {}: {err:#}", path.display()),
                );
                false
            }
        }
    }

    fn open_file(&self, path: &std::path::Path) -> bool {
        match jackin_host::host_desktop::open_host_file(path) {
            Ok(()) => true,
            Err(err) => {
                jackin_diagnostics::emit_compact_line(
                    "launch-open-file",
                    &format!("failed to open {}: {err:#}", path.display()),
                );
                false
            }
        }
    }
}

static HOST_TERMINAL: HostTerminal = HostTerminal;

pub(crate) fn host_terminal() -> &'static dyn LaunchHostTerminal {
    &HOST_TERMINAL
}

static LAUNCH_OUTPUT: LaunchTuiOutputSink = LaunchTuiOutputSink;

pub(crate) fn launch_output() -> &'static dyn LaunchOutputSink {
    &LAUNCH_OUTPUT
}

pub fn prelaunch_select_choice(
    no_motion: bool,
    title: &str,
    items: Vec<String>,
) -> anyhow::Result<usize> {
    jackin_launch_tui::progress::prelaunch_select_choice(
        no_motion,
        title,
        items,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

/// Standalone forced-choice picker with a `context` block above the options.
///
/// For callers that run after the launch progress surface has been torn down
/// — the post-attach worktree-cleanup prompt. Enters its own rich surface (or
/// draws into the host guard's screen when one is active).
pub fn standalone_select_with_context(
    title: &str,
    context: &[PromptContextLine],
    items: Vec<String>,
) -> anyhow::Result<usize> {
    jackin_launch_tui::progress::standalone_select_with_context(
        title,
        context,
        items,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

/// Standalone error popup for launch-adjacent failures that need operator
/// acknowledgement in the same rich surface.
pub fn standalone_error_popup(title: &str, message: &str) -> anyhow::Result<()> {
    jackin_launch_tui::progress::standalone_error_popup(
        title,
        message,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

/// D23/D24 standalone exit dialog with inspect support.
pub fn standalone_exit_dialog_with_inspect(
    title: &str,
    context: &[PromptContextLine],
    options: Vec<String>,
    worktrees_per_record: &[Vec<jackin_core::launch_progress::WorktreeInspect>],
) -> anyhow::Result<usize> {
    jackin_launch_tui::progress::standalone_exit_dialog_with_inspect(
        title,
        context,
        options,
        worktrees_per_record,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

/// D23/D21 standalone launch dialog with delete-in-place support.
pub fn standalone_launch_dialog(
    title: &str,
    candidates: &[jackin_core::launch_progress::LaunchCandidate],
) -> anyhow::Result<jackin_core::launch_progress::LaunchDialogResult> {
    jackin_launch_tui::progress::standalone_launch_dialog(
        title,
        candidates,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

pub fn rich_terminal_supported() -> bool {
    jackin_launch_tui::tui::terminal::rich_terminal_supported()
}

/// Bail with the canonical rich-terminal requirement message unless the
/// current terminal can host the launch surface. Both `LaunchProgress::new`
/// and the pre-launch `prelaunch_select_choice` picker gate through this so
/// the message cannot drift between them.
#[cfg(test)]
fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    no_motion: bool,
    rain: Option<&jackin_launch_tui::tui::components::rain::RainState>,
) {
    render_launch_frame_view(
        frame,
        view,
        run_id,
        run_log_path,
        no_motion,
        rain,
        jackin_diagnostics::is_debug_mode(),
        env!("JACKIN_VERSION"),
    );
}

#[cfg(test)]
mod tests;
