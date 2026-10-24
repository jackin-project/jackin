// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::screens::settings::update::settings_trust_row_at_position;

pub(super) fn painted_trust_label_position(
    settings: &SettingsState<'_>,
    terminal_area: Rect,
    label: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("Trust TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::screens::settings::view::render_settings_with_footer(
                frame,
                frame.area(),
                settings,
                false,
            );
        })
        .expect("Trust screen should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .find_map(|row| {
            let text = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(label)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                u16::try_from(column).expect("painted Trust label column fits u16"),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual Trust TestBackend buffer lacks {label:?}"))
}

fn translated_trust_tab_test_buffer(
    settings: &SettingsState<'_>,
    terminal_area: Rect,
    trust_area: Rect,
) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("translated Trust TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::screens::settings::view::render_trust_tab(frame, settings, trust_area);
        })
        .expect("Trust tab should render at translated TestBackend coordinates");
    terminal.backend().buffer().clone()
}

fn test_buffer_label_position(
    buffer: &ratatui::buffer::Buffer,
    label: &str,
) -> ratatui::layout::Position {
    (0..buffer.area.height)
        .find_map(|row| {
            let text = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(label)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                u16::try_from(column).expect("painted Trust label column fits u16"),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("translated Trust TestBackend buffer lacks {label:?}"))
}

#[test]
fn drag_ignored_on_non_list_stage() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    assert!(
        state.drag_state.is_none(),
        "Down on Editor stage must not drag",
    );
}

#[test]
fn drag_ignored_when_terminal_too_narrow() {
    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), 13),
        term(30),
    );
    assert!(state.drag_state.is_none());
}

#[test]
fn container_info_copy_click_queues_typed_effect() {
    let mut state = list_state();
    state.list_modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Debug info",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID", "run-123",
                )
                .copyable()
                .emphasised(),
            ],
        ),
    });
    let term = term_120x40();
    let mut hit = None;
    for y in 0..term.height {
        for x in 0..term.width {
            let mouse = mouse_down_at(x, y);
            if container_info_copyable_row_at(&state, mouse, term) {
                hit = Some(mouse);
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let hit = hit.expect("copyable container-info row should have a hitbox");

    handle_mouse(&mut state, hit, term);

    match state.drain_effects().as_slice() {
        [ManagerEffect::CopyContainerInfoValue { row, payload }] => {
            assert_eq!(*row, 0);
            assert_eq!(payload, "run-123");
        }
        other => panic!("expected CopyContainerInfoValue effect, got {other:?}"),
    }
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_ref() else {
        panic!("expected container-info modal");
    };
    assert_eq!(
        info.copied_row(),
        None,
        "mouse input must not mark copied before the effect executor writes OSC52"
    );
}

#[test]
fn mouse_down_on_editor_tab_selects_tab() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

    handle_mouse_with_config(&mut state, mouse_down_at(33, 3), term(100), Some(&config));

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::Secrets);
    assert!(matches!(editor.active_field, FieldFocus::Row(0)));
}

#[test]
fn mouse_motion_sets_and_clears_editor_tab_hover() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 22, 3),
        term(100),
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.hovered_tab(), Some(2));
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(2)));
    assert_eq!(editor.active_tab, EditorTab::General);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 22, 0),
        term(100),
        Some(&config),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.hovered_tab(), None);
    assert_eq!(editor.hover_target, None);
}

#[test]
fn mouse_motion_sets_and_clears_list_row_hover() {
    let mut state = list_state_with_saved(3);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 10, 4),
        term(100),
    );
    assert_eq!(
        state.hover_target,
        Some(ManagerHoverTarget::ListRow(ManagerListRow::SavedWorkspace(
            0
        )))
    );
    assert_eq!(
        state.hovered_list_row(),
        Some(ManagerListRow::SavedWorkspace(0))
    );

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, DEFAULT_SPLIT_PCT, 4),
        term(100),
    );
    assert_eq!(state.hover_target, None);
}

