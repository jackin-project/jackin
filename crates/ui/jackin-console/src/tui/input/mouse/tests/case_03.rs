// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn click_on_sentinel_row_sets_selected_to_sentinel_idx() {
    // 3 saved workspaces ⇒ rows are:
    //   y=3  → index 0 ("Current directory")
    //   y=4,5,6 → indices 1, 2, 3 (saved)
    //   y=7  → visual spacer
    //   y=8  → visual index 5 (sentinel "+ New workspace")
    let mut state = list_state_with_saved(3);
    state.selected = 0;
    handle_mouse(&mut state, mouse_at(10, 8), term(100));
    assert_eq!(state.selected, 4, "sentinel_idx = saved_count + 1 = 4");
}

#[test]
fn click_on_workspace_list_spacer_does_not_change_selected() {
    let mut state = list_state_with_saved(3);
    state.selected = 2;
    handle_mouse(&mut state, mouse_at(10, 7), term(100));
    assert_eq!(state.selected, 2);
}

#[test]
fn click_outside_list_rows_does_not_change_selected() {
    // Several "outside" positions must all leave selected untouched:
    //   - Click above the list (y < 3, e.g. in the header)
    //   - Click on the left border (x=0)
    //   - Click at x >= seam (right pane territory)
    //   - Click below the list content (footer)
    let mut state = list_state_with_saved(3);
    state.selected = 2;
    let initial = state.selected;

    // In the header.
    handle_mouse(&mut state, mouse_at(10, 1), term(100));
    assert_eq!(state.selected, initial, "click in header must not select");

    // On the top border of the list block.
    handle_mouse(&mut state, mouse_at(10, 2), term(100));
    assert_eq!(state.selected, initial, "click on top border");

    // On the left border column.
    handle_mouse(&mut state, mouse_at(0, 3), term(100));
    assert_eq!(state.selected, initial, "click on left border");

    // Past the sentinel row (y=8+ when we have 3 saved workspaces).
    handle_mouse(&mut state, mouse_at(10, 9), term(100));
    assert_eq!(state.selected, initial, "click below sentinel");

    // In the right pane (x=60, well clear of the default seam).
    handle_mouse(&mut state, mouse_at(60, 5), term(100));
    assert_eq!(state.selected, initial, "click in details pane");

    // In the footer.
    handle_mouse(&mut state, mouse_at(10, 29), term(100));
    assert_eq!(state.selected, initial, "click on footer row");
}

#[test]
fn click_on_seam_still_starts_drag_not_selection() {
    // Regression guard for batch 14: a click on the seam column must
    // kick off a drag and NOT retarget selection, even when the y
    // coordinate happens to overlap a valid list row.
    let mut state = list_state_with_saved(3);
    state.selected = 0;
    // Default split on a 100-col terminal ⇒ seam at column
    // `DEFAULT_SPLIT_PCT`. y=4 maps to list index 1 in our layout —
    // if seam didn't win, selection would flip to 1.
    handle_mouse(&mut state, mouse_at(DEFAULT_SPLIT_PCT, 4), term(100));
    assert!(state.drag_state.is_some(), "click on seam must start drag");
    assert_eq!(
        state.selected, 0,
        "seam-click must not change selection even when y lands on a list row"
    );
}

#[test]
fn click_scrollable_mount_block_focuses_it() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);

    // Right pane starts at x=30 for a 100-col terminal. Workspace mounts
    // block starts at y=5 after General's 3 rows.
    handle_mouse_with_config(&mut state, mouse_at(31, 6), term(100), Some(&config));

    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Workspace));
}

#[test]
fn click_current_directory_mount_block_focuses_and_scrolls_it() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp
        .path()
        .join("very-long-current-directory-name-that-forces-horizontal-scrolling-in-the-preview");
    std::fs::create_dir_all(&cwd).unwrap();
    let config = jackin_config::AppConfig::default();
    let mut state = current_dir_state_at(&cwd);
    assert!(state.is_current_dir_selected());

    handle_mouse_with_config(&mut state, mouse_at(31, 6), term(100), Some(&config));
    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Workspace));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 31, 6),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_mounts_scroll.offset_x(),
        MOUSE_HORIZONTAL_SCROLL_STEP
    );
}

#[test]
fn click_non_scrollable_area_clears_mount_focus() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    // y=3 is inside the General block, which is not a horizontal-scroll
    // target.
    handle_mouse_with_config(&mut state, mouse_at(31, 3), term(100), Some(&config));

    assert_eq!(state.list_scroll_focus(), None);
}

#[test]
fn horizontal_mouse_wheel_scrolls_block_under_pointer() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    // Global mounts block starts immediately after General (3 rows) and
    // the one-mount Workspace mounts block (5 rows): y=10.
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(state.list_mounts_scroll.offset_x(), 0);
    assert_eq!(
        state.list_global_mounts_scroll.offset_x(),
        MOUSE_HORIZONTAL_SCROLL_STEP
    );
    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Global));
}

