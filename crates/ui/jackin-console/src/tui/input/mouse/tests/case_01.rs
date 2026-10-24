// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn painted_editor_label_position(
    editor: &EditorState<'_>,
    config: &jackin_config::AppConfig,
    terminal_area: Rect,
    op_available: bool,
    label: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("editor TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::screens::editor::view::render_editor_with_footer(
                frame,
                frame.area(),
                editor,
                config,
                op_available,
            );
        })
        .expect("editor screen should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .find_map(|row| {
            let text = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(label)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                u16::try_from(column).expect("painted editor label column fits u16"),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual editor TestBackend buffer lacks {label:?}"))
}

fn painted_github_picker_row_position(
    state: &ManagerState<'_>,
    terminal_area: Rect,
    marker: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let modal = state
        .list_modal
        .as_ref()
        .expect("GitHub picker modal should be open before paint");
    let Modal::GithubPicker { state: picker } = modal else {
        panic!("GitHub picker modal expected before paint");
    };
    let modal_area = modal.rect(terminal_area);
    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("GitHub picker TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::components::github_picker::render(frame, modal_area, picker);
        })
        .expect("GitHub picker should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (modal_area.y..modal_area.bottom())
        .find_map(|row| {
            let text = (modal_area.x..modal_area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(marker)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                modal_area.x.saturating_add(
                    u16::try_from(column).expect("painted GitHub row column fits u16"),
                ),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual GitHub picker TestBackend buffer lacks {marker:?}"))
}

fn github_choices(count: usize) -> Vec<crate::github_mounts::GithubChoice> {
    (0..count)
        .map(|index| crate::github_mounts::GithubChoice {
            src: format!("/workspace/repository-{index:02}"),
            branch: format!("branch-{index:02}-painted"),
            url: format!("https://github.invalid/owner/repository-{index:02}"),
        })
        .collect()
}

fn workdir_picker_state(count: usize) -> crate::tui::components::workdir_pick::WorkdirPickState {
    let choices = (0..count)
        .map(
            |index| crate::tui::components::workdir_pick::WorkdirChoice {
                path: format!("/w/project-{index:02}"),
                label: format!("choice-{index:02}-painted"),
            },
        )
        .collect::<Vec<_>>();
    crate::tui::components::workdir_pick::WorkdirPickState {
        list_state: termrock::widgets::ListState::for_count(choices.len()),
        choices,
    }
}

fn state_with_workdir_picker(count: usize, create_prelude: bool) -> ManagerState<'static> {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    let picker = workdir_picker_state(count);

    if create_prelude {
        state.stage = ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState {
            modal: Some(Modal::WorkdirPick { state: picker }),
            ..Default::default()
        });
    } else {
        let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Roles;
        editor.tab_content_height = 50;
        crate::tui::scroll_block::scroll_area_set_y(&mut editor.tab_scroll, 4);
        editor.modal = Some(Modal::WorkdirPick { state: picker });
        state.stage = ManagerStage::Editor(editor);
    }
    state
}

fn workdir_picker_context<'state>(
    state: &'state ManagerState<'_>,
    terminal_area: Rect,
) -> (
    Rect,
    &'state crate::tui::components::workdir_pick::WorkdirPickState,
) {
    let modal = match &state.stage {
        ManagerStage::Editor(editor) => editor.modal.as_ref(),
        ManagerStage::CreatePrelude(prelude) => prelude.modal.as_ref(),
        _ => None,
    }
    .expect("editor or create-prelude Workdir modal should be present");
    let Modal::WorkdirPick { state: picker } = modal else {
        panic!("Workdir picker modal expected");
    };
    (modal.rect(terminal_area), picker)
}

