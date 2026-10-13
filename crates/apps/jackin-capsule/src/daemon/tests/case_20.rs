// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn export_selected_file_palette_action_sends_file_export_frames() {
    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("workspace");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();

    let mut mux = single_pane_tab_mux();
    mux.launch_env.workdir = workdir.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.client_registry.client.flush_out_of_band();
    while rx.try_recv().is_ok() {}

    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hsee report.txt now\x1b[1;1H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 4,
        end_row: 0,
        end_col: 13,
    });

    mux.handle_palette_command(PaletteCommand::ExportSelectedFileAndOpen);
    mux.client_registry.client.flush_out_of_band();

    input_rx
        .try_recv()
        .expect_err("export-selected command must not forward bytes to the pane");
    let bytes = rx.try_recv().expect("file-export-start frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode file-export-start frame")
        .expect("file-export-start frame");
    let ServerFrame::FileExportStart(start) = frame else {
        panic!("expected FileExportStart");
    };
    assert_eq!(
        start.source_path,
        workdir
            .join("report.txt")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(start.file_name, "report.txt");
    assert!(!start.reveal_after_export);
    assert!(start.open_after_export);
    assert!(
        mux.clipboard
            .clipboard_image_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("File export and open queued: report.txt"))
    );
}

#[test]
fn export_file_under_cursor_palette_action_reports_missing_path_token() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1H    \x1b[1;2H");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.handle_palette_command(PaletteCommand::ExportFileUnderCursor);
    mux.client_registry.client.flush_out_of_band();

    rx.try_recv()
        .expect_err("missing path token must not emit file-export frames");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("No exportable file path under focused cursor")
    );
}

#[test]
fn export_selected_file_palette_action_reports_missing_selection() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.handle_palette_command(PaletteCommand::ExportSelectedFile);
    mux.client_registry.client.flush_out_of_band();

    rx.try_recv()
        .expect_err("missing selection must not emit file-export frames");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("No selected file path to export")
    );
}

#[tokio::test]
async fn stage_image_path_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.handle_palette_command(PaletteCommand::StageImageFromClipboardPath);

    let bytes = rx.try_recv().expect("host-stage-image-path frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-stage-image-path frame")
        .expect("host-stage-image-path frame");
    assert_eq!(frame, ServerFrame::HostStageImageFromClipboardPath);
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn paste_image_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.handle_palette_command(PaletteCommand::PasteImageFromClipboard);

    let bytes = rx.try_recv().expect("host-paste-image frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-paste-image frame")
        .expect("host-paste-image frame");
    assert_eq!(frame, ServerFrame::HostPasteImageFromClipboard);
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn stage_image_palette_action_sends_typed_protocol_frame() {
    let mut mux = single_pane_tab_mux();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.handle_palette_command(PaletteCommand::StageImageFromClipboard);

    let bytes = rx.try_recv().expect("host-stage-image frame");
    let tag = bytes[0];
    let mut payload = &bytes[1..];
    let frame = read_server_frame(&mut payload, tag)
        .await
        .expect("decode host-stage-image frame")
        .expect("host-stage-image frame");
    assert_eq!(frame, ServerFrame::HostStageImageFromClipboard);
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::StageOnly
    );
}

#[tokio::test]
async fn chunked_image_start_reports_visible_receiving_notice() {
    let mut mux = single_pane_tab_mux();

    handle_client_frame(
        &mut mux,
        ClientFrame::ClipboardImageStart(jackin_protocol::attach::ClipboardImageStart {
            transfer_id: 42,
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            size: 16 * 1024 * 1024,
        }),
    );

    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Image paste: receiving 16777216 bytes")
    );
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[tokio::test]
async fn chunked_stage_image_start_reports_staging_receiving_notice() {
    let mut mux = single_pane_tab_mux();
    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    handle_client_frame(
        &mut mux,
        ClientFrame::ClipboardImageStart(jackin_protocol::attach::ClipboardImageStart {
            transfer_id: 43,
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            size: 4 * 1024 * 1024,
        }),
    );

    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Image staging: receiving 4194304 bytes")
    );
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::StageOnly
    );
}

#[test]
fn stage_only_clipboard_image_response_does_not_paste_path() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;

    mux.stage_clipboard_image_response_with(
        jackin_protocol::attach::ClipboardImage {
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        },
        |_| Ok(PathBuf::from("/jackin/run/clipboard/clipboard-test.png")),
    );

    input_rx
        .try_recv()
        .expect_err("stage-only response must not paste into the focused pane");
    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some("Image staged: /jackin/run/clipboard/clipboard-test.png (8 bytes)")
    );
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[test]
fn clipboard_image_response_reports_when_path_cannot_be_pasted() {
    let mut mux = single_pane_tab_mux();

    mux.stage_clipboard_image_response_with(
        jackin_protocol::attach::ClipboardImage {
            format: jackin_protocol::attach::ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        },
        |_| Ok(PathBuf::from("/jackin/run/clipboard/clipboard-test.png")),
    );

    assert_eq!(
        mux.clipboard.clipboard_image_notice.as_deref(),
        Some(
            "Image staged: /jackin/run/clipboard/clipboard-test.png (8 bytes; no writable focused pane; not pasted)"
        )
    );
    assert_eq!(
        mux.clipboard.clipboard_image_insert_mode,
        ClipboardImageInsertMode::PastePath
    );
}

#[test]
fn drag_extending_a_word_click_recopies_on_release() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col = inner.col + 6; // inside "/model"
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(
        mux.clipboard.selection_copied,
        "word click copies immediately"
    );

    // Extend the selection past the word, then release: the clipboard no
    // longer matches the highlight, so release must copy again.
    drop(apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row,
            col: inner.col + 13,
        },
    ));
    assert!(
        !mux.clipboard.selection_copied,
        "motion must invalidate the word-click copy"
    );
    drop(apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col: inner.col + 13,
            button: 0,
        },
    ));
    assert!(
        mux.clipboard.selection_copied,
        "release re-copies the extended span"
    );

    assert_osc52_payloads(&mut mux, &mut rx, &["/model", "/model to"]);
}

#[test]
fn double_click_on_scrolled_back_row_copies_the_history_word() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("w{i:02}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 5, "history must exist for the scrolled press");
    session.scroll_by(5);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row; // top visible row = scrollback row filled-5
    let col = inner.col + 1;
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));

    let sel = mux
        .clipboard
        .selection
        .expect("double-click on history selects");
    assert_eq!(
        sel.anchor_row,
        filled - 5,
        "anchor must be the scrolled-to content row"
    );
    let expected = format!("w{:02}", filled - 5);
    assert_osc52_payloads(&mut mux, &mut rx, &[expected.as_str()]);
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .expect("session")
            .scrollback_offset(),
        5,
        "word selection must not move the scrollback view"
    );
}
