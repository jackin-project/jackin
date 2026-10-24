// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crossterm::event::KeyCode;
use ratatui::layout::{Position, Rect};
use termrock::interaction::HitRegion;

fn editor_manager(active_tab: EditorTab) -> ManagerState<'static> {
    let mut state = list_state();
    let workspace = WorkspaceConfig {
        workdir: "/workspace".into(),
        mounts: vec![],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("demo".into(), workspace);
    editor.active_tab = active_tab;
    state.stage = ManagerStage::Editor(editor);
    state
}

fn settings_manager(active_tab: SettingsTab) -> ManagerState<'static> {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = active_tab;
    state.stage = ManagerStage::Settings(settings);
    state
}

fn tab_id_at(regions: &[HitRegion<usize>], column: u16, row: u16) -> Option<usize> {
    let point = Position::new(column, row);
    regions
        .iter()
        .find(|region| region.area.contains(point))
        .map(|region| region.id)
}

fn point_in_region(regions: &[HitRegion<usize>], id: usize) -> Option<Position> {
    regions
        .iter()
        .find(|region| region.id == id)
        .map(|region| Position::new(region.area.x, region.area.y))
}

fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    inner.is_empty()
        || (!outer.is_empty()
            && inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom())
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && left.x < right.right()
        && right.x < left.right()
        && left.y < right.bottom()
        && right.y < left.bottom()
}

#[test]
fn tab_select_and_overflow_triggers_are_inert() {
    let labels = [
        ("A very long General tab label", true),
        ("Mounts", false),
        ("Authentication", false),
    ];

    for width in [0, 1, 17, 18, 41, 42, 64] {
        let area = Rect::new(7, 9, width, 2);
        let tabs = labels
            .iter()
            .enumerate()
            .map(|(id, (label, active))| termrock::widgets::Tab::new(id, label).active(*active))
            .collect::<Vec<_>>();
        let system = termrock::style::DesignSystem::default();
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        let mut state = termrock::widgets::TabsState::default();
        state.selected = Some(0);
        state.focused = true;
        termrock::widgets::Tabs::new(&tabs, &system)
            .gap(termrock::widgets::TAB_GAP)
            .paint(area, &mut buffer, &mut state);

        let regions =
            crate::tui::components::editor_rows::tab_strip_hit_regions(area, &labels, true, None);
        assert!(
            regions
                .iter()
                .all(|region| rect_contains_rect(area, region.area))
        );

        if state.presentation() == termrock::widgets::TabsPresentation::Select {
            assert_eq!(state.overflow_trigger, Some(area));
            assert!(
                regions.is_empty(),
                "Select has no host action at width {width}"
            );
        }

        if let Some(trigger) = state.overflow_trigger {
            for row in trigger.y..trigger.bottom() {
                for column in trigger.x..trigger.right() {
                    assert_eq!(
                        tab_id_at(&regions, column, row),
                        None,
                        "trigger cell ({column}, {row}) is inert at width {width}"
                    );
                }
            }
        }

        if state.presentation() != termrock::widgets::TabsPresentation::Select
            && state.presentation() != termrock::widgets::TabsPresentation::Expanded
            && !area.is_empty()
        {
            assert!(
                regions.iter().any(|region| !region.area.is_empty()),
                "visible overflow/scrolling tabs remain targets at width {width}"
            );
        }
    }

    let config = jackin_config::AppConfig::default();
    for width in [17, 18, 41, 42] {
        let term_size = Rect::new(0, 0, width, 30);
        let mut state = editor_manager(EditorTab::General);
        let ManagerStage::Editor(editor) = &state.stage else {
            unreachable!();
        };
        let regions = editor_tab_hit_regions(editor, term_size, Some(&config), state.op_available);
        let areas = crate::tui::screens::editor::view::resolve_editor_frame(
            term_size,
            editor,
            &config,
            |editor, config, body| {
                crate::tui::components::footer_hints::editor_footer_items(
                    editor,
                    config,
                    state.op_available,
                    body,
                )
            },
        )
        .0;
        let labels = crate::tui::screens::editor::view::tab_labels(editor.active_tab);
        let mut widget_state = termrock::widgets::TabsState::default();
        widget_state.selected = labels.iter().position(|(_, active)| *active);
        widget_state.hovered = editor.hovered_tab();
        widget_state.focused = editor.tab_bar_focused();
        let tabs = labels
            .iter()
            .enumerate()
            .map(|(id, (label, active))| termrock::widgets::Tab::new(id, label).active(*active))
            .collect::<Vec<_>>();
        let mut buffer = ratatui::buffer::Buffer::empty(areas.tabs);
        termrock::widgets::Tabs::new(&tabs, &termrock::style::DesignSystem::default())
            .gap(termrock::widgets::TAB_GAP)
            .paint(areas.tabs, &mut buffer, &mut widget_state);

        if let Some(trigger) = widget_state.overflow_trigger {
            let original_tab = editor.active_tab;
            let mut trigger_point = None;
            for row in trigger.y..trigger.bottom() {
                for column in trigger.x..trigger.right() {
                    let mouse = mouse_down_at(column, row);
                    assert!(!clickable_at(&state, mouse, term_size, Some(&config)));
                    trigger_point.get_or_insert((column, row));
                }
            }
            if let Some((column, row)) = trigger_point {
                handle_mouse_with_config(
                    &mut state,
                    mouse_kind_at(MouseEventKind::Moved, column, row),
                    term_size,
                    Some(&config),
                );
                let ManagerStage::Editor(editor) = &state.stage else {
                    unreachable!();
                };
                assert_eq!(editor.hover_target, None);
                handle_mouse_with_config(
                    &mut state,
                    mouse_down_at(column, row),
                    term_size,
                    Some(&config),
                );
                let ManagerStage::Editor(editor) = &state.stage else {
                    unreachable!();
                };
                assert_eq!(editor.active_tab, original_tab);
            }

            if widget_state.presentation() == termrock::widgets::TabsPresentation::Select {
                assert!(
                    regions.is_empty(),
                    "Select has no host tab target at width {width}"
                );
            } else {
                assert!(
                    regions.iter().any(|region| {
                        !region.area.is_empty() && !rects_intersect(trigger, region.area)
                    }),
                    "visible regions outside the trigger survive at width {width}"
                );
            }
        }
    }
}