fn painted_workdir_picker_row_position(
    state: &ManagerState<'_>,
    terminal_area: Rect,
    marker: &str,
) -> ratatui::layout::Position {
    use ratatui::{Terminal, backend::TestBackend};

    let (modal_area, picker) = workdir_picker_context(state, terminal_area);
    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("Workdir picker TestBackend should initialize");
    terminal
        .draw(|frame| {
            crate::tui::components::workdir_pick::render(frame, modal_area, picker);
        })
        .expect("Workdir picker should render to TestBackend");

    let buffer = terminal.backend().buffer();
    (modal_area.y..modal_area.bottom())
        .find_map(|row| {
            let text = (modal_area.x..modal_area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(marker)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                modal_area.x.saturating_add(
                    u16::try_from(column).expect("painted Workdir label column fits u16"),
                ),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual Workdir picker TestBackend buffer lacks {marker:?}"))
}

fn assert_workdir_picker_background_unchanged(
    state: &ManagerState<'_>,
    expected_picker_selection: usize,
    original_selected: usize,
    original_focus: &crate::tui::focus::TabFocus<MountScrollFocus>,
    original_list_scroll: u16,
    original_editor_scroll: Option<u16>,
) {
    let (_, picker) = workdir_picker_context(state, term_120x40());
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(expected_picker_selection)
    );
    assert_eq!(state.selected, original_selected);
    assert_eq!(&state.list_focus_owner, original_focus);
    assert_eq!(state.list_names_scroll.offset_y(), original_list_scroll);
    if let Some(original_editor_scroll) = original_editor_scroll {
        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor route should remain active");
        };
        assert_eq!(editor.tab_scroll.offset_y(), original_editor_scroll);
    }
}

#[derive(Debug, Clone, Copy)]
enum RolePickerRoute {
    List,
    Editor,
    EditorOverride,
    CreatePrelude,
    CreatePreludeOverride,
    SettingsMounts,
    SettingsEnv,
}

impl RolePickerRoute {
    const ALL: [Self; 7] = [
        Self::List,
        Self::Editor,
        Self::EditorOverride,
        Self::CreatePrelude,
        Self::CreatePreludeOverride,
        Self::SettingsMounts,
        Self::SettingsEnv,
    ];
}

#[derive(Debug, PartialEq, Eq)]
struct RolePickerBackgroundSnapshot {
    selected: usize,
    list_focus_owner: crate::tui::focus::TabFocus<MountScrollFocus>,
    list_scroll: u16,
    pending_effects: usize,
    stage: RolePickerBackgroundStage,
}

#[derive(Debug, PartialEq, Eq)]
enum RolePickerBackgroundStage {
    List,
    Editor {
        active_tab: EditorTab,
        tab_focus_owner:
            crate::tui::focus::TabFocus<crate::tui::screens::editor::model::EditorFocusTarget>,
        tab_scroll: u16,
    },
    CreatePrelude,
    Settings {
        active_tab: SettingsTab,
        focus_owner: crate::tui::focus::TabFocus<SettingsTab>,
        mounts_scroll: u16,
        env_scroll: u16,
    },
}

fn role_picker_background_snapshot(state: &ManagerState<'_>) -> RolePickerBackgroundSnapshot {
    let stage = match &state.stage {
        ManagerStage::List => RolePickerBackgroundStage::List,
        ManagerStage::Editor(editor) => RolePickerBackgroundStage::Editor {
            active_tab: editor.active_tab,
            tab_focus_owner: editor.focus_owner.clone(),
            tab_scroll: editor.tab_scroll.offset_y(),
        },
        ManagerStage::CreatePrelude(_) => RolePickerBackgroundStage::CreatePrelude,
        ManagerStage::Settings(settings) => RolePickerBackgroundStage::Settings {
            active_tab: settings.active_tab,
            focus_owner: settings.focus_owner.clone(),
            mounts_scroll: settings.mounts.scroll.offset_y(),
            env_scroll: settings.env.scroll.offset_y(),
        },
        _ => panic!("role picker fixtures use List, Editor, CreatePrelude, or Settings"),
    };
    RolePickerBackgroundSnapshot {
        selected: state.selected,
        list_focus_owner: state.list_focus_owner.clone(),
        list_scroll: state.list_names_scroll.offset_y(),
        pending_effects: state.pending_effects.len(),
        stage,
    }
}

fn role_picker_manager_state(route: RolePickerRoute, count: usize) -> ManagerState<'static> {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    let picker = crate::tui::state::RolePickerState::new(
        (0..count)
            .map(|index| jackin_core::RoleSelector::new(None, &format!("role-{index:02}-painted")))
            .collect(),
    );

    match route {
        RolePickerRoute::List => {
            state.list_modal = Some(Modal::RolePicker { state: picker });
        }
        RolePickerRoute::Editor | RolePickerRoute::EditorOverride => {
            let modal = if matches!(route, RolePickerRoute::EditorOverride) {
                Modal::RoleOverridePicker { state: picker }
            } else {
                Modal::RolePicker { state: picker }
            };
            let mut editor =
                EditorState::new_edit("role-picker-test".into(), WorkspaceConfig::default());
            editor.active_tab = EditorTab::Roles;
            editor.tab_content_height = 50;
            crate::tui::scroll_block::scroll_area_set_y(&mut editor.tab_scroll, 4);
            editor.modal = Some(modal);
            state.stage = ManagerStage::Editor(editor);
        }
        RolePickerRoute::CreatePrelude | RolePickerRoute::CreatePreludeOverride => {
            let modal = if matches!(route, RolePickerRoute::CreatePreludeOverride) {
                Modal::RoleOverridePicker { state: picker }
            } else {
                Modal::RolePicker { state: picker }
            };
            state.stage = ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState {
                modal: Some(modal),
                ..Default::default()
            });
        }
        RolePickerRoute::SettingsMounts | RolePickerRoute::SettingsEnv => {
            let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
            let tab = if matches!(route, RolePickerRoute::SettingsMounts) {
                SettingsTab::Mounts
            } else {
                SettingsTab::Environments
            };
            settings.active_tab = tab;
            settings.set_content_focused(tab, true);
            match route {
                RolePickerRoute::SettingsMounts => {
                    crate::tui::scroll_block::scroll_area_set_y(&mut settings.mounts.scroll, 4);
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountRolePicker { state: picker });
                }
                RolePickerRoute::SettingsEnv => {
                    crate::tui::scroll_block::scroll_area_set_y(&mut settings.env.scroll, 4);
                    settings
                        .env
                        .modals
                        .open(SettingsModal::EnvRolePicker { state: picker });
                }
                _ => unreachable!(),
            }
            state.stage = ManagerStage::Settings(settings);
        }
    }
    state
}

