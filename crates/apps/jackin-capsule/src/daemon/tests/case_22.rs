// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    assert!(mux.clipboard.selection.is_none());
    assert_frame_conformance(&mut mux, &client, "after selection cleared");
}

#[test]
fn combining_mark_joins_base_character() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "e\u{301}!".as_bytes());
    let session = mux.session_supervisor.sessions.get(sid).unwrap();
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
fn vs16_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{2601}\u{fe0f}X".as_bytes());
    let session = mux.session_supervisor.sessions.get(sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("\u{2601}\u{fe0f}"),
        "VS16 emoji presentation must stay in the base cell"
    );
    assert!(
        view.cell(0, 0).expect("VS16 lead").is_wide,
        "VS16 emoji presentation must occupy two model columns"
    );
    assert!(
        view.cell(0, 1)
            .expect("VS16 continuation")
            .is_wide_continuation,
        "VS16 emoji presentation must create a continuation cell"
    );
    assert_eq!(
        view.cell(0, 2).map(Cell::contents),
        Some("X"),
        "next glyph must land after the grown VS16 cluster"
    );
}

#[test]
fn halfwidth_katakana_dakuten_width_echoes_to_client() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{ff76}\u{ff9e}X".as_bytes());
    assert_frame_conformance(&mut mux, &client, "dakuten width echo-back");
    let session = mux.session_supervisor.sessions.get(sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some("\u{ff76}\u{ff9e}"),
        "halfwidth katakana dakuten must stay in the base cell"
    );
    assert!(
        view.cell(0, 0).expect("dakuten lead").is_wide,
        "dakuten cluster must occupy two model columns"
    );
    assert!(
        view.cell(0, 1)
            .expect("dakuten continuation")
            .is_wide_continuation,
        "dakuten cluster must create a continuation cell"
    );
    assert_eq!(
        view.cell(0, 2).map(Cell::contents),
        Some("X"),
        "next glyph must land after the grown dakuten cluster"
    );
}

#[test]
fn zwj_family_emoji_stays_one_cluster() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    feed_and_compose(&mut mux, &mut client, sid, family.as_bytes());
    assert_frame_conformance(&mut mux, &client, "ZWJ family width echo-back");
    let session = mux.session_supervisor.sessions.get(sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    assert_eq!(
        view.cell(0, 0).map(Cell::contents),
        Some(family),
        "the full ZWJ sequence must live in one cell"
    );
}

#[test]
fn wide_lead_overwrite_blanks_continuation() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, "\u{4f60}".as_bytes());
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1;1HA");
    let session = mux.session_supervisor.sessions.get(sid).unwrap();
    let view = session.shadow_grid.scrollback_view(0, 1);
    let continuation = view.cell(0, 1).expect("continuation cell");
    assert!(
        !continuation.is_wide_continuation && continuation.contents.is_empty(),
        "overwriting the wide lead must blank the continuation cell, got {continuation:?}"
    );
}

