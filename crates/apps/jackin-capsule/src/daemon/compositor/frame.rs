// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Frame orchestration, titles, snapshots, and client reconciliation.

use std::time::Instant;

use crate::tui::model::{VisibleAgentState, visible_agent_state_from_protocol};

use super::super::{
    CursorVisibilityState, Multiplexer, Rect, append_osc_window_title,
    compose_outer_terminal_title, cursor_visible_for_state,
};
use super::AssertedClientState;

impl Multiplexer {
    /// Compose the frame for the current state when the generation moved
    /// since the last composed frame. This is the only compositor: there are
    /// no repaint tiers — every frame is the full widget tree, and the only
    /// branch is the wipe policy (a real `\x1b[2J` precedes the frame for
    /// `FirstAttach` and `Resize` only).
    pub(crate) fn compose_pending_frame(&mut self) -> Vec<u8> {
        if self.render.rendered_generation == self.render.frame_generation {
            return Vec::new();
        }
        let generation = self.render.frame_generation;
        self.render.last_invalidate_reason.take();
        let wipe = self.render.wipe_pending.take();
        let started = Instant::now();
        if wipe.is_some() {
            // Terminal::clear() emits the screen erase and resets Ratatui's
            // previous buffer so FirstAttach/Resize get a real baseline reset.
            drop(self.render.ratatui_terminal.clear());
        }
        let Some(output) = self.compose_ratatui_frame() else {
            // compose_ratatui_frame only returns None if the Ratatui draw
            // itself errored — effectively impossible with SocketBackend.
            // Skip the frame; the generation stays ahead so the next loop
            // pass retries.
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

    /// Prepend the outer-terminal title to a freshly composed frame.
    ///
    /// `append_outer_terminal_title` writes nothing when the title is unchanged
    /// (the common case: workdir/branch/PR static), so the frame is returned by
    /// move with no copy. Only a title change allocates and prepends.
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

    /// Reconcile the client terminal's cursor and mode state with the
    /// focused pane's grid — the frame model's non-cell payload (§3.4).
    /// Desired state is derived fresh every frame; only transitions against
    /// the last asserted state are emitted, except the cursor position +
    /// show, which must be re-asserted whenever visible because Ratatui's
    /// draw hides the cursor at the start of every frame.
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
            // DECSCUSR per pane: the focused pane's requested cursor shape
            // flows through the same reconciliation as every other mode, so
            // one pane's shape can never leak into another (D5).
            use std::io::Write as _;
            let _unused = write!(buf, "\x1b[{} q", desired.cursor_style);
        }
        if last.is_none_or(|l| l.kitty_flags != desired.kitty_flags) {
            // Pop whatever the previous pane pushed, then push the desired
            // level — the same pop+push shape the focus-swap reset used, so
            // the outer terminal's kitty stack depth stays bounded.
            buf.extend_from_slice(b"\x1b[<u");
            if desired.kitty_flags != 0 {
                use std::io::Write as _;
                let _unused = write!(buf, "\x1b[>{}u", desired.kitty_flags);
            }
        }
        if desired.cursor_visible {
            // Position at the focused pane's VT cursor in screen space, then
            // show. Re-asserted every frame: Ratatui's draw hid the cursor.
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
            // Hidden, and either never asserted or previously visible. The
            // draw already emitted ?25l this frame; this keeps the asserted
            // record explicit for the first frame after attach.
            buf.extend_from_slice(b"\x1b[?25l");
        }
        self.render.last_asserted_client_state = Some(desired);
    }
}
