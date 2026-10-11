// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn list_names_content_width_includes_trailing_scroll_padding() {
    let config = config_with_long_workspace_name();
    let tmp = tempfile::tempdir().unwrap();
    let state = ManagerState::from_config(&config, tmp.path());

    // Rows without active instances: cursor(1) + 2 spaces + name(27) = 30 cols.
    // The selected highlight adds a trailing-padding span: 30 + 3 = 33.
    let width = list_names_content_width(&state, 19);

    assert_eq!(width, 33);
    assert_eq!(max_offset(width, 19), 14);
}

#[test]
fn list_name_render_clamps_scroll_to_rendered_width() {
    let config = config_with_long_workspace_name();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_names_scroll, u16::MAX);
    state.set_list_names_focused(true);

    let backend = TestBackend::new(70, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    clamp_list_scroll_for_area(Rect::new(0, 0, 70, 24), &mut state, &config, tmp.path());

    terminal
        .draw(|frame| {
            render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
        })
        .unwrap();

    assert_eq!(state.list_names_scroll.offset_x(), 14);
}

#[test]
fn hovered_fitting_list_name_does_not_make_sidebar_horizontally_scrollable() {
    let config = config_with_sidebar_names_that_fit_wide_pane();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.hover_target = Some(crate::tui::state::ManagerHoverTarget::ListRow(
        ManagerListRow::SavedWorkspace(0),
    ));

    let content_width = list_names_content_width(&state, 54);
    assert_eq!(max_offset(content_width, 54), 0);
}

#[test]
fn list_name_vertical_scroll_follows_selected_new_workspace() {
    let config = config_with_many_workspaces();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = state.new_workspace_row_index();
    state.set_list_names_focused(true);

    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_list_body(frame, Rect::new(0, 0, 70, 10), &state, &config, tmp.path());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let dump = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(
        dump.contains("+ New workspace"),
        "selected sentinel should be scrolled into view: {dump:?}"
    );
}

#[test]
fn instance_details_live_snapshot_preserves_mixed_pane_identity() {
    let snapshot = jackin_protocol::InstanceSnapshot {
        active_tab: 0,
        tabs: vec![jackin_protocol::control::TabSnapshot {
            label: "mixed".into(),
            focused_pane: 1,
            instance: Some("tab-config".into()),
            account_id: Some("tab-account".into()),
            panes: vec![
                jackin_protocol::control::PaneSnapshot {
                    session_id: 1,
                    label: "worker".into(),
                    agent: Some("claude-work".into()),
                    account_id: Some("acc-work".into()),
                    state: jackin_protocol::control::AgentState::Idle,
                    agent_status_report: None,
                },
                jackin_protocol::control::PaneSnapshot {
                    session_id: 2,
                    label: "worker".into(),
                    agent: Some("claude-personal".into()),
                    account_id: Some("acc-personal".into()),
                    state: jackin_protocol::control::AgentState::Idle,
                    agent_status_report: None,
                },
            ],
        }],
    };

    let pane = instance_details_pane(
        &identity_test_instance_entry(),
        &[],
        false,
        Some(&snapshot),
        None,
        false,
    );
    let WorkspaceInstancePaneContent::Live { tabs } = pane.content else {
        panic!("expected live snapshot rows");
    };
    assert_eq!(tabs[0].panes[0].label, tabs[0].panes[1].label);
    assert_eq!(tabs[0].panes[0].account_id.as_deref(), Some("acc-work"));
    assert_eq!(tabs[0].panes[1].account_id.as_deref(), Some("acc-personal"));
    assert_eq!(tabs[0].panes[0].config_id.as_deref(), Some("claude-work"));
    assert_eq!(
        tabs[0].panes[1].config_id.as_deref(),
        Some("claude-personal")
    );
}