fn assert_tab_fit_boundary(labels: &[(&str, bool)]) {
    let fit_tabs = labels
        .iter()
        .enumerate()
        .map(|(id, (label, active))| termrock::widgets::Tab::new(id, label).active(*active))
        .collect::<Vec<_>>();
    for (width, presentation, visible_ids) in [
        (
            24,
            termrock::widgets::TabsPresentation::Overflow,
            vec![0, 1],
        ),
        (
            25,
            termrock::widgets::TabsPresentation::Expanded,
            vec![0, 1, 2],
        ),
    ] {
        let area = Rect::new(5, 8, width, 2);
        let mut widget_state = termrock::widgets::TabsState::default();
        widget_state.selected = Some(0);
        widget_state.hovered = None;
        widget_state.focused = false;
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        termrock::widgets::Tabs::new(&fit_tabs, &termrock::style::DesignSystem::default())
            .gap(termrock::widgets::TAB_GAP)
            .paint(area, &mut buffer, &mut widget_state);
        assert_eq!(widget_state.presentation(), presentation, "width {width}");
        let painted = widget_state
            .regions
            .iter()
            .map(|region| region.id)
            .collect::<Vec<_>>();
        assert_eq!(painted, visible_ids, "painted IDs at width {width}");

        let projected =
            crate::tui::components::editor_rows::tab_strip_hit_regions(area, labels, false, None);
        assert_eq!(
            projected.iter().map(|region| region.id).collect::<Vec<_>>(),
            visible_ids,
            "projected IDs at width {width}"
        );
        assert_eq!(
            projected
                .iter()
                .map(|region| (region.id, region.area))
                .collect::<Vec<_>>(),
            widget_state
                .regions
                .iter()
                .map(|region| (region.id, region.area))
                .collect::<Vec<_>>(),
            "projected areas match the independent widget paint at width {width}"
        );
    }
}