fn role_picker_context<'state>(
    state: &'state ManagerState<'_>,
    route: RolePickerRoute,
    term_size: Rect,
) -> (Rect, &'state crate::tui::state::RolePickerState) {
    match route {
        RolePickerRoute::List => {
            let modal = state
                .list_modal
                .as_ref()
                .expect("list role picker should be open");
            let Modal::RolePicker { state: picker } = modal else {
                panic!("list role picker modal expected");
            };
            (modal.rect(term_size), picker)
        }
        RolePickerRoute::Editor | RolePickerRoute::EditorOverride => {
            let ManagerStage::Editor(editor) = &state.stage else {
                panic!("editor role picker stage expected");
            };
            let modal = editor
                .modal
                .as_ref()
                .expect("editor role picker should be open");
            let ((RolePickerRoute::Editor, Modal::RolePicker { state: picker })
            | (RolePickerRoute::EditorOverride, Modal::RoleOverridePicker { state: picker })) =
                (route, modal)
            else {
                panic!("expected the matching editor role picker modal");
            };
            (modal.rect(term_size), picker)
        }
        RolePickerRoute::CreatePrelude | RolePickerRoute::CreatePreludeOverride => {
            let ManagerStage::CreatePrelude(prelude) = &state.stage else {
                panic!("create-prelude role picker stage expected");
            };
            let modal = prelude
                .modal
                .as_ref()
                .expect("prelude role picker should be open");
            let ((RolePickerRoute::CreatePrelude, Modal::RolePicker { state: picker })
            | (
                RolePickerRoute::CreatePreludeOverride,
                Modal::RoleOverridePicker { state: picker },
            )) = (route, modal)
            else {
                panic!("expected the matching create-prelude role picker modal");
            };
            (modal.rect(term_size), picker)
        }
        RolePickerRoute::SettingsMounts => {
            let ManagerStage::Settings(settings) = &state.stage else {
                panic!("settings role picker stage expected");
            };
            let Some(SettingsModal::MountRolePicker { state: picker }) =
                settings.mounts.modals.current()
            else {
                panic!("settings mounts role picker should be open");
            };
            (
                crate::tui::components::modal_overlay::role_picker_overlay_rect(
                    term_size,
                    picker.filtered.len(),
                ),
                picker,
            )
        }
        RolePickerRoute::SettingsEnv => {
            let ManagerStage::Settings(settings) = &state.stage else {
                panic!("settings role picker stage expected");
            };
            let Some(SettingsModal::EnvRolePicker { state: picker }) =
                settings.env.modals.current()
            else {
                panic!("settings env role picker should be open");
            };
            (
                crate::tui::components::modal_overlay::role_picker_overlay_rect(
                    term_size,
                    picker.filtered.len(),
                ),
                picker,
            )
        }
    }
}

fn role_picker_context_mut<'state>(
    state: &'state mut ManagerState<'_>,
    route: RolePickerRoute,
) -> &'state mut crate::tui::state::RolePickerState {
    match route {
        RolePickerRoute::List => {
            let Some(Modal::RolePicker { state: picker }) = state.list_modal.as_mut() else {
                panic!("list role picker modal expected");
            };
            picker
        }
        RolePickerRoute::Editor | RolePickerRoute::EditorOverride => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                panic!("editor role picker stage expected");
            };
            match (route, editor.modal.as_mut()) {
                (RolePickerRoute::Editor, Some(Modal::RolePicker { state }))
                | (RolePickerRoute::EditorOverride, Some(Modal::RoleOverridePicker { state })) => {
                    state
                }
                _ => panic!("expected the matching editor role picker modal"),
            }
        }
        RolePickerRoute::CreatePrelude | RolePickerRoute::CreatePreludeOverride => {
            let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
                panic!("create-prelude role picker stage expected");
            };
            match (route, prelude.modal.as_mut()) {
                (RolePickerRoute::CreatePrelude, Some(Modal::RolePicker { state }))
                | (
                    RolePickerRoute::CreatePreludeOverride,
                    Some(Modal::RoleOverridePicker { state }),
                ) => state,
                _ => panic!("expected the matching create-prelude role picker modal"),
            }
        }
        RolePickerRoute::SettingsMounts => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                panic!("settings role picker stage expected");
            };
            let Some(SettingsModal::MountRolePicker { state }) =
                settings.mounts.modals.current_mut()
            else {
                panic!("settings mounts role picker should be open");
            };
            state
        }
        RolePickerRoute::SettingsEnv => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                panic!("settings role picker stage expected");
            };
            let Some(SettingsModal::EnvRolePicker { state }) = settings.env.modals.current_mut()
            else {
                panic!("settings env role picker should be open");
            };
            state
        }
    }
}

