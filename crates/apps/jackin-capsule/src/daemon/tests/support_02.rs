// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn make_worktree_layout(temp: &Path, worktree_name: &str) -> (PathBuf, PathBuf) {
    let workdir = temp.join("workdir");
    let common_git = temp.join("repo/.git");
    let wt_git = common_git.join(format!("worktrees/{worktree_name}"));
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::create_dir_all(&wt_git).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", wt_git.display()),
    )
    .unwrap();
    (workdir, common_git)
}

pub(super) fn arm_pending_pr_lookup(mux: &mut Multiplexer, branch_name: &str, request_id: u64) {
    mux.pr_watch.pull_request_lookup.request_id = request_id;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch(branch_name));
    mux.open_github_context_dialog(Instant::now());
}

pub(super) fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, Some("codex".to_owned()))
}

pub(super) fn test_shell_session(
    rows: u16,
    cols: u16,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, None)
}

pub(super) fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
    [(Some("codex"), "agent"), (None, "shell")]
}

pub(super) fn test_pane_session(
    rows: u16,
    cols: u16,
    agent: Option<&str>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, agent.map(str::to_owned))
}

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

pub(super) fn test_session_with_agent(
    rows: u16,
    cols: u16,
    agent: Option<String>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let mut session = Session::new_for_test(
        "Test".to_owned(),
        agent.clone(),
        None,
        (rows, cols),
        100,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.usage_capability =
        agent.map(
            |agent| jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: format!("test-{agent}"),
                surface_id: agent,
            },
        );
    (session, input_rx)
}

pub(super) fn test_provider_session(
    provider: jackin_protocol::Provider,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (mut session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: provider.label().to_owned(),
        env_overrides: vec![("ANTHROPIC_AUTH_TOKEN".into(), "zai-test-token".into())],
    });
    (session, input_rx)
}

pub(super) fn split_tab_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    let mut tab = Tab::new_single("Shell", 1, "test");
    assert!(tab.tree.split_h(1, 2, SplitPosition::After));
    mux.session_supervisor.tabs.push(tab);
    drop(mux.compose_pending_frame());
    mux
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

pub(super) struct VirtualClient {
    pub(super) grid: DamageGrid,
}

impl VirtualClient {
    pub(super) fn new(rows: u16, cols: u16) -> Self {
        Self {
            grid: DamageGrid::new(rows, cols, 0),
        }
    }

    pub(super) fn apply(&mut self, frame: &[u8]) {
        self.grid.process(frame);
        drop(self.grid.drain_passthrough());
        drop(self.grid.dirty_spans());
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        self.grid.set_size(rows, cols);
    }

    fn cell_text(cell: Option<&Cell>) -> String {
        match cell {
            Some(c) if !c.contents.is_empty() => c.contents().to_owned(),
            _ => " ".to_owned(),
        }
    }
}

pub(super) fn feed_and_compose(
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

pub(super) fn dispatch_and_compose(
    mux: &mut Multiplexer,
    client: &mut VirtualClient,
    event: InputEvent,
) {
    mux.handle_input(event);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
}

pub(super) fn assert_screen_matches_model(
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

pub(super) fn assert_cursor_contract(mux: &mut Multiplexer, client: &VirtualClient, context: &str) {
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

pub(super) fn assert_frame_conformance(
    mux: &mut Multiplexer,
    client: &VirtualClient,
    context: &str,
) {
    assert_screen_matches_model(mux, client, context);
    assert_cursor_contract(mux, client, context);
}

pub(super) fn attached_single_pane() -> (Multiplexer, VirtualClient, u64) {
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
