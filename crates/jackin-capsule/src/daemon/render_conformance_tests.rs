//! Echo-back conformance harness — the I1 (screen == model) enforcer.
//!
//! Replays PTY bytes through the multiplexer, feeds every composed frame into
//! a virtual client terminal (a second `DamageGrid` emulating the operator's
//! outer terminal), and asserts cell-exact equality between each visible
//! pane's grid and the client screen within the pane rect, plus the
//! frame-model cursor contract. Composition is driven deterministically —
//! direct `compose_pending_frame` / `compose_full_redraw` calls, no ticker,
//! no sleeps.
//!
//! Scenarios that still fail after PR 1 of the capsule rendering plan are the
//! executable spec for PR 3 / PR 4 and carry `#[ignore]` tags naming the
//! fixing PR. Recorded fixtures land in `tests/fixtures/pty/` once a Stage-0
//! operator run id exists; until then the byte streams below are synthetic.

use super::tests::{single_pane_tab_mux, split_tab_mux, test_session, test_session_with_agent};
use super::{FullRedrawReason, InputEvent, Multiplexer, STATUS_BAR_ROWS};
use crate::tui::app::{CursorVisibilityState, cursor_visible_for_state};
use jackin_term::{Cell, DamageGrid};

/// The outer terminal: a second `DamageGrid` sized to the attach client.
/// `apply` is `process()`; the capsule's own `?2026` brackets and mode
/// toggles parse harmlessly. Passthrough events are drained and dropped —
/// they carry no cell content.
struct VirtualClient {
    grid: DamageGrid,
}

impl VirtualClient {
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            grid: DamageGrid::new(rows, cols, 0),
        }
    }

    fn apply(&mut self, frame: &[u8]) {
        self.grid.process(frame);
        drop(self.grid.drain_passthrough());
        drop(self.grid.dirty_spans());
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.set_size(rows, cols);
    }

    fn cell_text(cell: Option<&Cell>) -> String {
        match cell {
            Some(c) if !c.contents.is_empty() => c.contents().to_owned(),
            _ => " ".to_owned(),
        }
    }
}

/// Feed PTY bytes into one session and compose the resulting frame exactly
/// the way the daemon's event loop would: mark the pane dirty, compose, and
/// hand the bytes to the virtual client. Out-of-band passthrough and mode
/// transitions are drained and dropped — they never carry cells.
fn feed_and_compose(
    mux: &mut Multiplexer,
    client: &mut VirtualClient,
    session_id: u64,
    bytes: &[u8],
) {
    if let Some(session) = mux.sessions.get_mut(&session_id) {
        session.feed_pty(bytes);
        drop(session.drain_passthrough());
        drop(session.drain_mode_transitions());
    }
    mux.invalidate(FullRedrawReason::PtyOutput);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

/// Drive one input event the way the daemon loop does: dispatch, then
/// compose whatever the recorded invalidations produce and hand it to the
/// virtual client.
fn dispatch_and_compose(mux: &mut Multiplexer, client: &mut VirtualClient, event: InputEvent) {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

/// I1: after a frame, the client screen equals every visible pane's grid
/// (grapheme, full attribute set, wide flags) within the pane rect. Only
/// valid while no dialog covers the panes and no selection overlay is
/// active — those scenarios assert after the overlay is dismissed.
fn assert_screen_matches_model(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
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
            .sessions
            .get(&pane.id)
            .unwrap_or_else(|| panic!("{context}: pane {} has no session", pane.id));
        let view = session
            .shadow_grid
            .scrollback_view(session.scrollback_offset, pane.inner.rows);
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

/// Frame-model cursor contract: the client cursor is visible exactly when
/// `cursor_visible_for_state` says so for the focused pane, and when visible
/// it sits at the focused pane's VT cursor translated into screen space.
fn assert_cursor_contract(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    let dialog_open = mux.dialog_open();
    let focused = mux.active_focused_id();
    let pane = focused.and_then(|id| mux.visible_panes().into_iter().find(|p| p.id == id));
    let expected_visible = match (focused, &pane) {
        (Some(id), Some(_)) => {
            let session = mux.sessions.get(&id).expect("focused session");
            cursor_visible_for_state(CursorVisibilityState {
                dialog_open,
                focused_pane_available: true,
                focused_session_received_output: session.received_output,
                scrollback_active: session.scrollback_offset != 0,
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
        let session = mux.sessions.get(&id).expect("focused session");
        let (vt_row, vt_col) = session.shadow_grid.cursor_position();
        assert_eq!(
            client.grid.cursor_position(),
            (pane.inner.row + vt_row, pane.inner.col + vt_col),
            "{context}: cursor position must be the focused pane's VT cursor in screen space"
        );
    }
}

fn assert_frame_conformance(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
    assert_screen_matches_model(mux, client, context);
    assert_cursor_contract(mux, client, context);
}

/// Single pane sized to the pane's inner rect, with the session installed and
/// the first-attach frame applied to a fresh virtual client.
fn attached_single_pane() -> (Multiplexer, VirtualClient, u64) {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (session, rx) = test_session(pane.inner.rows, pane.inner.cols);
    // The reply receiver is dropped intentionally: these scenarios never
    // read DSR replies. Sessions that need it use test_session directly.
    drop(rx);
    mux.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    (mux, client, 1)
}

/// Synthetic Codex-style stream chunk: SGR-colored, wrapped prose lines.
fn codex_chunk(i: usize) -> Vec<u8> {
    format!(
        "\x1b[38;5;39mcodex\x1b[0m line {i}: \x1b[1mthinking\x1b[0m about \x1b[38;2;0;255;65mrendering\x1b[0m\r\n"
    )
    .into_bytes()
}

#[test]
fn stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
        if i % 13 == 0 {
            assert_frame_conformance(&mut mux, &client, &format!("stream chunk {i}"));
        }
    }
    assert_frame_conformance(&mut mux, &client, "stream end");
}

#[test]
fn full_scroll_cycle_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Wheel up three steps into history.
    for step in 0..3 {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );
        assert_frame_conformance(&mut mux, &client, &format!("wheel up step {step}"));
    }
    assert_ne!(mux.sessions.get(&sid).unwrap().scrollback_offset, 0);

    // Stream while scrolled: the anchored view must stay equal to the model.
    for i in 60..70 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "anchored feed while scrolled");

    // Wheel back to the live tail — wheel only.
    while mux.sessions.get(&sid).unwrap().scrollback_offset != 0 {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 65,
            },
        );
    }
    assert_frame_conformance(&mut mux, &client, "wheel back to live");
}