#[test]
fn instance_details_fallback_preserves_session_account_and_config_identity() {
    let sessions = vec![jackin_core::SessionRecord {
        session_id: "session-1".into(),
        name: "worker".into(),
        agent_runtime: "claude".into(),
        tmux_name: "tmux-worker".into(),
        created_at: "2026-09-20T00:00:00Z".into(),
        status: jackin_core::SessionStatus::Exited,
        last_attached_at: None,
        instance: Some("claude-work".into()),
        account_id: Some("acc-work".into()),
    }];

    let pane = instance_details_pane(
        &identity_test_instance_entry(),
        &sessions,
        false,
        None,
        None,
        false,
    );
    let WorkspaceInstancePaneContent::Sessions { rows } = pane.content else {
        panic!("expected persisted session rows");
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_id.as_deref(), Some("acc-work"));
    assert_eq!(rows[0].config_id.as_deref(), Some("claude-work"));
}

#[test]
fn header_and_data_rows_share_path_column_width() {
    // Short path + long path forces path_w to be the length of the long one.
    let rows = vec![
        mount_row("~/short", "rw", "shared", "git · main"),
        mount_row(
            "~/Projects/very/deeply/nested/directory",
            "ro",
            "worktree",
            "dir",
        ),
    ];
    let path_w = mount_path_width(&rows);
    assert!(path_w >= "~/Projects/very/deeply/nested/directory".len());

    let header = render_mount_header(path_w);
    let data = render_mount_lines(&rows, path_w);

    let header_mode_col = mode_col_start(&header);
    let data0_mode_col = mode_col_start(&data[0]);
    let data1_mode_col = mode_col_start(&data[1]);

    assert_eq!(
        header_mode_col, data0_mode_col,
        "header 'mode' column must align with data row 0"
    );
    assert_eq!(
        header_mode_col, data1_mode_col,
        "header 'mode' column must align with data row 1"
    );
}

#[test]
fn single_row_still_uses_minimum_column_width() {
    // Single short mount — path_w should stay at the floor so the
    // table is still visibly tabular.
    let rows = vec![mount_row(
        "~/Projects/ChainArgos/blockchain-nodes",
        "rw",
        "shared",
        "git · main",
    )];
    let path_w = mount_path_width(&rows);
    assert_eq!(path_w, "~/Projects/ChainArgos/blockchain-nodes".len());

    let header = render_mount_header(path_w);
    let data = render_mount_lines(&rows, path_w);
    assert_eq!(mode_col_start(&header), mode_col_start(&data[0]));
}

#[test]
fn empty_rows_uses_floor_for_header() {
    // Empty case: header should still render with the floor width and
    // include the two-space gap between every column.
    let path_w = mount_path_width(&[]);
    assert_eq!(path_w, "Destination".len());
    let header = render_mount_header(path_w);
    // "  <path padded>  <mode padded>  <iso padded>  Type"
    let expected = format!(
        "  {path:<path_w$}  {mode:<mw$}  {iso:<iw$}  Type",
        path = "Destination",
        mode = "Mode",
        iso = "Isolation",
        path_w = path_w,
        mw = MOUNT_MODE_COL_WIDTH,
        iw = MOUNT_ISOLATION_COL_WIDTH,
    );
    let s = line_text(&header);
    assert_eq!(s, expected);
}

#[test]
fn header_has_two_space_gap_between_columns() {
    // Regression for the "Mode Type" spacing bug, extended to cover the
    // new `Isolation` column: header must emit a literal two-space gap
    // between every column (Mode → Isolation → Type), mirroring the gap
    // data rows emit between `rw`/`ro`, the isolation label, and the
    // kind. Additionally pins the type-column alignment: the `Type`
    // header label must start at the same character offset as the data
    // row's kind label.
    let rows = vec![mount_row("~/p", "rw", "shared", "folder")];
    let path_w = mount_path_width(&rows);
    let header = render_mount_header(path_w);
    let data = render_mount_lines(&rows, path_w);
    let header_text = line_text(&header);
    let data_text = line_text(&data[0]);
    // Header should have "Mode" followed by gap+padding to the isolation column.
    assert!(
        header_text.contains("Isolation"),
        "expected header to contain 'Isolation'; got {header_text:?}"
    );
    let header_type_offset = header_text.find("Type").expect("header has 'Type'");
    let data_kind_offset = data_text.find("folder").expect("data row has 'folder'");
    assert_eq!(
        header_type_offset, data_kind_offset,
        "Type column misaligned: header at {header_type_offset}, data at {data_kind_offset}"
    );
}

