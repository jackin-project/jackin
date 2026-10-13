// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn settings_vertical_scrollbar_drag_ignores_background_when_modal_open() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Mounts;
    settings.mounts.pending = (0..20)
        .map(|idx| jackin_config::GlobalMountRow {
            scope: None,
            name: format!("mount-{idx}"),
            mount: MountConfig {
                src: format!("/host/{idx}"),
                dst: format!("/home/agent/{idx}"),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        })
        .collect();
    settings.mounts.modals.open(SettingsModal::MountConfirm {
        action: GlobalMountConfirm::Save,
        state: global_mount_confirm_state(GlobalMountConfirm::Save),
    });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), 99, 7),
        term(100),
        None,
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(settings.mounts.scroll.offset_y(), 0);
}

#[test]
fn editor_mounts_tab_click_full_row_width_selects_mount_and_focuses_block() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![
            MountConfig {
                src: "/host/one".into(),
                dst: "/host/one".into(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
            MountConfig {
                src: "/host/two".into(),
                dst: "/host/two".into(),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    // Mounts editor body begins at y=5. Interior row y=6 is the
    // header, y=7 is mount 0, y=8 is mount 1. Click far to the
    // right in whitespace on mount 1's row, not on the path text.
    handle_mouse_with_config(&mut state, mouse_at(95, 8), term(100), None);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(matches!(editor.active_field, FieldFocus::Row(1)));
    assert!(editor.workspace_mounts_scroll_focused());
}

#[test]
fn editor_mounts_tab_click_host_source_continuation_selects_parent_and_focuses_block() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![MountConfig {
            src: "/host/source".into(),
            dst: "/container/destination".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.active_field = FieldFocus::Row(editor.pending.mounts.len());
    state.stage = ManagerStage::Editor(editor);

    // y=8 is the host-source continuation line for the first mount.
    handle_mouse_with_config(&mut state, mouse_at(95, 8), term(100), None);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(matches!(editor.active_field, FieldFocus::Row(0)));
    assert!(editor.workspace_mounts_scroll_focused());
}

#[test]
fn scroll_up_decrements_vertical_scroll_offset() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_global_mounts_scroll, 3);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(state.list_global_mounts_scroll.offset_y(), 0);
}

#[test]
fn clicking_editor_content_area_clears_tab_bar_focus() {
    // Defect 17: clicking the content block must transfer interaction focus
    // into it — same end state as Tab/↓ — regardless of whether it overflows.
    let mut state = list_state();
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(true); // tab bar owns focus before the click
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 10;
    state.stage = ManagerStage::Editor(editor);

    // Click somewhere in the content area (rows 5–14 on a term(42) at SCREEN_HEADER_HEIGHT=2,
    // TAB_STRIP_HEIGHT=2 → content starts at row 4).
    handle_mouse_with_config(&mut state, mouse_at(10, 6), term(42), None);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    // After clicking the content block, tab_bar_focused must be false.
    assert!(
        !editor.tab_bar_focused(),
        "clicking content must clear tab_bar_focused (Defect 17)"
    );
    assert!(
        editor.tab_content_scroll_focused(),
        "clicking content must set tab_content_scroll_focused"
    );
}

#[test]
fn wheel_shift_fallback_retries_vertical_at_horizontal_edge() {
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
    let areas = list_scroll_areas(&state, term(100), Some(&config)).expect("list areas");
    // Pre-set the horizontal offset at the block's real maximum so the
    // Shift+wheel horizontal application is `Ignored` and the consumer retry
    // must fire the vertical fallback on the SAME block (matrix row 3).
    let global_mounts: Vec<MountConfig> = config
        .list_mount_rows()
        .into_iter()
        .filter(|row| row.scope.is_none())
        .map(|row| row.mount)
        .collect();
    let max_x = max_scroll_offset(
        global_mounts_content_width(global_mounts.as_slice()),
        crate::tui::layout::scroll_viewport_width(areas.global.area),
    );
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_global_mounts_scroll, max_x);

    let shift_down = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: areas.global.area.x + 1,
        row: areas.global.area.y + 1,
        modifiers: KeyModifiers::SHIFT,
    };
    handle_mouse_with_config(&mut state, shift_down, term(100), Some(&config));

    assert_eq!(
        state.list_global_mounts_scroll.offset_x(),
        max_x,
        "horizontal offset pinned at max must not move"
    );
    assert_eq!(
        state.list_global_mounts_scroll.offset_y(),
        1,
        "Shift+wheel at the horizontal edge retries vertical on the same block"
    );
}