#[test]
fn tab_regions_obey_nonzero_area_fit_and_tiny_bounds() {
    let labels = [("General", true), ("Mounts", false), ("Auth", false)];
    let old_wide_strip = Rect::new(0, SCREEN_HEADER_HEIGHT, 120, TAB_STRIP_HEIGHT);
    let wide_regions = crate::tui::components::editor_rows::tab_strip_hit_regions(
        old_wide_strip,
        &labels,
        false,
        None,
    );

    // These are the retained wide-frame mappings from both removed helpers:
    // a normal row and the Tabs widget's emitted second row remain usable.
    assert_eq!(tab_id_at(&wide_regions, 1, SCREEN_HEADER_HEIGHT), Some(0));
    assert_eq!(
        tab_id_at(&wide_regions, 11, SCREEN_HEADER_HEIGHT + 1),
        Some(1)
    );
    assert_eq!(tab_id_at(&wide_regions, 1, SCREEN_HEADER_HEIGHT - 1), None);
    assert_eq!(
        tab_id_at(&wide_regions, 1, SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT),
        None
    );
    for row in [SCREEN_HEADER_HEIGHT, SCREEN_HEADER_HEIGHT + 1] {
        assert_eq!(
            tab_id_at(&wide_regions, 9, row),
            None,
            "the inter-tab gap at column 9 has no target on row {row}"
        );
    }

    assert_tab_fit_boundary(&labels);

    let translated = Rect::new(7, 11, 80, 2);
    let translated_regions = crate::tui::components::editor_rows::tab_strip_hit_regions(
        translated,
        &labels,
        true,
        Some(1),
    );
    assert!(
        translated_regions
            .iter()
            .all(|region| rect_contains_rect(translated, region.area))
    );
    let first = point_in_region(&translated_regions, 0).expect("first visible tab");
    let second = point_in_region(&translated_regions, 1).expect("second visible tab");
    assert_eq!(tab_id_at(&translated_regions, first.x, first.y), Some(0));
    assert_eq!(tab_id_at(&translated_regions, second.x, second.y), Some(1));
    assert_eq!(
        tab_id_at(&translated_regions, translated.x - 1, translated.y),
        None
    );
    assert_eq!(
        tab_id_at(&translated_regions, translated.right(), translated.y),
        None
    );
    assert_eq!(
        tab_id_at(&translated_regions, translated.x, translated.y - 1),
        None
    );
    assert_eq!(
        tab_id_at(&translated_regions, translated.x, translated.bottom()),
        None
    );

    for area in [
        Rect::new(5, 8, 0, 2),
        Rect::new(5, 8, 20, 0),
        Rect::new(5, 8, 1, 1),
        Rect::new(5, 8, 7, 2),
        Rect::new(5, 8, 10, 2),
        Rect::new(5, 8, 17, 2),
        Rect::new(5, 8, 18, 2),
        Rect::new(5, 8, 41, 2),
        Rect::new(5, 8, 42, 2),
    ] {
        let regions =
            crate::tui::components::editor_rows::tab_strip_hit_regions(area, &labels, false, None);
        assert!(
            regions
                .iter()
                .all(|region| rect_contains_rect(area, region.area))
        );
        if area.is_empty() {
            assert!(regions.is_empty(), "empty tab area {area:?}");
        }
    }

    let config = jackin_config::AppConfig::default();
    let term_size = term_120x40();
    let mut editor = editor_manager(EditorTab::Auth);
    for row in [SCREEN_HEADER_HEIGHT, SCREEN_HEADER_HEIGHT + 1] {
        assert!(!clickable_at(
            &editor,
            mouse_down_at(9, row),
            term_size,
            Some(&config)
        ));
        handle_mouse_with_config(
            &mut editor,
            mouse_kind_at(MouseEventKind::Moved, 9, row),
            term_size,
            Some(&config),
        );
        let ManagerStage::Editor(editor_state) = &editor.stage else {
            unreachable!();
        };
        assert_eq!(editor_state.hover_target, None, "Editor gap row {row}");
        handle_mouse_with_config(&mut editor, mouse_down_at(9, row), term_size, Some(&config));
        let ManagerStage::Editor(editor_state) = &editor.stage else {
            unreachable!();
        };
        assert_eq!(
            editor_state.active_tab,
            EditorTab::Auth,
            "Editor gap row {row}"
        );
    }

    let mut settings = settings_manager(SettingsTab::Trust);
    for row in [SCREEN_HEADER_HEIGHT, SCREEN_HEADER_HEIGHT + 1] {
        assert!(!clickable_at(
            &settings,
            mouse_down_at(9, row),
            term_size,
            Some(&config)
        ));
        handle_mouse_with_config(
            &mut settings,
            mouse_kind_at(MouseEventKind::Moved, 9, row),
            term_size,
            Some(&config),
        );
        let ManagerStage::Settings(settings_state) = &settings.stage else {
            unreachable!();
        };
        assert_eq!(settings_state.hover_target, None, "Settings gap row {row}");
        handle_mouse_with_config(
            &mut settings,
            mouse_down_at(9, row),
            term_size,
            Some(&config),
        );
        let ManagerStage::Settings(settings_state) = &settings.stage else {
            unreachable!();
        };
        assert_eq!(
            settings_state.active_tab,
            SettingsTab::Trust,
            "Settings gap row {row}"
        );
    }
}

