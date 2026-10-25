// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use jackin_core::LaunchHostTerminal;
use jackin_core::StandaloneDialogSink;

use crate::progress::standalone_error_popup;
use crate::progress::standalone_exit_dialog_with_inspect;

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
        let seq = termrock::osc::encode_pointer(if pointer {
            termrock::osc::PointerShape::Pointer
        } else {
            termrock::osc::PointerShape::Default
        });
        let mut out = std::io::stdout();
        drop(out.write_all(&seq));
        drop(out.flush());
    }

    fn copy_to_clipboard(&self, payload: &str) -> bool {
        let mut out = std::io::stdout();
        out.write_all(&termrock::osc::encode_clipboard(
            termrock::osc::ClipboardWrite {
                selection: termrock::osc::ClipboardSelection::Clipboard,
                text: payload,
            },
        ))
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

#[derive(Debug)]
struct JackinStandaloneDialogSink;

impl JackinStandaloneDialogSink {
    fn error_popup_with_renderer(
        title: &str,
        message: &str,
        renderer: impl FnOnce(
            &str,
            &str,
            &'static dyn LaunchHostTerminal,
            &'static str,
        ) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        renderer(title, message, &SINK_HOST_TERMINAL, env!("JACKIN_VERSION"))
    }
}

impl StandaloneDialogSink for JackinStandaloneDialogSink {
    fn error_popup(&self, title: &str, message: &str) -> anyhow::Result<()> {
        Self::error_popup_with_renderer(title, message, standalone_error_popup)
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

pub fn install() {
    jackin_core::set_global_dialog_sink(&SINK);
}