#[test]
fn scroll_block_registry_hit_test_prefers_later_registration() {
    let area = Rect {
        x: 10,
        y: 5,
        width: 20,
        height: 6,
    };
    let blocks = [
        ScrollBlockRegion {
            id: ConsoleScrollBlock::EditorTabContent,
            rect: area,
            content_w: 40,
            content_h: 20,
        },
        ScrollBlockRegion {
            id: ConsoleScrollBlock::EditorWorkspaceMounts,
            rect: area,
            content_w: 30,
            content_h: 20,
        },
    ];
    assert_eq!(
        hit(&blocks, 15, 6),
        Some(ConsoleScrollBlock::EditorWorkspaceMounts),
        "the later-registered (paint-topmost) block must win an overlap"
    );
    assert_eq!(hit(&blocks, 0, 0), None);
}

#[test]
fn click_non_row_trust_block_area_deselects_via_sentinel() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Trust;
    settings.trust.pending = vec![SettingsTrustRow {
        role: "agent-smith".into(),
        git: "/repo".into(),
        trusted: true,
    }];
    settings.trust.selected = 0;
    let content = settings.content_area(term(100));
    state.stage = ManagerStage::Settings(settings);

    handle_mouse(
        &mut state,
        mouse_kind_at(
            MouseEventKind::Down(MouseButton::Left),
            content.x + 1,
            content.y + 3,
        ),
        term(100),
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    // The lane dispatches SelectSettingsTrustRow(usize::MAX), which the plan
    // maps to `selected: None` (covered by
    // settings_trust_row_select_plan_bounds_checks_and_focuses_content) —
    // selection is left unchanged while the block takes content focus.
    assert_eq!(settings.trust.selected, 0);
    assert_eq!(
        settings.focus_owner(),
        crate::tui::focus::ConsoleFocusTarget::Content(SettingsTab::Trust),
        "click on non-row Trust-block area must still route through the trust lane"
    );
}

#[test]
fn hover_regions_topmost_first_matches_hit_test() {
    // Row-15 convention lock: hover regions are built topmost-FIRST
    // (reverse of paint order) because `HoverState::update` keeps the
    // FIRST hit in the slice while scene `hit_test` keeps the LAST
    // registered — topmost-first makes both pick the same target.
    use termrock::interaction::{HitRegion, HoverState};

    let top = HitRegion {
        id: ConsoleHoverTarget::Workspace(ManagerHoverTarget::ListRow(
            ManagerListRow::NewWorkspace,
        )),
        area: Rect::new(0, 0, 10, 2),
    };
    let under = HitRegion {
        id: ConsoleHoverTarget::Editor(EditorHoverTarget::Tab(0)),
        area: Rect::new(0, 1, 10, 2),
    };
    let regions = [top.clone(), under];
    let mut hover = HoverState::default();

    // Overlap row: the first (topmost) region wins.
    assert_eq!(
        hover.update(ratatui::layout::Position::new(5, 1), &regions),
        Some(&top.id)
    );
    assert_eq!(hover.hovered(), Some(&top.id));
    // Off every region: the cache clears.
    assert_eq!(
        hover.update(ratatui::layout::Position::new(50, 50), &regions),
        None
    );
    assert_eq!(hover.hovered(), None);
}