#[test]
fn editor_tabs_selection_hover_and_clickability_share_painted_cells() {
    let config = jackin_config::AppConfig::default();
    let term_size = term_120x40();
    let mut state = editor_manager(EditorTab::Auth);
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    let regions = editor_tab_hit_regions(editor, term_size, Some(&config), state.op_available);

    // Retains both old coordinate-helper mappings as current painted targets.
    assert_eq!(tab_id_at(&regions, 1, SCREEN_HEADER_HEIGHT), Some(0));
    assert_eq!(tab_id_at(&regions, 11, SCREEN_HEADER_HEIGHT), Some(1));
    assert_eq!(tab_id_at(&regions, 1, SCREEN_HEADER_HEIGHT - 1), None);
    assert!(clickable_at(
        &state,
        mouse_down_at(1, 3),
        term_size,
        Some(&config)
    ));
    assert!(clickable_at(
        &state,
        mouse_down_at(11, 3),
        term_size,
        Some(&config)
    ));
    assert!(!clickable_at(
        &state,
        mouse_down_at(1, 2),
        term_size,
        Some(&config)
    ));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 1, 3),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(0)));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 3),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(1)));
    assert!(clickable_at(
        &state,
        mouse_down_at(11, 4),
        term_size,
        Some(&config)
    ));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 4),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(1)));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 1, 2),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, None);

    handle_mouse_with_config(&mut state, mouse_down_at(1, 3), term_size, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.active_tab, EditorTab::General);
    assert!(editor.tab_bar_focused());
    handle_mouse_with_config(&mut state, mouse_down_at(11, 4), term_size, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.active_tab, EditorTab::Mounts);
    assert!(editor.tab_bar_focused());

    let visible_ids = regions.iter().map(|region| region.id).collect::<Vec<_>>();
    assert_eq!(visible_ids, (0..EditorTab::ALL.len()).collect::<Vec<_>>());
    for (id, expected_tab) in EditorTab::ALL.into_iter().enumerate() {
        let point = point_in_region(&regions, id).expect("every Editor tab is visible");
        assert_eq!(tab_id_at(&regions, point.x, point.y), Some(id));
        assert!(clickable_at(
            &state,
            mouse_down_at(point.x, point.y),
            term_size,
            Some(&config)
        ));
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::Moved, point.x, point.y),
            term_size,
            Some(&config),
        );
        let ManagerStage::Editor(editor) = &state.stage else {
            unreachable!();
        };
        assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(id)));
        handle_mouse_with_config(
            &mut state,
            mouse_down_at(point.x, point.y),
            term_size,
            Some(&config),
        );
        let ManagerStage::Editor(editor) = &state.stage else {
            unreachable!();
        };
        assert_eq!(editor.active_tab, expected_tab);
        assert!(editor.tab_bar_focused());
    }

    let mut no_config = editor_manager(EditorTab::General);
    assert!(!clickable_at(
        &no_config,
        mouse_down_at(11, 3),
        term_size,
        None
    ));
    handle_mouse_with_config(
        &mut no_config,
        mouse_kind_at(MouseEventKind::Moved, 1, 3),
        term_size,
        None,
    );
    let ManagerStage::Editor(editor) = &no_config.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, None);
    handle_mouse_with_config(&mut no_config, mouse_down_at(11, 3), term_size, None);
    let ManagerStage::Editor(editor) = &no_config.stage else {
        unreachable!();
    };
    assert_eq!(editor.active_tab, EditorTab::General);
}