fn role_picker_test_buffer(
    state: &ManagerState<'_>,
    route: RolePickerRoute,
    term_size: Rect,
) -> (Rect, ratatui::buffer::Buffer) {
    use ratatui::{Terminal, backend::TestBackend};

    let (area, picker) = role_picker_context(state, route, term_size);
    let mut terminal = Terminal::new(TestBackend::new(term_size.width, term_size.height))
        .expect("role picker TestBackend should initialize");
    terminal
        .draw(|frame| crate::tui::components::role_picker::render(frame, area, picker))
        .expect("role picker should render to TestBackend");
    (area, terminal.backend().buffer().clone())
}

fn painted_role_picker_position(
    buffer: &ratatui::buffer::Buffer,
    area: Rect,
    marker: &str,
) -> ratatui::layout::Position {
    (area.y..area.bottom())
        .find_map(|row| {
            let text = (area.x..area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            let byte = text.find(marker)?;
            let column = termrock::text::display_cols(&text[..byte]);
            Some(ratatui::layout::Position::new(
                area.x.saturating_add(
                    u16::try_from(column).expect("painted role label column fits u16"),
                ),
                row,
            ))
        })
        .unwrap_or_else(|| panic!("actual role picker TestBackend buffer lacks {marker:?}"))
}

fn role_picker_selected(state: &ManagerState<'_>, route: RolePickerRoute) -> Option<usize> {
    role_picker_context(state, route, term_120x40())
        .1
        .list_state
        .selected()
        .copied()
}

#[test]
fn role_picker_wheel_uses_painted_rows_for_all_owners() {
    let term_size = term_120x40();
    for route in RolePickerRoute::ALL {
        let mut state = role_picker_manager_state(route, 20);
        let background_before = role_picker_background_snapshot(&state);
        let (area, buffer) = role_picker_test_buffer(&state, route, term_size);
        let first_row = painted_role_picker_position(&buffer, area, "role-00-painted");
        let row_blank = ratatui::layout::Position::new(
            first_row.x + "role-00-painted".len() as u16,
            first_row.y,
        );
        assert_eq!(buffer[(row_blank.x, row_blank.y)].symbol(), " ");

        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, row_blank.x, row_blank.y),
            term_size,
        );
        assert_eq!(role_picker_selected(&state, route), Some(1));
        assert_eq!(
            role_picker_background_snapshot(&state),
            background_before,
            "a role picker wheel must not scroll or focus its parent in {route:?}"
        );

        let selected_marker = "role-01-painted";
        let (area, buffer) = role_picker_test_buffer(&state, route, term_size);
        let selected_row = painted_role_picker_position(&buffer, area, selected_marker);
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, selected_row.x, selected_row.y),
            term_size,
        );
        assert_eq!(role_picker_selected(&state, route), Some(2));
        assert_eq!(
            role_picker_background_snapshot(&state),
            background_before,
            "a role picker wheel must not scroll or focus its parent in {route:?}"
        );

        let selected_marker = "role-02-painted";
        let (area, buffer) = role_picker_test_buffer(&state, route, term_size);
        let selected_row = painted_role_picker_position(&buffer, area, selected_marker);
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollUp, selected_row.x, selected_row.y),
            term_size,
        );
        assert_eq!(role_picker_selected(&state, route), Some(1));
        assert_eq!(
            role_picker_background_snapshot(&state),
            background_before,
            "a role picker wheel must not scroll or focus its parent in {route:?}"
        );
    }
}