#[test]
fn vertical_mouse_wheel_does_not_scroll_horizontal_only_list_block() {
    // W3C rule: ScrollUp/Down are vertical events; horizontal-only blocks
    // (List view mounts) must ignore them. Only ScrollLeft/Right scroll them.
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_global_mounts_scroll.offset_x(),
        0,
        "ScrollDown must not change horizontal scroll on a horizontal-only block"
    );

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(state.list_global_mounts_scroll.offset_x(), 0);
}

#[test]
fn vertical_mouse_wheel_routes_to_block_under_pointer_not_stale_focus() {
    let mut config = config_with_scrollable_workspace_and_global_mounts();
    for idx in 0..6 {
        config.add_mount(
            &format!("global-extra-{idx}"),
            MountConfig {
                src: format!("/host/source/extra/{idx}"),
                dst: format!("/container/destination/extra/{idx}"),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
            None,
        );
    }
    let mut state = selected_demo_state(&config);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    let areas = list_scroll_areas(&state, term(100), Some(&config)).expect("list areas");
    let mouse = mouse_kind_at(
        MouseEventKind::ScrollDown,
        areas.global.area.x + 1,
        areas.global.area.y + 1,
    );

    handle_mouse_with_config(&mut state, mouse, term(100), Some(&config));

    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Global));
    assert_eq!(state.list_mounts_scroll.offset_y(), 0);
    assert_eq!(state.list_global_mounts_scroll.offset_y(), 1);
}

#[test]
fn horizontal_mouse_wheel_clamps_stored_offset_at_block_end() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);

    for _ in 0..100 {
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 31, 11),
            term(100),
            Some(&config),
        );
    }

    let global_mounts: Vec<MountConfig> = config
        .list_mount_rows()
        .into_iter()
        .filter(|row| row.scope.is_none())
        .map(|row| row.mount)
        .collect();
    let global_area = Rect {
        x: 30,
        y: 10,
        width: 70,
        height: 5,
    };
    let expected_max = max_scroll_offset(
        global_mounts_content_width(global_mounts.as_slice()),
        crate::tui::layout::scroll_viewport_width(global_area),
    );
    assert_eq!(state.list_global_mounts_scroll.offset_x(), expected_max);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollLeft, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_global_mounts_scroll.offset_x(),
        expected_max.saturating_sub(MOUSE_HORIZONTAL_SCROLL_STEP),
        "left-scroll after overscrolling right must move immediately, not burn hidden offset"
    );
}

#[test]
fn horizontal_mouse_wheel_reaches_rendered_workspace_width() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(
        repo.join(".git").join("HEAD"),
        "ref: refs/heads/feat/backend-rust-gdpr-purge-normalization\n",
    )
    .unwrap();
    let config = config_with_long_git_type_mount(&repo);
    let mut state = selected_demo_state(&config);
    state.mount_info_cache.refresh_mounts(
        &config
            .workspaces
            .get("demo")
            .expect("demo workspace")
            .mounts,
    );

    for _ in 0..100 {
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollRight, 31, 6),
            term(100),
            Some(&config),
        );
    }

    let workspace = config.workspaces.get("demo").unwrap();
    let workspace_area = Rect {
        x: 30,
        y: 5,
        width: 70,
        height: 4,
    };
    let expected_max = max_scroll_offset(
        workspace_mounts_content_width(workspace.mounts.as_slice()),
        crate::tui::layout::scroll_viewport_width(workspace_area),
    );

    assert_eq!(
        state.list_mounts_scroll.offset_x(),
        expected_max,
        "mouse/touch scroll must clamp at the same rendered width keyboard scrolling reaches"
    );
}

#[test]
fn horizontal_mouse_wheel_clamps_before_applying_left_delta() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_global_mounts_scroll, u16::MAX);

    let global_mounts: Vec<MountConfig> = config
        .list_mount_rows()
        .into_iter()
        .filter(|row| row.scope.is_none())
        .map(|row| row.mount)
        .collect();
    let global_area = Rect {
        x: 30,
        y: 10,
        width: 70,
        height: 5,
    };
    let expected_max = max_scroll_offset(
        global_mounts_content_width(global_mounts.as_slice()),
        crate::tui::layout::scroll_viewport_width(global_area),
    );

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollLeft, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_global_mounts_scroll.offset_x(),
        expected_max.saturating_sub(MOUSE_HORIZONTAL_SCROLL_STEP),
        "left-scroll must first clamp stale resize/overscroll state, then move left"
    );
}

#[test]
fn editor_mounts_tab_horizontal_wheel_requires_mounts_tab() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![MountConfig {
            src: "/host/source/with/a/very/long/path/that/forces/editor/mount/scrolling".into(),
            dst: "/container/destination/with/a/very/long/path/that/forces/editor/mount/scrolling"
                .into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("editor stage expected");
    };
    assert!(editor.workspace_mounts_scroll_focused());
    assert_eq!(
        editor.workspace_mounts_scroll.offset_x(),
        MOUSE_HORIZONTAL_SCROLL_STEP
    );

    editor.active_tab = EditorTab::General;
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(!editor.workspace_mounts_scroll_focused());
    assert_eq!(
        editor.workspace_mounts_scroll.offset_x(),
        MOUSE_HORIZONTAL_SCROLL_STEP
    );
}
