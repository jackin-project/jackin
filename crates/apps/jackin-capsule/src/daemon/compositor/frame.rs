// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use crate::tui::model::{VisibleAgentState, visible_agent_state_from_protocol};

use super::super::{
    CursorVisibilityState, Multiplexer, Rect, append_osc_window_title,
    compose_outer_terminal_title, cursor_visible_for_state,
};
use super::AssertedClientState;

impl Multiplexer {
    pub(crate) fn compose_pending_frame(&mut self) -> Vec<u8> {
        if self.render.rendered_generation == self.render.frame_generation {
            return Vec::new();
        }
        let generation = self.render.frame_generation;
        self.render.last_invalidate_reason.take();
        let wipe = self.render.wipe_pending.take();
        let started = Instant::now();
        if wipe.is_some() {
            drop(self.render.ratatui_terminal.clear());
        }
        let Some(output) = self.compose_ratatui_frame() else {
            return Vec::new();
        };
        self.render.rendered_generation = generation;
        jackin_diagnostics::record_render(
            u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            0,
        );
        self.frame_with_title(output)
    }

    pub(crate) fn append_outer_terminal_title(&mut self, buf: &mut Vec<u8>) {
        let title = compose_outer_terminal_title(
            &self.launch_env.workdir,
            self.context_bar_branch(),
            self.pr_watch.pull_request_context.as_deref(),
        );
        if self.client_registry.last_outer_terminal_title.as_deref() == Some(title.as_str()) {
            return;
        }
        append_osc_window_title(buf, &title);
        self.client_registry.last_outer_terminal_title = Some(title);
    }

    fn frame_with_title(&mut self, ratatui_output: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        self.append_outer_terminal_title(&mut out);
        if out.is_empty() {
            return ratatui_output;
        }
        out.reserve(ratatui_output.len());
        out.extend_from_slice(&ratatui_output);
        out
    }

    pub(crate) fn snapshot_session_states(&self) -> Vec<(u64, VisibleAgentState)> {
        self.session_supervisor
            .sessions
            .iter()
            .map(|(id, s)| (id, visible_agent_state_from_protocol(s.state)))
            .collect()
    }

    pub(crate) fn append_client_state_reconciliation(
        &mut self,
        buf: &mut Vec<u8>,
        focused_id: Option<u64>,
        focused_pane_rect: Option<Rect>,
    ) {
        let dialog_open = self.dialog_open();
        let focused = focused_id.and_then(|id| self.session_supervisor.sessions.get(id));
        let desired = AssertedClientState {
            bracketed_paste: focused.is_some_and(|s| s.shadow_grid.bracketed_paste()),
            application_cursor: focused.is_some_and(|s| s.shadow_grid.application_cursor()),
            kitty_flags: focused.map_or(0, |s| s.shadow_grid.kitty_kb_flags()),
            cursor_style: focused.map_or(0, |s| s.shadow_grid.cursor_style()),
            cursor_visible: match (focused, focused_pane_rect) {
                (Some(session), Some(_)) => cursor_visible_for_state(CursorVisibilityState {
                    dialog_open,
                    focused_pane_available: true,
                    focused_session_received_output: session.received_output,
                    scrollback_active: session.scrollback_offset() != 0,
                    agent_cursor_hidden: session.shadow_grid.hide_cursor(),
                }),
                _ => false,
            },
        };
        let last = self.render.last_asserted_client_state;
        if last.is_none_or(|l| l.bracketed_paste != desired.bracketed_paste) {
            buf.extend_from_slice(if desired.bracketed_paste {
                b"\x1b[?2004h"
            } else {
                b"\x1b[?2004l"
            });
        }
        if last.is_none_or(|l| l.application_cursor != desired.application_cursor) {
            buf.extend_from_slice(if desired.application_cursor {
                b"\x1b[?1h"
            } else {
                b"\x1b[?1l"
            });
        }
        if last.is_none_or(|l| l.cursor_style != desired.cursor_style) {
            use std::io::Write as _;
            let _unused = write!(buf, "\x1b[{} q", desired.cursor_style);
        }
        if last.is_none_or(|l| l.kitty_flags != desired.kitty_flags) {
            buf.extend_from_slice(b"\x1b[<u");
            if desired.kitty_flags != 0 {
                use std::io::Write as _;
                let _unused = write!(buf, "\x1b[>{}u", desired.kitty_flags);
            }
        }
        if desired.cursor_visible {
            if let (Some(session), Some(rect)) = (focused, focused_pane_rect) {
                let (vt_row, vt_col) = session.shadow_grid.cursor_position();
                use std::io::Write as _;
                let _unused = write!(
                    buf,
                    "\x1b[{};{}H",
                    rect.row + vt_row + 1,
                    rect.col + vt_col + 1
                );
                buf.extend_from_slice(b"\x1b[?25h");
            }
        } else if last.is_none_or(|l| l.cursor_visible) {
            buf.extend_from_slice(b"\x1b[?25l");
        }
        self.render.last_asserted_client_state = Some(desired);
    }
}