#[test]
fn role_picker_wheel_misses_and_edges_preserve_background_for_all_owners() {
    let term_size = term_120x40();
    for route in RolePickerRoute::ALL {
        let mut state = role_picker_manager_state(route, 2);
        let background_before = role_picker_background_snapshot(&state);
        let (area, buffer) = role_picker_test_buffer(&state, route, term_size);
        let title = painted_role_picker_position(&buffer, area, "Select Role");
        let filter = painted_role_picker_position(&buffer, area, "Filter:");
        let filter_spacer = ratatui::layout::Position::new(filter.x, filter.y + 1);
        assert!(area.contains(filter_spacer));
        assert_eq!(buffer[(filter_spacer.x, filter_spacer.y)].symbol(), " ");
        let last_row = painted_role_picker_position(&buffer, area, "role-01-painted");
        let unused_list_body = ratatui::layout::Position::new(last_row.x, last_row.y + 1);
        assert!(unused_list_body.y < area.bottom());

        for position in [
            title,
            filter,
            filter_spacer,
            unused_list_body,
            ratatui::layout::Position::new(area.x, area.y),
            ratatui::layout::Position::new(0, 0),
        ] {
            handle_mouse(
                &mut state,
                mouse_kind_at(MouseEventKind::ScrollDown, position.x, position.y),
                term_size,
            );
            assert_eq!(
                role_picker_selected(&state, route),
                Some(0),
                "an unpainted cell must not scroll the picker in {route:?}"
            );
            assert_eq!(
                role_picker_background_snapshot(&state),
                background_before,
                "an unpainted cell must not scroll or focus the parent in {route:?}"
            );
        }

        let first_row = painted_role_picker_position(&buffer, area, "role-00-painted");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollUp, first_row.x, first_row.y),
            term_size,
        );
        assert_eq!(role_picker_selected(&state, route), Some(0));
        assert_eq!(role_picker_background_snapshot(&state), background_before);

        let mut long_state = role_picker_manager_state(route, 20);
        role_picker_context_mut(&mut long_state, route)
            .list_state
            .select(Some(19));
        let long_background = role_picker_background_snapshot(&long_state);
        let (long_area, long_buffer) = role_picker_test_buffer(&long_state, route, term_size);
        let last_visible_row =
            painted_role_picker_position(&long_buffer, long_area, "role-19-painted");
        let right_inner_gutter =
            ratatui::layout::Position::new(long_area.right().saturating_sub(2), last_visible_row.y);
        assert!(long_area.x < right_inner_gutter.x);
        assert!(right_inner_gutter.x < long_area.right().saturating_sub(1));
        assert!(!crate::tui::components::role_picker::selection_hit_at(
            long_area,
            role_picker_context(&long_state, route, term_size).1,
            right_inner_gutter,
        ));
        handle_mouse(
            &mut long_state,
            mouse_kind_at(
                MouseEventKind::ScrollDown,
                right_inner_gutter.x,
                right_inner_gutter.y,
            ),
            term_size,
        );
        assert_eq!(role_picker_selected(&long_state, route), Some(19));
        assert_eq!(
            role_picker_background_snapshot(&long_state),
            long_background
        );
        handle_mouse(
            &mut long_state,
            mouse_kind_at(
                MouseEventKind::ScrollDown,
                last_visible_row.x,
                last_visible_row.y,
            ),
            term_size,
        );
        assert_eq!(role_picker_selected(&long_state, route), Some(19));
        assert_eq!(
            role_picker_background_snapshot(&long_state),
            long_background
        );

        let mut filtered_state = role_picker_manager_state(route, 20);
        let filtered_background = role_picker_background_snapshot(&filtered_state);
        role_picker_context_mut(&mut filtered_state, route)
            .list_state
            .select(Some(19));
        for ch in "05".chars() {
            role_picker_context_mut(&mut filtered_state, route).handle_key(key(KeyCode::Char(ch)));
        }
        {
            let picker = role_picker_context(&filtered_state, route, term_size).1;
            assert_eq!(picker.filter, "05");
            assert_eq!(picker.filtered.len(), 1);
            assert_eq!(picker.list_state.selected().copied(), Some(0));
        }

        let shrunk_term = Rect::new(0, 0, 80, 24);
        let (filtered_area, filtered_buffer) =
            role_picker_test_buffer(&filtered_state, route, shrunk_term);
        let filtered_row =
            painted_role_picker_position(&filtered_buffer, filtered_area, "role-05-painted");
        let filtered_picker = role_picker_context(&filtered_state, route, shrunk_term).1;
        assert!(crate::tui::components::role_picker::selection_hit_at(
            filtered_area,
            filtered_picker,
            filtered_row,
        ));
        assert_eq!(filtered_picker.list_state.selected().copied(), Some(0));
        assert_eq!(
            role_picker_background_snapshot(&filtered_state),
            filtered_background
        );

        for ch in "z".chars() {
            role_picker_context_mut(&mut filtered_state, route).handle_key(key(KeyCode::Char(ch)));
        }
        let no_match_filter = "05z";
        {
            let picker = role_picker_context(&filtered_state, route, shrunk_term).1;
            assert_eq!(picker.filter, no_match_filter);
            assert!(picker.filtered.is_empty());
            assert_eq!(picker.list_state.selected().copied(), None);
        }
        let (empty_area, empty_buffer) =
            role_picker_test_buffer(&filtered_state, route, shrunk_term);
        let no_matches = painted_role_picker_position(&empty_buffer, empty_area, "no matches");
        handle_mouse(
            &mut filtered_state,
            mouse_kind_at(MouseEventKind::ScrollDown, no_matches.x, no_matches.y),
            shrunk_term,
        );
        let picker = role_picker_context(&filtered_state, route, shrunk_term).1;
        assert_eq!(picker.filter, no_match_filter);
        assert!(picker.filtered.is_empty());
        assert_eq!(picker.list_state.selected().copied(), None);
        assert_eq!(
            role_picker_background_snapshot(&filtered_state),
            filtered_background
        );
    }
}

