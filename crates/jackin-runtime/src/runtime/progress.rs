// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host wiring for launch progress: re-exports presentation types from
//! `jackin-launch` and installs host-terminal/desktop adapters.
//!
//! Not responsible for rendering or product composition tests — those live in
//! `jackin-launch` (`progress` + `tui`). This module only bridges host I/O
//! (clipboard, reveal/open, diagnostics compact lines) into the launch surface.

use std::io::Write;

pub use jackin_core::LaunchCancelled;
use jackin_core::{LaunchHostTerminal, LaunchOutputSink};
use jackin_launch::LaunchTuiOutputSink;
pub use jackin_launch::progress::LaunchProgress;
pub use jackin_launch::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchMessage, LaunchStage, LaunchTargetKind,
    LaunchView, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    active_stage_index, initial_view, update_launch_view, update_stage,
};

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
        let seq = jackin_launch::terminal_protocol::encode_pointer_shape(pointer);
        let mut out = std::io::stdout();
        drop(out.write_all(&seq));
        drop(out.flush());
    }

    fn copy_to_clipboard(&self, payload: &str) -> bool {
        let mut out = std::io::stdout();
        out.write_all(&jackin_launch::terminal_protocol::encode_clipboard_write(
            payload,
        ))
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
    jackin_launch::progress::prelaunch_select_choice(
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
    jackin_launch::progress::standalone_select_with_context(
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
    jackin_launch::progress::standalone_error_popup(
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
    worktrees_per_record: &[Vec<jackin_core::WorktreeInspect>],
) -> anyhow::Result<usize> {
    jackin_launch::progress::standalone_exit_dialog_with_inspect(
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
    candidates: &[jackin_core::LaunchCandidate],
) -> anyhow::Result<jackin_core::LaunchDialogResult> {
    jackin_launch::progress::standalone_launch_dialog(
        title,
        candidates,
        host_terminal(),
        env!("JACKIN_VERSION"),
    )
}

pub fn rich_terminal_supported() -> bool {
    jackin_launch::tui::terminal::rich_terminal_supported()
}