#[test]
fn focus_swap_mid_stream_keeps_screen_equal_to_model() {
    let mut mux = split_tab_mux();
    let panes = mux.visible_panes();
    assert_eq!(panes.len(), 2);
    for pane in &panes {
        let (session, rx) = test_session_with_agent(
            pane.inner.rows,
            pane.inner.cols,
            Some(format!("agent-{}", pane.id)),
        );
        drop(rx);
        mux.sessions.insert(pane.id, session);
    }
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
        feed_and_compose(
            &mut mux,
            &mut client,
            panes[1].id,
            format!("pane two output {i}\r\n").as_bytes(),
        );
    }
    assert_frame_conformance(&mut mux, &client, "split stream");

    // Click into the second pane mid-stream, then keep streaming.
    let target = &panes[1];
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: target.inner.row + 1,
            col: target.inner.col + 1,
            button: 0,
        },
    );
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "focus swap mid-stream");
}

#[test]
fn resize_mid_stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.resize(30, 100);
    client.resize(30, 100);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert_frame_conformance(&mut mux, &client, "after grow resize");

    for i in 30..40 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "stream after resize");
}

#[test]
fn dialog_open_close_over_streaming_leaves_no_residue() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_action(crate::tui::message::Action::OpenGithubContext);
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty(), "opening a dialog composes a frame");
    client.apply(&frame);
    assert!(mux.dialog_open());

    // Stream under the open dialog — frames keep flowing.
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_dialog_action(crate::tui::components::dialog::DialogAction::Dismiss);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert!(!mux.dialog_open());
    assert_frame_conformance(&mut mux, &client, "after dialog close over streaming");
}

#[test]
fn alt_screen_session_enter_exit_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Claude-style alt-screen TUI: enter, paint a frame, exit.
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049h\x1b[2J\x1b[H");
    feed_and_compose(
        &mut mux,
        &mut client,
        sid,
        b"\x1b[1;1H\x1b[44m claude \x1b[0m\x1b[3;2HWelcome back\x1b[10;2H> ",
    );
    assert_frame_conformance(&mut mux, &client, "alt screen painted");

    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049l");
    assert_frame_conformance(&mut mux, &client, "after alt-screen exit");
}

#[test]
fn clear_screen_during_selection_overlay_converges_after_clear() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    let pane = mux.visible_panes().into_iter().next().expect("one pane");

    // Drag a selection so composition routes through the Ratatui path
    // (the direct-patch tier refuses while a selection is active).
    let press_row = pane.inner.row + 2;
    let press_col = pane.inner.col + 1;
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row + 1,
            col: press_col + 10,
            button: 32,
        },
    );

    // The program clears its screen while the selection overlay is active.
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[2J\x1b[H$ ");

    // Release and click once to clear the selection overlay.
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MouseRelease {
            row: press_row + 1,
            col: press_col + 10,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MouseRelease {
            row: press_row,
            col: press_col,
            button: 0,
        },
    );

    assert_frame_conformance(&mut mux, &client, "screen cleared during selection");
}

#[test]
fn selection_residue_cleared_after_copy_click() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let press_row = pane.inner.row + 2;
    let press_col = pane.inner.col + 1;
    for event in [
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
        InputEvent::MousePress {
            row: press_row + 1,
            col: press_col + 8,
            button: 32,
        },
        InputEvent::MouseRelease {
            row: press_row + 1,
            col: press_col + 8,
            button: 0,
        },
        // The follow-up click clears the highlight.
        InputEvent::MousePress {
            row: press_row,
            col: press_col,
            button: 0,
        },
        InputEvent::MouseRelease {
            row: press_row,
            col: press_col,
            button: 0,
        },
    ] {
        dispatch_and_compose(&mut mux, &mut client, event);
    }
    assert!(mux.selection.is_none());
    assert_frame_conformance(&mut mux, &client, "after selection cleared");
}

