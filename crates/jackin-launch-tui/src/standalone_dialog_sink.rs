// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Process-wide [`StandaloneDialogSink`] installed at CLI startup.
//!
//! `jackin_core::error_popup` / `jackin_core::exit_dialog_with_inspect`
//! route through a global port-trait sink. The sink impl forwards to the
//! same standalone dialog renderers that `jackin_runtime::progress`
//! historically called directly. Keeping the impl in `jackin-launch-tui`
//! preserves the L0 (port) → L3 (impl) Branch-by-Abstraction layout that
//! the rest of the codebase applies to `BuildLogSink`, `DebugLogSink`,
//! and `OperatorNoticeSink`.
//!
//! The renderer only needs [`LaunchHostTerminal::is_debug_mode`] during
//! the standalone dialog path — `reveal_file` / `open_file` are only
//! called from the live launch UI, never from the post-attach surface —
//! so the embedded host terminal here forwards a minimal subset and a
//! default for everything else.

use std::io::Write;

use jackin_core::LaunchHostTerminal;
use jackin_core::StandaloneDialogSink;

use crate::progress::standalone_error_popup;
use crate::progress::standalone_exit_dialog_with_inspect;

/// Inline [`LaunchHostTerminal`] used inside the standalone dialog sink.
///
/// Only `is_debug_mode` is consulted by the standalone dialog renderers;
/// the other methods exist to satisfy the trait and are not invoked by
/// the post-attach / cancellation paths.
#[derive(Debug)]
struct SinkHostTerminal;

impl LaunchHostTerminal for SinkHostTerminal {
    fn set_rich_surface_active(&self, _active: bool) {}

    fn host_screen_owned(&self) -> bool {
        false
    }

    fn is_debug_mode(&self) -> bool {
        jackin_diagnostics::is_debug_mode()
    }

    fn emit_compact_line(&self, kind: &str, line: &str) {
        jackin_diagnostics::emit_compact_line(kind, line);
    }

    fn emit_debug_line(&self, category: &str, line: &str) {
        jackin_diagnostics::emit_debug_line(category, line);
    }

    fn set_pointer_shape(&self, pointer: bool) {
        let seq = if pointer {
            jackin_tui::ansi::POINTER_HAND
        } else {
            jackin_tui::ansi::POINTER_DEFAULT
        };
        let mut out = std::io::stdout();
        drop(out.write_all(seq.as_bytes()));
        drop(out.flush());
    }

    fn copy_to_clipboard(&self, payload: &str) -> bool {
        let mut out = std::io::stdout();
        out.write_all(&jackin_tui::ansi::encode_osc52_clipboard_write(payload))
            .and_then(|()| out.flush())
            .is_ok()
    }

    fn reveal_file(&self, _path: &std::path::Path) -> bool {
        false
    }

    fn open_file(&self, _path: &std::path::Path) -> bool {
        false
    }
}

static SINK_HOST_TERMINAL: SinkHostTerminal = SinkHostTerminal;

/// Single process-wide [`StandaloneDialogSink`] impl backed by the
/// standalone dialog renderers.
#[derive(Debug)]
struct JackinStandaloneDialogSink;

impl StandaloneDialogSink for JackinStandaloneDialogSink {
    fn error_popup(&self, title: &str, message: &str) -> anyhow::Result<()> {
        standalone_error_popup(title, message, &SINK_HOST_TERMINAL, env!("JACKIN_VERSION"))
    }

    fn exit_dialog_with_inspect(
        &self,
        title: &str,
        context: &[jackin_core::PromptContextLine],
        options: Vec<String>,
        worktrees_per_record: &[Vec<jackin_core::WorktreeInspect>],
    ) -> anyhow::Result<usize> {
        standalone_exit_dialog_with_inspect(
            title,
            context,
            options,
            worktrees_per_record,
            &SINK_HOST_TERMINAL,
            env!("JACKIN_VERSION"),
        )
    }
}

static SINK: JackinStandaloneDialogSink = JackinStandaloneDialogSink;

/// Install the process-wide [`StandaloneDialogSink`]. Idempotent —
/// [`jackin_core::set_global_dialog_sink`] keeps the first install.
///
/// Call this once at CLI startup, before any code path that may invoke
/// the post-attach exit dialog or error popup.
pub fn install() {
    jackin_core::set_global_dialog_sink(&SINK);
}

#[cfg(test)]
mod tests;