#[test]
fn role_picker_help_blocks_hidden_picker_for_all_owners() {
    let term_size = term_120x40();
    for route in RolePickerRoute::ALL {
        let mut state = role_picker_manager_state(route, 20);
        let background_before = role_picker_background_snapshot(&state);
        let (area, buffer) = role_picker_test_buffer(&state, route, term_size);
        let first_row = painted_role_picker_position(&buffer, area, "role-00-painted");
        state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());

        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, first_row.x, first_row.y),
            term_size,
        );

        assert!(state.keyboard_help.is_some(), "help must retain ownership");
        assert_eq!(role_picker_selected(&state, route), Some(0));
        assert_eq!(
            role_picker_background_snapshot(&state),
            background_before,
            "keyboard help must block picker and parent input in {route:?}"
        );
    }
}

#[test]
fn conformance_wire_mouse_coordinates_become_only_semantic_action() -> anyhow::Result<()> {
    const CHILD: &str = "JACKIN_MOUSE_PRIVACY_WIRE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "tui::input::mouse::tests::case_01::conformance_wire_mouse_coordinates_become_only_semantic_action",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated mouse privacy test failed");
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )?;
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "wire-private-workspace".into(),
        WorkspaceConfig::default(),
    ));
    let terminal_area = term(100);
    let position = {
        let ManagerStage::Editor(editor) = &state.stage else {
            anyhow::bail!("expected editor stage before paint");
        };
        painted_editor_label_position(editor, &config, terminal_area, state.op_available, "Mounts")
    };
    let coordinate = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: position.x,
        row: position.y,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse_with_config(&mut state, coordinate, terminal_area, Some(&config));
    let ManagerStage::Editor(editor) = &state.stage else {
        anyhow::bail!("mouse route left editor stage");
    };
    anyhow::ensure!(
        editor.active_tab == EditorTab::Mounts,
        "coordinates did not reach the real tab hit-test"
    );
    drop(jackin_telemetry::ui::take_action_parent());
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|reason| anyhow::anyhow!("validation event rejected: {reason:?}"))?;
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);
    anyhow::ensure!(
        runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))),
        "mouse route did not export all three signals"
    );
    let action_spans = testbed
        .spans()
        .into_iter()
        .filter(|span| span.name == "ui.action")
        .collect::<Vec<_>>();
    anyhow::ensure!(action_spans.len() == 1, "unexpected ui.action spans");
    let action = &action_spans[0];
    anyhow::ensure!(
        format!("{:?}", action.attributes).contains("tab.switch"),
        "mouse action was not exported semantically: {action:?}"
    );
    let ui_action_count = testbed
        .metrics()
        .into_iter()
        .flat_map(|request| request.resource_metrics)
        .flat_map(|resource| resource.scope_metrics)
        .flat_map(|scope| scope.metrics)
        .filter(|metric| metric.name == "ui.actions")
        .filter_map(|metric| metric.data)
        .filter_map(|data| match data {
            opentelemetry_proto::tonic::metrics::v1::metric::Data::Sum(sum) => Some(sum),
            _ => None,
        })
        .flat_map(|sum| sum.data_points)
        .filter_map(|point| point.value)
        .map(|value| match value {
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(value) => {
                value as f64
            }
            opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(value) => {
                value
            }
        })
        .sum::<f64>();
    anyhow::ensure!(
        (ui_action_count - 1.0).abs() < f64::EPSILON,
        "one semantic mouse action must increment ui.actions exactly once, got {ui_action_count}"
    );
    for key in action
        .attributes
        .iter()
        .map(|attribute| attribute.key.as_str())
        .chain(testbed.metric_dimension_keys().iter().map(String::as_str))
    {
        anyhow::ensure!(
            !matches!(
                key,
                "row" | "column" | "mouse.row" | "mouse.column" | "ui.pointer.x" | "ui.pointer.y"
            ),
            "raw pointer coordinate key escaped to OTLP: {key}"
        );
    }
    anyhow::ensure!(
        testbed
            .prohibited_value_violations(&["wire-private-workspace"])
            .is_empty(),
        "private editor identity escaped to OTLP"
    );
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn content_areas_exclude_the_cached_footer() {
    let term = Rect::new(0, 0, 80, 24);

    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.cached_footer_h = 3;
    let s = settings.content_area(term);
    assert_eq!(s.y, SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT);
    assert_eq!(
        s.y + s.height,
        term.height - 3,
        "settings content must stop where the footer begins"
    );

    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.cached_footer_h = 4;
    let e = editor.content_area(term);
    assert_eq!(
        e.y + e.height,
        term.height - 4,
        "editor content must stop where the footer begins"
    );
}

#[test]
fn mouse_down_on_seam_starts_drag() {
    let mut state = list_state();
    assert_eq!(state.list_split_pct, DEFAULT_SPLIT_PCT);
    let e = mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT);
    handle_mouse(&mut state, e, term(100));
    assert!(
        state.drag_state.is_some(),
        "Down on seam must capture drag anchor; got {:?}",
        state.drag_state,
    );
    let drag = state.drag_state.unwrap();
    assert_eq!(drag.anchor_pct, DEFAULT_SPLIT_PCT);
    assert_eq!(drag.anchor_x, DEFAULT_SPLIT_PCT);
}