// ---------------------------------------------------------------------------
// Model-expectation cases (PR 4): these assert the *correct* terminal-model
// semantics. They are red against the current jackin-term model and flip
// green when PR 4 lands; the echo-back equality above cannot catch them
// because the virtual client shares the model's bugs.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "fixed by PR 4 (grapheme-cluster cells): combining marks currently overwrite their base character"]
fn combining_mark_joins_base_character() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "e\u{301}!".as_bytes());
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("e\u{301}"),
        "combining acute must join the base cell as one grapheme cluster"
    );
    assert_eq!(
        view.cell(0, 1).map(Cell::contents),
        Some("!"),
        "the next glyph lands in the next cell, not over the cluster"
    );
}

#[test]
#[ignore = "fixed by PR 4 (grapheme-cluster cells): VS16 sequences are currently split across cells"]
fn vs16_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{2601}\u{fe0f}X".as_bytes());
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("\u{2601}\u{fe0f}"),
        "VS16 emoji presentation must stay in the base cell"
    );
}

#[test]
#[ignore = "fixed by PR 4 (grapheme-cluster cells): ZWJ sequences are currently split across cells"]
fn zwj_family_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    feed_and_compose(&mut mux, &mut client, sid, family.as_bytes());
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some(family),
        "the full ZWJ sequence must live in one cell"
    );
}

#[test]
#[ignore = "fixed by PR 4 (wide-lead overwrite): overwriting a wide lead currently leaves the continuation cell stale"]
fn wide_lead_overwrite_blanks_continuation() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{4f60}".as_bytes());
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1;1HA");
    let session = mux.sessions.get(&sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    let continuation = view.cell(0, 1).expect("continuation cell");
    assert!(
        !continuation.is_wide_continuation && continuation.contents.is_empty(),
        "overwriting the wide lead must blank the continuation cell, got {continuation:?}"
    );
}

#[test]
#[ignore = "fixed by PR 4 (DECSTR in-grid): soft reset is currently forwarded raw to the client instead of being handled by the grid"]
fn decstr_soft_reset_is_handled_in_grid() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1m\x1b[?25l\x1b[5;10r");
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[!p");
    let session = mux.sessions.get_mut(&sid).unwrap();
    assert!(
        !session.shadow_grid.hide_cursor(),
        "DECSTR must reset cursor visibility in the grid"
    );
    session.feed_pty(b"x");
    let passthrough = session.drain_passthrough();
    assert!(
        passthrough.iter().all(|seq| !seq.ends_with(b"p")),
        "DECSTR must never be forwarded to the client: {passthrough:?}"
    );
    let view = session.shadow_grid.scrollback_view(0, 1);
    let cell = view
        .cell(
            session.shadow_grid.cursor_position().0,
            session.shadow_grid.cursor_position().1.saturating_sub(1),
        )
        .expect("written cell");
    assert!(
        !cell.attrs.bold,
        "DECSTR must reset SGR attributes before the next write"
    );
}

#[test]
#[ignore = "fixed by PR 4 (DSR clamp): the cursor-position report currently exposes the deferred-wrap phantom column cols+1"]
fn dsr_cursor_report_clamps_phantom_column() {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let cols = pane.inner.cols;
    let (session, mut input_rx) = test_session(pane.inner.rows, cols);
    mux.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.term_rows, mux.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    // Fill the first row to the last column: the cursor enters the
    // deferred-wrap state whose internal column is cols (0-based phantom).
    let fill = "x".repeat(usize::from(cols));
    feed_and_compose(&mut mux, &mut client, 1, fill.as_bytes());
    feed_and_compose(&mut mux, &mut client, 1, b"\x1b[6n");

    let reply = input_rx.try_recv().expect("DSR reply goes to the agent");
    let reply = String::from_utf8(reply).expect("CPR is ASCII");
    let expected = format!("\x1b[1;{cols}R");
    assert_eq!(
        reply, expected,
        "CPR must clamp the phantom column to the last real column"
    );
}