#[test]
fn settings_tabs_selection_hover_and_clickability_share_painted_cells() {
    let config = jackin_config::AppConfig::default();
    let term_size = term_120x40();
    let mut state = settings_manager(SettingsTab::Trust);
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    let regions = settings_tab_hit_regions(settings, term_size, state.op_available);

    // Retains both old Settings coordinate-helper mappings through real paint.
    assert_eq!(tab_id_at(&regions, 1, SCREEN_HEADER_HEIGHT), Some(0));
    assert_eq!(tab_id_at(&regions, 11, SCREEN_HEADER_HEIGHT), Some(1));
    assert_eq!(tab_id_at(&regions, 1, SCREEN_HEADER_HEIGHT - 1), None);
    assert!(clickable_at(
        &state,
        mouse_down_at(1, 3),
        term_size,
        Some(&config)
    ));
    assert!(clickable_at(
        &state,
        mouse_down_at(11, 3),
        term_size,
        Some(&config)
    ));
    assert!(!clickable_at(
        &state,
        mouse_down_at(1, 2),
        term_size,
        Some(&config)
    ));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 1, 3),
        term_size,
        Some(&config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.hover_target, Some(SettingsHoverTarget::Tab(0)));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 3),
        term_size,
        Some(&config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.hover_target, Some(SettingsHoverTarget::Tab(1)));
    assert!(clickable_at(
        &state,
        mouse_down_at(11, 4),
        term_size,
        Some(&config)
    ));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 4),
        term_size,
        Some(&config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.hover_target, Some(SettingsHoverTarget::Tab(1)));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 1, 2),
        term_size,
        Some(&config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.hover_target, None);

    handle_mouse_with_config(&mut state, mouse_down_at(1, 3), term_size, Some(&config));
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.active_tab, SettingsTab::General);
    assert!(settings.tab_bar_focused());
    handle_mouse_with_config(&mut state, mouse_down_at(11, 4), term_size, Some(&config));
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(settings.active_tab, SettingsTab::Mounts);
    assert!(settings.tab_bar_focused());

    let visible_ids = regions.iter().map(|region| region.id).collect::<Vec<_>>();
    assert_eq!(visible_ids, (0..SettingsTab::ALL.len()).collect::<Vec<_>>());
    for (id, expected_tab) in SettingsTab::ALL.into_iter().enumerate() {
        let point = point_in_region(&regions, id).expect("every Settings tab is visible");
        assert_eq!(tab_id_at(&regions, point.x, point.y), Some(id));
        assert!(clickable_at(
            &state,
            mouse_down_at(point.x, point.y),
            term_size,
            Some(&config)
        ));
        handle_mouse_with_config(
            &mut state,
            mouse_kind_at(MouseEventKind::Moved, point.x, point.y),
            term_size,
            Some(&config),
        );
        let ManagerStage::Settings(settings) = &state.stage else {
            unreachable!();
        };
        assert_eq!(settings.hover_target, Some(SettingsHoverTarget::Tab(id)));
        handle_mouse_with_config(
            &mut state,
            mouse_down_at(point.x, point.y),
            term_size,
            Some(&config),
        );
        let ManagerStage::Settings(settings) = &state.stage else {
            unreachable!();
        };
        assert_eq!(settings.active_tab, expected_tab);
        assert!(settings.tab_bar_focused());
    }
}

#[test]
fn tab_regions_resize_and_footer_follow_current_paint() {
    let config = jackin_config::AppConfig::default();
    let editor_state = editor_manager(EditorTab::General);
    let settings_state = settings_manager(SettingsTab::General);

    for (width, height) in [(0, 0), (1, 1), (17, 4), (80, 8), (80, 10), (120, 30)] {
        let area = Rect::new(0, 0, width, height);
        for op_available in [false, true] {
            let ManagerStage::Editor(editor) = &editor_state.stage else {
                unreachable!();
            };
            let (editor_areas, _) = crate::tui::screens::editor::view::resolve_editor_frame(
                area,
                editor,
                &config,
                |editor, config, body| {
                    crate::tui::components::footer_hints::editor_footer_items(
                        editor,
                        config,
                        op_available,
                        body,
                    )
                },
            );
            let editor_regions = editor_tab_hit_regions(editor, area, Some(&config), op_available);
            assert!(
                editor_regions
                    .iter()
                    .all(|region| rect_contains_rect(editor_areas.tabs, region.area))
            );

            let ManagerStage::Settings(settings) = &settings_state.stage else {
                unreachable!();
            };
            let (settings_areas, _) = crate::tui::screens::settings::view::resolve_settings_frame(
                area,
                settings,
                |settings, body| {
                    crate::tui::screens::settings::view::settings_screen_footer_for_state(
                        settings,
                        op_available,
                        body,
                    )
                },
            );
            let settings_regions = settings_tab_hit_regions(settings, area, op_available);
            assert!(
                settings_regions
                    .iter()
                    .all(|region| rect_contains_rect(settings_areas.tabs, region.area))
            );
        }
    }

    let wide = term_120x40();
    let narrow = Rect::new(0, 0, 17, 40);
    let mut state = editor_manager(EditorTab::General);
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 3),
        wide,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(1)));
    assert!(!clickable_at(
        &state,
        mouse_down_at(11, 3),
        narrow,
        Some(&config)
    ));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 3),
        narrow,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, None);
    handle_mouse_with_config(&mut state, mouse_down_at(11, 3), narrow, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.active_tab, EditorTab::General);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 11, 3),
        wide,
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(1)));
    handle_mouse_with_config(&mut state, mouse_down_at(11, 3), wide, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        unreachable!();
    };
    assert_eq!(editor.active_tab, EditorTab::Mounts);

    // Keep the Settings frame variable used above tied to its unchanged model.
    let ManagerStage::Settings(settings) = &settings_state.stage else {
        unreachable!();
    };
    assert_eq!(settings.active_tab, SettingsTab::General);
}

