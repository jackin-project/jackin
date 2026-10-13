// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn read_packed_git_ref_oid_does_not_cache_truncated_read() {
    // packed-refs cap forces a synthetic-truncation scenario: write
    // exactly PACKED_REFS_MAX_BYTES of content so read_text_bounded's
    // length equals the cap, then mutate underlying bytes and confirm
    // the second read sees the new value (would not, if the truncated
    // first read had cached).
    let temp = tempfile::tempdir().unwrap();
    let packed_refs = temp.path().join("packed-refs-truncated");
    // Pad with comment lines + a real ref entry until total length
    // matches the cap exactly.
    let real_line = "1111111111111111111111111111111111111111 refs/heads/feat/x\n";
    let padding_per_line = "# padding to fill packed-refs to the cap byte limit aaaaaaaaaa\n";
    // Target one byte OVER the cap so metadata.len() > cap triggers
    // the real truncation path (not the exactly-cap edge case).
    let target_size = usize::try_from(PACKED_REFS_MAX_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let mut buf = String::with_capacity(target_size);
    while buf.len() + real_line.len() + padding_per_line.len() <= target_size {
        buf.push_str(padding_per_line);
    }
    buf.push_str(real_line);
    let remaining = target_size.saturating_sub(buf.len());
    buf.extend(std::iter::repeat_n('#', remaining));
    buf.truncate(target_size);
    std::fs::write(&packed_refs, &buf).unwrap();

    drop(read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x"));

    // Mutate same-length bytes (overwrite oid in place); mtime advances.
    let buf2 = buf.replacen(
        "1111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222",
        1,
    );
    std::fs::write(&packed_refs, &buf2).unwrap();

    assert_eq!(
        read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x").as_deref(),
        Some("2222222222222222222222222222222222222222"),
        "truncated first read must not have cached; second read sees fresh content"
    );
}

#[test]
fn packed_refs_cache_eviction_bounds_entries_at_cap() {
    // Create CAP+1 distinct packed-refs paths and read each once.
    // After the (CAP+1)th insert, exactly CAP of the inserted
    // paths must remain — proves both the upper bound AND that
    // eviction removed only one entry (catches over-evict bugs
    // where the cache would degrade to a single entry).
    let temp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..=PACKED_REFS_CACHE_MAX_ENTRIES {
        let path = temp.path().join(format!("packed-refs-evict-{i}"));
        std::fs::write(
            &path,
            format!("1111111111111111111111111111111111111111 refs/heads/branch-{i}\n"),
        )
        .unwrap();
        drop(read_packed_git_ref_oid(
            &path,
            &format!("refs/heads/branch-{i}"),
        ));
        paths.push(path);
    }

    let count = with_packed_refs_cache(|cache| {
        paths
            .iter()
            .filter(|p| cache.contains_key(p.as_path()))
            .count()
    });
    // The just-inserted (CAP+1)th entry MUST be present; eviction
    // targets pre-existing entries, never the new insert.
    assert!(
        with_packed_refs_cache(|cache| cache.contains_key(paths.last().unwrap().as_path())),
        "newly-inserted entry must survive eviction"
    );
    // Exactly one of the previously-inserted CAP entries must have
    // been evicted: count of our tracked paths in the cache should
    // equal CAP, not less (over-evict) or more (no-op evict).
    assert_eq!(
        count, PACKED_REFS_CACHE_MAX_ENTRIES,
        "eviction must drop exactly one entry; saw {count} surviving of CAP={PACKED_REFS_CACHE_MAX_ENTRIES}"
    );
}

#[test]
fn read_git_ref_oid_loose_wins_over_packed() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().to_path_buf();
    std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    std::fs::write(
        git_dir.join("refs/heads/feat-x"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )
    .unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feat-x\n",
    )
    .unwrap();

    assert_eq!(
        read_git_ref_oid(&git_dir, None, "refs/heads/feat-x").as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "loose ref must win over packed-refs entry"
    );
}

#[test]
fn force_spawn_pull_request_context_lookup_skipped_when_in_flight() {
    let mut mux = test_mux(24, 100);
    mux.launch_env.workdir_context.gh_available = true;
    mux.launch_env.workdir_context.is_git_repo = true;
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/x"));
    mux.pr_watch.pull_request_lookup.in_flight = true;
    let id_before = mux.pr_watch.pull_request_lookup.request_id;

    let spawned = mux.force_spawn_pull_request_context_lookup(Instant::now());

    assert!(
        !spawned,
        "force-spawn must no-op when a worker is in flight"
    );
    assert_eq!(
        mux.pr_watch.pull_request_lookup.request_id, id_before,
        "force-spawn skip must not bump request_id"
    );
}