#[test]
fn decstr_soft_reset_is_handled_in_grid() {
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[1m\x1b[?25l\x1b[5;10r");
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[!p");
    let session = mux.session_supervisor.sessions.get_mut(sid).unwrap();
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
fn dsr_cursor_report_clamps_phantom_column() {
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let cols = pane.inner.cols;
    let (session, mut input_rx) = test_session(pane.inner.rows, cols);
    mux.session_supervisor.sessions.insert(1, session);
    let mut client = VirtualClient::new(mux.render.term_rows, mux.render.term_cols);
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

#[test]
fn osc_color_query_answers_with_the_attached_terminal_palette() {
    // Codex's startup probe: it paints no backgrounds at all until OSC 11
    // is answered, so the reply must carry the attach client's real colors.
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (session, mut input_rx) = test_session(pane.inner.rows, pane.inner.cols);
    mux.session_supervisor.sessions.insert(1, session);
    mux.client_registry.attached_terminal.default_fg = Some((0xe6, 0xe6, 0xe6));
    mux.client_registry.attached_terminal.default_bg = Some((0x17, 0x17, 0x17));
    mux.apply_client_colors_to_sessions();
    let mut client = VirtualClient::new(mux.render.term_rows, mux.render.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    feed_and_compose(&mut mux, &mut client, 1, b"\x1b]10;?\x1b\\\x1b]11;?\x07");

    let fg_reply = input_rx.try_recv().expect("OSC 10 reply goes to the agent");
    assert_eq!(fg_reply, b"\x1b]10;rgb:e6e6/e6e6/e6e6\x1b\\");
    let bg_reply = input_rx.try_recv().expect("OSC 11 reply goes to the agent");
    assert_eq!(bg_reply, b"\x1b]11;rgb:1717/1717/1717\x07");
}

#[test]
fn decscusr_reconciles_per_pane_and_never_forwards_raw() {
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let (mut mux, mut client, sid) = attached_single_pane();
    feed_and_compose(&mut mux, &mut client, sid, b"hello");

    // The agent picks a bar cursor; the next frame reconciles it.
    if let Some(session) = mux.session_supervisor.sessions.get_mut(sid) {
        session.feed_pty(b"\x1b[5 q");
        let passthrough = session.drain_passthrough();
        assert!(
            passthrough.is_empty(),
            "DECSCUSR must never be forwarded raw: {passthrough:?}"
        );
    }
    mux.invalidate(FullRedrawReason::PtyOutput);
    let frame = mux.compose_pending_frame();
    assert!(
        contains(&frame, b"\x1b[5 q"),
        "frame must assert the pane's cursor style: {:?}",
        String::from_utf8_lossy(&frame)
    );
    client.apply(&frame);
    assert_frame_conformance(&mut mux, &client, "after DECSCUSR");

    // Unchanged style: no re-assertion on the next frame.
    feed_and_compose(&mut mux, &mut client, sid, b" world");
    mux.invalidate(FullRedrawReason::PtyOutput);
    let next = mux.compose_pending_frame();
    assert!(
        !contains(&next, b"\x1b[5 q"),
        "unchanged cursor style must not be re-asserted: {:?}",
        String::from_utf8_lossy(&next)
    );
}

#[test]
fn render_perf_probe() {
    let (mut mux, mut client, sid) = attached_single_pane();
    let mut durations_us: Vec<u128> = Vec::with_capacity(300);
    let mut bytes: Vec<usize> = Vec::with_capacity(300);
    for i in 0..300 {
        if let Some(session) = mux.session_supervisor.sessions.get_mut(sid) {
            session.feed_pty(&codex_chunk(i));
            drop(session.drain_passthrough());
        }
        mux.invalidate(FullRedrawReason::PtyOutput);
        let started = Instant::now();
        let frame = mux.compose_pending_frame();
        durations_us.push(started.elapsed().as_micros());
        bytes.push(frame.len());
        client.apply(&frame);
    }
    durations_us.sort_unstable();
    bytes.sort_unstable();
    let pick = |v: &[u128], q: f64| v[percentile_index(v.len(), q)];
    let pick_b = |v: &[usize], q: f64| v[percentile_index(v.len(), q)];
    {
        println!(
            "render_perf_probe: frames={} duration_us p50={} p95={} max={} bytes p50={} p95={} max={}",
            durations_us.len(),
            pick(&durations_us, 0.50),
            pick(&durations_us, 0.95),
            durations_us.last().copied().unwrap_or_default(),
            pick_b(&bytes, 0.50),
            pick_b(&bytes, 0.95),
            bytes.last().copied().unwrap_or_default(),
        );
    }
    assert_frame_conformance(&mut mux, &client, "perf probe end");
}

#[test]
fn exit_dirty_down_arrow_advances_selection_via_handle_input() {
    // Zero live panes — exactly the dirty-exit modal scenario.
    let mut mux = test_mux(30, 100);
    mux.dialog_push(Dialog::new_exit_dirty(
        vec!["holla   1 changed".to_owned()],
        Arc::from([]),
    ));
    assert_eq!(exit_dirty_selected_value(&mux), 0);

    // Down arrow, as the input parser hands it to handle_input.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert_eq!(
        exit_dirty_selected_value(&mux),
        1,
        "down arrow must advance the dirty-exit selection through the full daemon path"
    );

    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x42]));
    assert_eq!(exit_dirty_selected_value(&mux), 2);

    // Up arrow walks back.
    mux.handle_input(InputEvent::Data(vec![0x1b, 0x5b, 0x41]));
    assert_eq!(exit_dirty_selected_value(&mux), 1);
}