#[test]
fn mouse_motion_sets_and_clears_editor_mount_row_hover() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![MountConfig {
            src: "/host".into(),
            dst: "/home/agent/host".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 10, 7),
        term(100),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::MountRow(0)));
    assert_eq!(editor.hovered_mount_row(), Some(0));

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 10, 0),
        term(100),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.hover_target, None);
}

#[test]
fn mouse_motion_sets_and_clears_settings_trust_row_hover() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Trust;
    settings.trust.pending = vec![
        SettingsTrustRow {
            role: "trust-row-zero-unique".into(),
            git: "/repo/zero".into(),
            trusted: true,
        },
        SettingsTrustRow {
            role: "trust-row-one-unique".into(),
            git: "/repo/one".into(),
            trusted: false,
        },
    ];
    state.stage = ManagerStage::Settings(settings);
    let terminal_area = term_120x40();
    let row_position = {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        painted_trust_label_position(settings, terminal_area, "trust-row-zero-unique")
    };

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, row_position.x, row_position.y),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::TrustRow(0))
    );
    assert_eq!(settings.hovered_trust_row(), Some(0));

    let blank_content_column = settings
        .content_area(terminal_area)
        .right()
        .saturating_sub(2);
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, blank_content_column, row_position.y),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::TrustRow(0))
    );

    let area = settings.content_area(terminal_area);
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, area.x + 1, row_position.y),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target, None,
        "left padding is not a row target"
    );

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, row_position.x, area.y + 1),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target, None,
        "the visible Trust header is not a row"
    );

    let empty_position = {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        settings.trust.pending.clear();
        painted_trust_label_position(settings, terminal_area, "(none)")
    };
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, empty_position.x, empty_position.y),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.hover_target, None);
}

#[test]
fn settings_trust_hover_uses_painted_scroll_clamp_and_horizontal_offset() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Trust;
    settings.trust.pending = (0..20)
        .map(|index| SettingsTrustRow {
            role: format!("trust-row-{index:02}-unique"),
            git: format!("/repo/{index:02}"),
            trusted: index % 2 == 0,
        })
        .collect();
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.trust.scroll, u16::MAX);
    state.stage = ManagerStage::Settings(settings);
    let terminal_area = Rect::new(0, 0, 100, 14);
    crate::tui::view::prepare_for_render(
        &mut state,
        &config,
        std::path::Path::new("/"),
        terminal_area,
    );
    let (last_row_position, stored_scroll_before_hover) = {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        let stored_scroll_before_hover = settings.trust.scroll.offset_y();
        assert!(
            usize::from(stored_scroll_before_hover)
                > settings.trust.pending.len().saturating_add(1),
            "fixture should remain beyond the complete rendered Trust content"
        );
        (
            painted_trust_label_position(settings, terminal_area, "trust-row-19-unique"),
            stored_scroll_before_hover,
        )
    };

    handle_mouse(
        &mut state,
        mouse_kind_at(
            MouseEventKind::Moved,
            last_row_position.x,
            last_row_position.y,
        ),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::TrustRow(19))
    );
    assert_eq!(
        settings.trust.scroll.offset_y(),
        stored_scroll_before_hover,
        "hit testing uses paint's effective clamp without mutating stored scroll"
    );

    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Trust;
    settings.trust.pending = vec![
        SettingsTrustRow {
            role: "trust-horizontal-row-zero".into(),
            git: "/repo/zero".into(),
            trusted: true,
        },
        SettingsTrustRow {
            role: "trust-horizontal-row-one".into(),
            git: "/repo/one".into(),
            trusted: true,
        },
    ];
    crate::tui::scroll_block::scroll_area_set_x(&mut settings.trust.scroll, 1);
    state.stage = ManagerStage::Settings(settings);
    let terminal_area = Rect::new(0, 0, 48, 40);
    crate::tui::view::prepare_for_render(
        &mut state,
        &config,
        std::path::Path::new("/"),
        terminal_area,
    );
    let second_row_position = {
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        painted_trust_label_position(settings, terminal_area, "trust-horizontal-row-one")
    };
    handle_mouse(
        &mut state,
        mouse_kind_at(
            MouseEventKind::Moved,
            second_row_position.x,
            second_row_position.y,
        ),
        terminal_area,
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::TrustRow(1))
    );
}

