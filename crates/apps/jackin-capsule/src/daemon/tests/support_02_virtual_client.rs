// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Virtual-client frame-conformance harness for daemon tests.

use super::support_02_fixtures::test_session;
use super::*;

pub(crate) struct VirtualClient {
    pub(crate) grid: DamageGrid,
}

impl VirtualClient {
    pub(crate) fn new(rows: u16, cols: u16) -> Self {
        Self {
            grid: DamageGrid::new(rows, cols, 0),
        }
    }

    pub(crate) fn apply(&mut self, frame: &[u8]) {
        self.grid.process(frame);
        drop(self.grid.drain_passthrough());
        drop(self.grid.dirty_spans());
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.set_size(rows, cols);
    }

    fn cell_text(cell: Option<&Cell>) -> String {
        match cell {
            Some(c) if !c.contents.is_empty() => c.contents().to_owned(),
            _ => " ".to_owned(),
        }
    }
}

pub(crate) fn feed_and_compose(
    mux: &mut Multiplexer,
    client: &mut VirtualClient,
    session_id: u64,
    bytes: &[u8],
) {
    if let Some(session) = mux.session_supervisor.sessions.get_mut(session_id) {
        session.feed_pty(bytes);
        drop(session.drain_passthrough());
    }
    mux.invalidate(FullRedrawReason::PtyOutput);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

pub(crate) fn dispatch_and_compose(
    mux: &mut Multiplexer,
    client: &mut VirtualClient,
    event: InputEvent,
) {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

pub(crate) fn assert_screen_matches_model(
    mux: &mut Multiplexer,
    client: &VirtualClient,
    context: &str,
) {
    assert!(
        !mux.dialog_open(),
        "{context}: I1 cell comparison requires no dialog over the panes"
    );
    let (client_rows, client_cols) = client.grid.size();
    let client_view = client.grid.scrollback_view(0, client_rows);
    let panes = mux.visible_panes();
    assert!(!panes.is_empty(), "{context}: no visible panes");
    for pane in &panes {
        let session = mux
            .session_supervisor
            .sessions
            .get(pane.id)
            .unwrap_or_else(|| panic!("{context}: pane {} has no session", pane.id));
        let view = session
            .shadow_grid
            .scrollback_view(session.scrollback_offset(), pane.inner.rows);
        for row in 0..pane.inner.rows.min(view.rows) {
            for col in 0..pane.inner.cols.min(view.cols) {
                let screen_row = pane.inner.row + row;
                let screen_col = pane.inner.col + col;
                if screen_row >= client_rows || screen_col >= client_cols {
                    continue;
                }
                let model = view.cell(row, col);
                let client_cell = client_view.cell(screen_row, screen_col);
                let model_text = VirtualClient::cell_text(model);
                let client_text = VirtualClient::cell_text(client_cell);
                assert_eq!(
                    model_text, client_text,
                    "{context}: grapheme mismatch pane {} cell ({row},{col}) / screen ({screen_row},{screen_col})",
                    pane.id
                );
                let default = Cell::default();
                let model_cell = model.unwrap_or(&default);
                let client_cell = client_cell.unwrap_or(&default);
                assert_eq!(
                    model_cell.attrs, client_cell.attrs,
                    "{context}: attr mismatch pane {} cell ({row},{col}) text {model_text:?}",
                    pane.id
                );
                assert_eq!(
                    (model_cell.is_wide, model_cell.is_wide_continuation),
                    (client_cell.is_wide, client_cell.is_wide_continuation),
                    "{context}: wide-flag mismatch pane {} cell ({row},{col}) text {model_text:?}",
                    pane.id
                );
            }
        }
    }
}

pub(crate) fn assert_cursor_contract(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    let dialog_open = mux.dialog_open();
    let focused = mux.active_focused_id();
    let pane = focused.and_then(|id| mux.visible_panes().into_iter().find(|p| p.id == id));
    let expected_visible = match (focused, &pane) {
        (Some(id), Some(_)) => {
            let session = mux
                .session_supervisor
                .sessions
                .get(id)
                .expect("focused session");
            cursor_visible_for_state(CursorVisibilityState {
                dialog_open,
                focused_pane_available: true,
                focused_session_received_output: session.received_output,
                scrollback_active: session.scrollback_offset() != 0,
                agent_cursor_hidden: session.shadow_grid.hide_cursor(),
            })
        }
        _ => false,
    };
    assert_eq!(
        !client.grid.hide_cursor(),
        expected_visible,
        "{context}: cursor visibility violates the frame-model contract"
    );
    if expected_visible {
        let id = focused.expect("visible cursor implies focused pane");
        let pane = pane.expect("visible cursor implies pane rect");
        let session = mux
            .session_supervisor
            .sessions
            .get(id)
            .expect("focused session");
        let (vt_row, vt_col) = session.shadow_grid.cursor_position();
        assert_eq!(
            client.grid.cursor_position(),
            (pane.inner.row + vt_row, pane.inner.col + vt_col),
            "{context}: cursor position must be the focused pane's VT cursor in screen space"
        );
    }
}

pub(crate) fn assert_frame_conformance(
    mux: &mut Multiplexer,
    client: &VirtualClient,
    context: &str,
) {
    assert_screen_matches_model(mux, client, context);
    assert_cursor_contract(mux, client, context);
}

pub(crate) fn attached_single_pane() -> (Multiplexer, VirtualClient, u64) {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (session, rx) = test_session(pane.inner.rows, pane.inner.cols);
    // The reply receiver is dropped intentionally: these scenarios never
    // read DSR replies. Sessions that need it use test_session directly.
    drop(rx);
    mux.session_supervisor.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.render.term_rows, mux.render.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    (mux, client, 1)
}