#[test]
fn modal_owners_block_painted_tab_targets() {
    let config = jackin_config::AppConfig::default();
    let term_size = term_120x40();

    let mut editor = editor_manager(EditorTab::Auth);
    let editor_target = {
        let ManagerStage::Editor(editor_state) = &editor.stage else {
            unreachable!();
        };
        let regions =
            editor_tab_hit_regions(editor_state, term_size, Some(&config), editor.op_available);
        point_in_region(&regions, 0).expect("General tab is visible")
    };
    assert!(clickable_at(
        &editor,
        mouse_down_at(editor_target.x, editor_target.y),
        term_size,
        Some(&config)
    ));
    handle_mouse_with_config(
        &mut editor,
        mouse_kind_at(MouseEventKind::Moved, editor_target.x, editor_target.y),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor_state) = &editor.stage else {
        unreachable!();
    };
    assert_eq!(editor_state.hover_target, Some(EditorHoverTarget::Tab(0)));
    assert_ne!(editor_state.active_tab, EditorTab::General);
    let ManagerStage::Editor(editor_state) = &mut editor.stage else {
        unreachable!();
    };
    editor_state.modal = Some(Modal::ErrorPopup {
        state: crate::tui::components::ErrorPopupState::new("Editor error", "modal owner"),
    });
    let ManagerStage::Editor(editor_state) = &editor.stage else {
        unreachable!();
    };
    assert_eq!(editor_state.hover_target, Some(EditorHoverTarget::Tab(0)));
    assert!(!clickable_at(
        &editor,
        mouse_down_at(editor_target.x, editor_target.y),
        term_size,
        Some(&config)
    ));
    handle_mouse_with_config(
        &mut editor,
        mouse_kind_at(MouseEventKind::Moved, editor_target.x, editor_target.y),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor_state) = &editor.stage else {
        unreachable!();
    };
    assert_eq!(editor_state.hover_target, None);
    handle_mouse_with_config(
        &mut editor,
        mouse_down_at(editor_target.x, editor_target.y),
        term_size,
        Some(&config),
    );
    let ManagerStage::Editor(editor_state) = &editor.stage else {
        unreachable!();
    };
    assert_eq!(editor_state.active_tab, EditorTab::Auth);

    for owner in ["error", "mounts", "env", "auth", "help", "status"] {
        assert_settings_modal_blocks_painted_tabs(owner, &config, term_size);
    }
}