#[test]
fn mount_row_renders_isolation_badge_for_worktree() {
    let m = MountConfig {
        src: "/tmp/x".into(),
        dst: "/workspace/x".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Worktree,
    };
    let rows = format_mount_rows(std::slice::from_ref(&m));
    assert_eq!(rows.len(), 1);
    let path_w = mount_path_width(&rows);
    let lines = render_mount_lines(&rows, path_w);
    let text = line_text(&lines[0]);
    assert!(
        text.contains("worktree"),
        "missing worktree badge: {text:?}"
    );
}

#[test]
fn mount_row_renders_isolation_badge_for_shared() {
    let m = MountConfig {
        src: "/tmp/x".into(),
        dst: "/workspace/x".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    };
    let rows = format_mount_rows(std::slice::from_ref(&m));
    assert_eq!(rows.len(), 1);
    let path_w = mount_path_width(&rows);
    let lines = render_mount_lines(&rows, path_w);
    let text = line_text(&lines[0]);
    assert!(text.contains("shared"), "missing shared badge: {text:?}");
}

#[test]
fn empty_mounts_reserves_row_for_none_placeholder() {
    // 0 data rows + "(none)" placeholder (1 row) + 1 header + 2 borders = 4.
    assert_eq!(mount_block_height(&[]), 4);
}

#[test]
fn single_mount_fits_in_four_rows() {
    // Regression: the current-dir pane used to hard-code `Length(5)`
    // which left an extra empty line inside the block. Correct total
    // for a 1-mount workspace is 1 data + 1 header + 2 borders = 4.
    assert_eq!(mount_block_height(&[mount("/tmp/a")]), 4);
}

#[test]
fn multiple_mounts_scale_linearly() {
    assert_eq!(mount_block_height(&[mount("/tmp/a"), mount("/tmp/b")]), 5);
    assert_eq!(
        mount_block_height(&[mount("/a"), mount("/b"), mount("/c")]),
        6
    );
}

#[test]
fn many_mounts_clamp_to_twelve() {
    let mounts: Vec<MountConfig> = (0..20).map(|i| mount(&format!("/m/{i}"))).collect();
    assert_eq!(mount_block_height(&mounts), 12);
}

#[test]
fn global_mount_heights_match_rendered_line_count() {
    let same_path = mount("/cache/shared");
    let split_path = MountConfig {
        src: "/host/cache".into(),
        dst: "/container/cache".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    };

    assert_eq!(global_mounts_content_height(&[same_path]), 2);
    assert_eq!(global_mounts_content_height(&[split_path]), 3);
    assert_eq!((global_mounts_content_height(&[]) + 2).min(12), 3);
}

#[test]
fn subpanel_content_column_alignment() {
    // General
    let backend = TestBackend::new(40, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_general_subpanel(f, Rect::new(0, 0, 40, 4), &summary().workdir);
    })
    .unwrap();
    let general_col = first_content_indent(&term).expect("general has content");

    // Mounts
    let backend = TestBackend::new(40, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let cache = MountInfoCache::default();
        render_mounts_subpanel(f, Rect::new(0, 0, 40, 4), &[], &cache, 0, 0, false);
    })
    .unwrap();
    let mounts_col = first_content_indent(&term).expect("mounts has content");

    // Roles, "any role" branch (no allowed list)
    let cfg = AppConfig::default();
    let backend = TestBackend::new(40, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_agents_subpanel(f, Rect::new(0, 0, 40, 4), None, &cfg);
    })
    .unwrap();
    let agents_any_col = first_content_indent(&term).expect("roles 'any' has content");

    assert_eq!(
        general_col, SUBPANEL_CONTENT_INDENT,
        "General first char at col {general_col}, expected {SUBPANEL_CONTENT_INDENT}"
    );
    assert_eq!(
        mounts_col, SUBPANEL_CONTENT_INDENT,
        "Mounts first char at col {mounts_col}, expected {SUBPANEL_CONTENT_INDENT}"
    );
    assert_eq!(
        agents_any_col, SUBPANEL_CONTENT_INDENT,
        "Roles (any) first char at col {agents_any_col}, expected {SUBPANEL_CONTENT_INDENT}"
    );
}