#[test]
fn palette_exit_opens_exit_confirm() {
    let mut mux = single_pane_tab_mux();
    mux.handle_palette_command(PaletteCommand::Exit);

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ConfirmAction {
            kind: ConfirmKind::Exit,
            selected_yes: false
        })
    ));
}

#[test]
fn kitty_escape_in_agent_picker_returns_to_menu() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    let frame = handle_input_frame(&mut mux, InputEvent::Data(b"\r".to_vec()))
        .expect("New tab command should redraw");
    assert!(String::from_utf8_lossy(&frame).contains("New tab"));
    assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));

    let events = mux.control.input_parser.parse(b"\x1b[27;1u");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b".to_vec())]);
    for event in events {
        handle_input_frame(&mut mux, event);
    }

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette { .. })
    ));
}

#[test]
fn mouse_sgr_encoding_preserves_press_and_release() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, termpane::MouseProtocolEncoding::Sgr).unwrap(),
        b"\x1b[<0;12;3M"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, termpane::MouseProtocolEncoding::Sgr).unwrap(),
        b"\x1b[<0;12;3m"
    );
}

#[test]
fn mouse_default_encoding_uses_xterm_fields() {
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, true, termpane::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M ,#"
    );
    assert_eq!(
        encode_mouse_for_protocol(0, 12, 3, false, termpane::MouseProtocolEncoding::Default)
            .unwrap(),
        b"\x1b[M#,#"
    );
}

#[test]
fn mouse_mode_filter_respects_tracking_granularity() {
    use termpane::MouseProtocolMode;

    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::None,
        0,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::Press,
        0,
        false
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::PressRelease,
        32,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        32,
        true
    ));
    assert!(!mouse_event_allowed_for_mode(
        MouseProtocolMode::ButtonMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
    assert!(mouse_event_allowed_for_mode(
        MouseProtocolMode::AnyMotion,
        SGR_NO_BUTTON_MOTION,
        true
    ));
}

#[test]
fn wheel_forwards_to_mouse_enabled_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
    mux.session_supervisor.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned wheel should not redraw jackin❯"
    );
    assert_eq!(
        input_rx.try_recv().expect("wheel should reach PTY"),
        b"\x1b[<64;1;1M"
    );
    input_rx
        .try_recv()
        .expect_err("wheel should not produce extra PTY input");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0
    );
}

#[test]
fn wheel_scrolls_jackin_scrollback_when_mouse_is_disabled() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_pane_session(20, 78, agent);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_offset(), 0);
        mux.session_supervisor.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        assert!(
            redraw.is_some(),
            "{pane_kind} pane scrollback should redraw jackin❯"
        );
        input_rx.try_recv().expect_err(&format!(
            "mouse-disabled {pane_kind} panes must not receive raw wheel bytes"
        ));
        assert_eq!(
            mux.session_supervisor
                .sessions
                .get(1)
                .unwrap()
                .scrollback_offset(),
            3
        );
    }
}

#[test]
fn wheel_back_to_live_repaints_body_and_footer() {
    let mut mux = single_pane_tab_mux();
    // Size the session to the pane so the live tail is exactly the grid.
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (mut session, _input_rx) = test_session(pane.inner.rows, pane.inner.cols);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.session_supervisor.sessions.insert(1, session);
    // The encoder skips cells identical to the reset baseline (the space in
    // "line 39"), so assert on the digit pair unique to the tail row.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    // Park the view in history; the frame switches to the scrollback footer.
    let scrolled = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel into history must repaint");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        3
    );
    // The diff encoder skips cells that match the live footer, so "scrollback"
    // may be split across a cursor-move escape. Match the prefix that is always
    // emitted as a contiguous run.
    assert!(
        contains(&scrolled, b"exit scrollb"),
        "scrolled frame must show the scrollback footer: {:?}",
        String::from_utf8_lossy(&scrolled)
    );

    // Wheel-only return to the live tail: body and footer must repaint
    // together — the D2 regression left the scrollback view and the
    // scrollback footer on screen here.
    let live = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    )
    .expect("wheel back to live must repaint");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0
    );
    assert!(
        !contains(&live, b"scrollback"),
        "footer must return to the live hint: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        contains(&live, b"9"),
        "body diff must include changed live-tail cells: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        !frame_contains_screen_erase(&live),
        "returning to live must repaint in place, not wipe"
    );
}