#[test]
fn mouse_drag_updates_split_pct() {
    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    let target = DEFAULT_SPLIT_PCT + 10;
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Drag(MouseButton::Left), target),
        term(100),
    );
    assert_eq!(state.list_split_pct, target);
}

#[test]
fn mouse_drag_clamps_to_min_and_max() {
    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Drag(MouseButton::Left), 0),
        term(100),
    );
    assert_eq!(state.list_split_pct, MIN_SPLIT_PCT);

    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Drag(MouseButton::Left), 99),
        term(100),
    );
    assert_eq!(state.list_split_pct, MAX_SPLIT_PCT);
}

#[test]
fn mouse_up_ends_drag() {
    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    assert!(state.drag_state.is_some());
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Up(MouseButton::Left), 60),
        term(100),
    );
    assert!(state.drag_state.is_none(), "Up must clear drag anchor");
}

#[test]
fn mouse_down_far_from_seam_does_not_start_drag() {
    let mut state = list_state();
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), 2),
        term(100),
    );
    assert!(state.drag_state.is_none(), "left-pane click must not drag");
    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), 80),
        term(100),
    );
    assert!(state.drag_state.is_none(), "right-pane click must not drag");
}

#[test]
fn drag_ignored_when_list_modal_open() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![MountConfig {
            src: "/w".into(),
            dst: "/w".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..Default::default()
    };
    drop(crate::github_mounts::resolve_for_workspace(&ws));
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(vec![
            crate::github_mounts::GithubChoice {
                src: "/w".into(),
                branch: "main".into(),
                url: "https://github.com/o/r".into(),
            },
        ]),
    });

    handle_mouse(
        &mut state,
        mouse(MouseEventKind::Down(MouseButton::Left), DEFAULT_SPLIT_PCT),
        term(100),
    );
    assert!(
        state.drag_state.is_none(),
        "Down with list_modal open must not drag",
    );
}

#[test]
fn list_github_picker_wheel_scrolls_modal_selection() {
    let mut state = list_state();
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(2)),
    });
    let term = term_120x40();
    let position = painted_github_picker_row_position(&state, term, "branch-01-painted");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, position.x, position.y),
        term,
    );

    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("github picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}

#[test]
fn list_github_picker_wheel_misses_preserve_selection_and_background() {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(3)),
    });
    let term = term_120x40();
    let modal_area = state
        .list_modal
        .as_ref()
        .expect("GitHub picker modal should be open")
        .rect(term);
    let original_selected = state.selected;
    let original_focus = state.list_focus_owner.clone();
    let original_scroll = state.list_names_scroll.offset_y();

    for (column, row) in [(modal_area.x.saturating_add(2), modal_area.y), (0, 0)] {
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, column, row),
            term,
        );
        let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
            panic!("GitHub picker modal expected after an unpainted wheel event");
        };
        assert_eq!(
            picker.list_state.selected().copied(),
            Some(0),
            "an unpainted modal or outside cell must not scroll the picker"
        );
        assert_eq!(state.selected, original_selected);
        assert_eq!(state.list_focus_owner, original_focus);
        assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
    }

    let painted_row = painted_github_picker_row_position(&state, term, "branch-00-painted");
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, painted_row.x, painted_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("hidden GitHub picker modal should remain stored");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(0),
        "a blocked owner must keep the hidden GitHub picker unchanged"
    );
    assert_eq!(state.selected, original_selected);
    assert_eq!(state.list_focus_owner, original_focus);
    assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
}

#[test]
fn list_github_picker_wheel_edges_do_not_wrap_or_scroll_background() {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_names_scroll, 4);
    state
        .list_focus_owner
        .focus_content(MountScrollFocus::Workspace);
    state.list_modal = Some(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(github_choices(2)),
    });
    let term = term_120x40();
    let original_selected = state.selected;
    let original_focus = state.list_focus_owner.clone();
    let original_scroll = state.list_names_scroll.offset_y();
    let first_row = painted_github_picker_row_position(&state, term, "branch-00-painted");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, first_row.x, first_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after first-row wheel event");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(0));

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, first_row.x, first_row.y),
        term,
    );
    let second_row = painted_github_picker_row_position(&state, term, "branch-01-painted");
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after non-edge wheel event");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(1),
        "a valid non-edge wheel event should move to the next choice"
    );

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, second_row.x, second_row.y),
        term,
    );
    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("GitHub picker modal expected after last-row wheel event");
    };
    assert_eq!(
        picker.list_state.selected().copied(),
        Some(1),
        "a valid last-row wheel event must be consumed without wrapping"
    );
    assert_eq!(state.selected, original_selected);
    assert_eq!(state.list_focus_owner, original_focus);
    assert_eq!(state.list_names_scroll.offset_y(), original_scroll);
}

