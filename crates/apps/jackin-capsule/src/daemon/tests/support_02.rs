// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

mod fixtures;
mod virtual_client;

pub(crate) use fixtures::{
    arm_pending_pr_lookup, make_worktree_layout, pane_kind_cases, split_tab_mux, test_pane_session,
    test_provider_session, test_session, test_session_with_agent, test_shell_session,
};
pub(crate) use virtual_client::{
    VirtualClient, assert_frame_conformance, attached_single_pane, dispatch_and_compose,
    feed_and_compose,
};

pub(super) fn assert_frame_stays_within_geometry(
    frame: &[u8],
    rows: u16,
    cols: u16,
    context: &str,
) {
    let metrics = scan_emitted_frame(frame);
    assert!(
        metrics.full_screen_erases > 0,
        "{context} resize repaint must clear the old geometry"
    );
    assert!(
        metrics.cursor_moves > 0,
        "{context} resize repaint must draw cells"
    );
    assert!(
        metrics.max_row_addressed <= rows && metrics.max_col_addressed <= cols,
        "{context} resize repaint moved outside {rows}x{cols}: max {}x{}",
        metrics.max_row_addressed,
        metrics.max_col_addressed
    );
}

pub(super) fn assert_wheel_cursor_fallback_sent(
    input_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected_bytes: &[u8],
) {
    assert_eq!(
        input_rx
            .try_recv()
            .expect("wheel fallback should reach PTY"),
        expected_bytes,
    );
    input_rx
        .try_recv()
        .expect_err("wheel should not produce extra PTY input");
}

pub(super) fn feed_top_anchored_inline_history(
    session: &mut Session,
    region_bottom: u16,
    lines: usize,
) {
    session.feed_pty(format!("\x1b[1;{region_bottom}r\x1b[{region_bottom};1H").as_bytes());
    for i in 0..lines {
        session.feed_pty(format!("\r\n\x1b[2Khistory {i}").as_bytes());
    }
    session.feed_pty(b"\x1b[r");
}

pub(super) fn view_row_text(session: &Session, row: u16) -> String {
    let (grid_rows, _) = session.shadow_grid.size();
    let view = session
        .shadow_grid
        .scrollback_view(session.scrollback_offset(), grid_rows);
    (0..view.cols)
        .map(|col| {
            view.cell(row, col)
                .map_or(' ', |cell| cell.contents().chars().next().unwrap_or(' '))
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

pub(super) fn attach_drained_client(mux: &mut Multiplexer) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.client_registry.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}
    rx
}

pub(super) fn osc52_payloads(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    while let Ok(bytes) = rx.try_recv() {
        let mut rest = bytes.as_slice();
        while let Some(start) = rest
            .windows(b"\x1b]52;c;".len())
            .position(|w| w == b"\x1b]52;c;")
        {
            let after = &rest[start + 7..];
            let end = after.iter().position(|&b| b == 0x07).unwrap_or(after.len());
            found.push(after[..end].to_vec());
            rest = &after[end..];
        }
    }
    found
}

pub(super) fn expected_osc52_payload(text: &str) -> Vec<u8> {
    let encoded = crate::tui::view::encode_osc52_clipboard_write(text);
    // strip "\x1b]52;c;" prefix and trailing BEL
    encoded[7..encoded.len() - 1].to_vec()
}

pub(super) fn assert_osc52_payloads(
    mux: &mut Multiplexer,
    rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    expected: &[&str],
) {
    mux.client_registry.client.flush_out_of_band();
    let payloads = osc52_payloads(rx);
    assert_eq!(payloads.len(), expected.len(), "OSC 52 write count");
    for (payload, text) in payloads.iter().zip(expected) {
        assert_eq!(payload, &expected_osc52_payload(text));
    }
}

pub(super) fn codex_chunk(i: usize) -> Vec<u8> {
    format!(
        "\x1b[38;5;39mcodex\x1b[0m line {i}: \x1b[1mthinking\x1b[0m about \x1b[38;2;0;255;65mrendering\x1b[0m\r\n"
    )
    .into_bytes()
}

#[must_use]
pub(super) fn percentile_index(len: usize, q: f64) -> usize {
    if len == 0 {
        return 0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "The test vector is bounded and percentile selection only needs a display index."
    )]
    let last = (len - 1) as f64;
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "q is clamped to 0.0..=1.0, so the result is a valid test-vector index."
    )]
    let index = (last * q.clamp(0.0, 1.0)) as usize;
    index
}

pub(super) fn exit_dirty_selected_value(mux: &Multiplexer) -> usize {
    match mux.dialog_top() {
        Some(Dialog::ExitDirty { selected, .. }) => *selected,
        other => panic!("expected ExitDirty on top, got {other:?}"),
    }
}