#[test]
fn translated_trust_widget_buffer_matches_painted_row_hit_test() {
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Trust;
    settings.trust.pending = (0..20)
        .map(|index| SettingsTrustRow {
            role: format!("translated-row-{index:02}-unique"),
            git: "/r".into(),
            trusted: true,
        })
        .collect();
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.trust.scroll, u16::MAX);

    let terminal_area = Rect::new(0, 0, 100, 24);
    let compact_area = Rect::new(12, 4, 52, 8);
    let compact_buffer = translated_trust_tab_test_buffer(&settings, terminal_area, compact_area);
    let compact_last_row = test_buffer_label_position(&compact_buffer, "translated-row-19-unique");
    assert!(compact_last_row.x > 0 && compact_last_row.y > 0);
    assert_eq!(
        settings_trust_row_at_position(
            compact_area,
            compact_last_row.x,
            compact_last_row.y,
            u16::MAX,
            settings.trust.pending.len(),
        ),
        Some(19),
        "compact translated paint and hit testing agree at the stale-scroll clamp"
    );
    let compact_blank_column = compact_area.right().saturating_sub(2);
    assert_eq!(
        compact_buffer[(compact_blank_column, compact_last_row.y)].symbol(),
        " "
    );
    assert_eq!(
        settings_trust_row_at_position(
            compact_area,
            compact_blank_column,
            compact_last_row.y,
            u16::MAX,
            settings.trust.pending.len(),
        ),
        Some(19),
        "a blank content cell on the painted row retains its row identity"
    );

    let enlarged_area = Rect::new(8, 3, 66, 14);
    let enlarged_buffer = translated_trust_tab_test_buffer(&settings, terminal_area, enlarged_area);
    let enlarged_first_row =
        test_buffer_label_position(&enlarged_buffer, "translated-row-09-unique");
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            enlarged_first_row.x,
            enlarged_first_row.y,
            u16::MAX,
            settings.trust.pending.len(),
        ),
        Some(9),
        "enlarging the translated panel changes the painted stale-scroll clamp"
    );
    for column in [
        enlarged_area.x,
        enlarged_area.x.saturating_add(1),
        enlarged_area.x.saturating_add(2),
        enlarged_area.right().saturating_sub(1),
    ] {
        assert_eq!(
            settings_trust_row_at_position(
                enlarged_area,
                column,
                enlarged_first_row.y,
                u16::MAX,
                settings.trust.pending.len(),
            ),
            None,
            "border and padded columns have no Trust row identity"
        );
    }
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            enlarged_first_row.x,
            enlarged_area.bottom(),
            u16::MAX,
            settings.trust.pending.len(),
        ),
        None,
        "offscreen coordinates have no Trust row identity"
    );

    settings.trust.pending.truncate(2);
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.trust.scroll, 0);
    let short_buffer = translated_trust_tab_test_buffer(&settings, terminal_area, enlarged_area);
    let header =
        test_buffer_label_position(&short_buffer, "Role                         Trust      Git");
    let first_row = test_buffer_label_position(&short_buffer, "translated-row-00-unique");
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            first_row.x,
            first_row.y,
            0,
            settings.trust.pending.len(),
        ),
        Some(0)
    );
    assert_eq!(header.y, enlarged_area.y.saturating_add(1));
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            header.x,
            header.y,
            0,
            settings.trust.pending.len(),
        ),
        None,
        "the actually painted header is not a Trust row"
    );
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            first_row.x,
            enlarged_area.y.saturating_add(4),
            0,
            settings.trust.pending.len(),
        ),
        None,
        "blank body below painted rows has no row identity"
    );
    assert_eq!(
        settings_trust_row_at_position(
            enlarged_area,
            enlarged_area.x.saturating_sub(1),
            first_row.y,
            0,
            settings.trust.pending.len(),
        ),
        None,
        "outside coordinates have no row identity"
    );

    settings.trust.pending.clear();
    let empty_buffer = translated_trust_tab_test_buffer(&settings, terminal_area, enlarged_area);
    let empty_marker = test_buffer_label_position(&empty_buffer, "(none)");
    assert_eq!(
        settings_trust_row_at_position(enlarged_area, empty_marker.x, empty_marker.y, 0, 0),
        None,
        "the actually painted empty marker has no row identity"
    );
}