#[test]
fn editor_workdir_picker_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let mounts = vec![MountConfig {
        src: "/workspace/project".into(),
        dst: "/workspace/project".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }];
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&mounts),
    });
    state.stage = ManagerStage::Editor(editor);
    let term = term_120x40();
    let position = painted_workdir_picker_row_position(&state, term, "(mount dst)");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, position.x, position.y),
        term,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        0,
        "background editor must not scroll"
    );
    let Some(Modal::WorkdirPick { state: picker }) = &editor.modal else {
        panic!("workdir picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}

#[test]
fn workdir_picker_wheel_misses_and_blocked_owners_preserve_state() {
    let term = term_120x40();
    for create_prelude in [false, true] {
        let mut state = state_with_workdir_picker(3, create_prelude);
        let (modal_area, _) = workdir_picker_context(&state, term);
        let original_selected = state.selected;
        let original_focus = state.list_focus_owner.clone();
        let original_list_scroll = state.list_names_scroll.offset_y();
        let original_editor_scroll = match &state.stage {
            ManagerStage::Editor(editor) => Some(editor.tab_scroll.offset_y()),
            ManagerStage::CreatePrelude(_) => None,
            _ => panic!("editor or create-prelude stage expected"),
        };

        for (column, row) in [
            (modal_area.x, modal_area.y),
            (modal_area.x + 2, modal_area.y + 1),
            (modal_area.x + 2, modal_area.y + modal_area.height - 3),
            (0, 0),
        ] {
            handle_mouse(
                &mut state,
                mouse_kind_at(MouseEventKind::ScrollDown, column, row),
                term,
            );
            assert_workdir_picker_background_unchanged(
                &state,
                0,
                original_selected,
                &original_focus,
                original_list_scroll,
                original_editor_scroll,
            );
        }

        let painted_row = painted_workdir_picker_row_position(&state, term, "choice-00-painted");
        state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, painted_row.x, painted_row.y),
            term,
        );
        assert!(state.keyboard_help.is_some());
        assert_workdir_picker_background_unchanged(
            &state,
            0,
            original_selected,
            &original_focus,
            original_list_scroll,
            original_editor_scroll,
        );
    }
}

#[test]
fn workdir_picker_wheel_edges_saturate_for_editor_and_create_prelude() {
    let term = term_120x40();
    for create_prelude in [false, true] {
        let mut state = state_with_workdir_picker(3, create_prelude);
        let original_selected = state.selected;
        let original_focus = state.list_focus_owner.clone();
        let original_list_scroll = state.list_names_scroll.offset_y();
        let original_editor_scroll = match &state.stage {
            ManagerStage::Editor(editor) => Some(editor.tab_scroll.offset_y()),
            ManagerStage::CreatePrelude(_) => None,
            _ => panic!("editor or create-prelude stage expected"),
        };

        let first_row = painted_workdir_picker_row_position(&state, term, "choice-00-painted");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollUp, first_row.x, first_row.y),
            term,
        );
        assert_workdir_picker_background_unchanged(
            &state,
            0,
            original_selected,
            &original_focus,
            original_list_scroll,
            original_editor_scroll,
        );

        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, first_row.x, first_row.y),
            term,
        );
        assert_workdir_picker_background_unchanged(
            &state,
            1,
            original_selected,
            &original_focus,
            original_list_scroll,
            original_editor_scroll,
        );

        let second_row = painted_workdir_picker_row_position(&state, term, "choice-01-painted");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, second_row.x, second_row.y),
            term,
        );
        assert_workdir_picker_background_unchanged(
            &state,
            2,
            original_selected,
            &original_focus,
            original_list_scroll,
            original_editor_scroll,
        );

        let last_row = painted_workdir_picker_row_position(&state, term, "choice-02-painted");
        handle_mouse(
            &mut state,
            mouse_kind_at(MouseEventKind::ScrollDown, last_row.x, last_row.y),
            term,
        );
        assert_workdir_picker_background_unchanged(
            &state,
            2,
            original_selected,
            &original_focus,
            original_list_scroll,
            original_editor_scroll,
        );
    }
}

#[test]
fn settings_role_picker_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Mounts;
    settings.set_content_focused(SettingsTab::Mounts, true);
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.mounts.scroll, 4);
    settings.mounts.modals.open(SettingsModal::MountRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    state.stage = ManagerStage::Settings(settings);
    let (area, buffer) =
        role_picker_test_buffer(&state, RolePickerRoute::SettingsMounts, term_120x40());
    let painted_row = painted_role_picker_position(&buffer, area, "scentbird/agent-jones");

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, painted_row.x, painted_row.y),
        term_120x40(),
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(
        settings.mounts.scroll.offset_y(),
        4,
        "background settings must not scroll"
    );
    let Some(SettingsModal::MountRolePicker { state: picker }) = settings.mounts.modals.current()
    else {
        panic!("settings role picker modal expected");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
}