fn assert_settings_modal_blocks_painted_tabs(
    owner: &str,
    config: &jackin_config::AppConfig,
    term_size: Rect,
) {
    let active_tab = match owner {
        "mounts" => SettingsTab::Mounts,
        "env" => SettingsTab::Environments,
        "auth" => SettingsTab::Auth,
        _ => SettingsTab::General,
    };
    let mut state = settings_manager(active_tab);
    let target_tab_id = usize::from(active_tab == SettingsTab::General);
    let target = {
        let ManagerStage::Settings(settings) = &state.stage else {
            unreachable!();
        };
        let regions = settings_tab_hit_regions(settings, term_size, state.op_available);
        point_in_region(&regions, target_tab_id).expect("different Settings tab is visible")
    };
    assert_ne!(SettingsTab::ALL[target_tab_id], active_tab);
    assert!(clickable_at(
        &state,
        mouse_down_at(target.x, target.y),
        term_size,
        Some(config)
    ));
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, target.x, target.y),
        term_size,
        Some(config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::Tab(target_tab_id)),
        "{owner} stale-hover precondition"
    );
    let auth_root = (owner == "auth").then(|| tempfile::tempdir().expect("auth picker root"));
    match owner {
        "error" => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                unreachable!();
            };
            settings.error_popup = Some(crate::tui::components::ErrorPopupState::new(
                "Settings error",
                "error popup owns pointer input",
            ));
        }
        "mounts" => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                unreachable!();
            };
            settings.mounts.modals.open(SettingsModal::MountConfirm {
                action: GlobalMountConfirm::Remove,
                state: crate::tui::components::ConfirmState::new("Remove mount?"),
            });
        }
        "env" => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                unreachable!();
            };
            settings.env.modals.open(SettingsModal::EnvRolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("ops").unwrap(),
                ]),
            });
        }
        "auth" => {
            let browser =
                file_browser_with_dirs(auth_root.as_ref().expect("auth picker root").path(), 1);
            let ManagerStage::Settings(settings) = &mut state.stage else {
                unreachable!();
            };
            settings
                .auth
                .modals
                .open(SettingsModal::AuthSourceFolderPicker { state: browser });
        }
        "help" => {
            state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
        }
        "status" => {
            state.status_overlay = Some(crate::tui::components::StatusPopupState::new(
                "Working",
                "status owns pointer input",
            ));
        }
        _ => unreachable!(),
    }

    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::Tab(target_tab_id)),
        "{owner} retains the real pre-owner hover until pointer refresh"
    );
    assert!(
        !clickable_at(
            &state,
            mouse_down_at(target.x, target.y),
            term_size,
            Some(config)
        ),
        "{owner} owner blocks a different painted tab"
    );
    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, target.x, target.y),
        term_size,
        Some(config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(
        settings.hover_target, None,
        "{owner} clears stale tab hover"
    );
    handle_mouse_with_config(
        &mut state,
        mouse_down_at(target.x, target.y),
        term_size,
        Some(config),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        unreachable!();
    };
    assert_eq!(
        settings.active_tab, active_tab,
        "{owner} preserves selection"
    );
    assert!(
        state.drain_effects().is_empty(),
        "{owner} queues no tab effect"
    );
}

#[test]
fn tab_keyboard_navigation_reaches_hidden_tabs() {
    let config = jackin_config::AppConfig::default();
    let temp = tempfile::tempdir().expect("navigation tempdir");
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let mut editor = editor_manager(EditorTab::General);
    editor.cached_term_size = Rect::new(0, 0, 17, 30);
    let ManagerStage::Editor(editor_state) = &editor.stage else {
        unreachable!();
    };
    assert!(
        editor_tab_hit_regions(
            editor_state,
            editor.cached_term_size,
            Some(&config),
            editor.op_available,
        )
        .is_empty()
    );

    for expected in [
        EditorTab::Mounts,
        EditorTab::Roles,
        EditorTab::Secrets,
        EditorTab::Auth,
        EditorTab::General,
    ] {
        crate::tui::input::editor::handle_editor_key(
            &mut editor,
            &config,
            &paths,
            temp.path(),
            key(KeyCode::Right),
        )
        .unwrap();
        let ManagerStage::Editor(editor_state) = &editor.stage else {
            unreachable!();
        };
        assert_eq!(editor_state.active_tab, expected);
    }

    let mut settings = settings_manager(SettingsTab::General);
    settings.cached_term_size = Rect::new(0, 0, 17, 30);
    let ManagerStage::Settings(settings_state) = &settings.stage else {
        unreachable!();
    };
    assert!(
        settings_tab_hit_regions(
            settings_state,
            settings.cached_term_size,
            settings.op_available,
        )
        .is_empty()
    );
    for expected in [
        SettingsTab::Mounts,
        SettingsTab::Environments,
        SettingsTab::Auth,
        SettingsTab::Trust,
        SettingsTab::General,
    ] {
        crate::tui::input::global_mounts::handle_settings_key(&mut settings, key(KeyCode::Right));
        let ManagerStage::Settings(settings_state) = &settings.stage else {
            unreachable!();
        };
        assert_eq!(settings_state.active_tab, expected);
    }
}