#[test]
fn mouse_down_on_editor_tab_clears_secrets_view_when_leaving() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor
        .unmasked_rows
        .insert((SecretsScopeTag::Workspace, "TOKEN".to_owned()));
    editor.secrets_expanded.insert("agent-smith".to_owned());
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(&mut state, mouse_down_at(3, 3), term(100), Some(&config));

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::General);
    assert!(editor.unmasked_rows.is_empty());
    assert!(editor.secrets_expanded.is_empty());
}

#[test]
fn mouse_down_on_url_row_in_prelude_with_url_does_not_drag() {
    use crate::tui::components::file_browser::FileBrowserState;
    use crate::tui::state::CreatePreludeState;
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut fb = FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        tmp.path().to_path_buf(),
        parent,
    ));
    fb.handle_key(key(KeyCode::Down));
    fb.handle_key(key(KeyCode::Enter));
    fb.pending_git_prompt = Some(repo);
    fb.pending_git_url = Some("file:///tmp/unreachable".to_owned());

    let prelude = CreatePreludeState {
        modal: Some(Modal::FileBrowser {
            target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        }),
        ..CreatePreludeState::default()
    };
    state.stage = ManagerStage::CreatePrelude(prelude);

    let term = term_120x40();
    let mut hit = None;
    for y in 0..term.height {
        for x in 0..term.width {
            let mouse = mouse_down_at(x, y);
            if file_browser_url_row_at(&state, mouse, term) {
                hit = Some(mouse);
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let hit = hit.expect("URL row should have a clickable hitbox");

    let outcome = handle_mouse(&mut state, hit, term);
    assert!(matches!(outcome, InputOutcome::Continue));
    let effects = state.drain_effects();
    match effects.as_slice() {
        [ManagerEffect::OpenUrl(url)] => {
            assert_eq!(url, "file:///tmp/unreachable");
        }
        other => panic!("expected OpenUrl effect, got {other:?}"),
    }
    assert!(
        state.drag_state.is_none(),
        "URL click must not start a seam drag",
    );
}

#[test]
fn mouse_down_outside_url_row_in_prelude_is_silent_noop() {
    use crate::tui::components::file_browser::FileBrowserState;
    use crate::tui::state::CreatePreludeState;
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut fb = FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        tmp.path().to_path_buf(),
        parent,
    ));
    fb.handle_key(key(KeyCode::Down));
    fb.handle_key(key(KeyCode::Enter));
    fb.pending_git_url = Some("file:///tmp/unreachable".to_owned());

    let prelude = CreatePreludeState {
        modal: Some(Modal::FileBrowser {
            target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        }),
        ..CreatePreludeState::default()
    };
    state.stage = ManagerStage::CreatePrelude(prelude);

    handle_mouse(&mut state, mouse_down_at(60, 0), term_120x40());
    assert!(state.drag_state.is_none());
}

#[test]
fn click_on_first_row_sets_selected_to_zero() {
    let mut state = list_state_with_saved(3);
    state.selected = 2;
    handle_mouse(&mut state, mouse_at(10, 3), term(100));
    assert_eq!(state.selected, 0);
}

#[test]
fn click_on_fifth_row_sets_selected_to_four() {
    let mut state = list_state_with_saved(5);
    state.selected = 0;
    handle_mouse(&mut state, mouse_at(10, 7), term(100));
    assert_eq!(state.selected, 4);
}
