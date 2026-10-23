// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::components::StatusPopupState;
use crate::tui::input::mouse::clickable_at;

#[test]
fn manager_owner_matches_painted_sidebar_and_route() {
    let mut state = list_state();
    state.inline_new_session_picker = Some((
        "container".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
        ]),
        Vec::new(),
    ));
    state.inline_account_picker = Some(crate::tui::state::AccountPickerState::new(
        "workspace".to_owned(),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));
    state.launch_account_picker = Some(crate::tui::state::AccountPickerState::new(
        jackin_core::RoleSelector::new(None, "architect"),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::InlineAccountPicker
        ),
        "the account picker is painted ahead of the new-session picker"
    );
    state.inline_account_picker = None;
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::LaunchAccountPicker
        ),
        "the launch account picker is painted ahead of the new-session picker"
    );
    state.launch_account_picker = None;
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::InlineNewSessionPicker
        )
    );
    state.inline_account_picker = Some(crate::tui::state::AccountPickerState::new(
        "workspace".to_owned(),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));
    state.launch_account_picker = Some(crate::tui::state::AccountPickerState::new(
        jackin_core::RoleSelector::new(None, "architect"),
        jackin_core::Agent::Claude,
        Vec::new(),
    ));

    state.usage.visible = true;
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Usage,
        "hidden sidebar picker flags must not take ownership over Usage"
    );

    state.list_modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Debug info",
            Vec::new(),
        ),
    });
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::ListModal
        ),
        "a painted List modal stays above Usage"
    );

    state.status_overlay = Some(StatusPopupState::new("Working", "Please wait"));
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::StatusPopup
    );
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::KeyboardHelp
    );

    state.keyboard_help = None;
    state.status_overlay = None;
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".to_owned(),
        WorkspaceConfig::default(),
    ));
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::Stage(
                crate::tui::model::ConsoleManagerStageRoute::Editor
            )
        ),
        "stale Usage, List modal, and picker facts are hidden on Editor"
    );

    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::Stage(
                crate::tui::model::ConsoleManagerStageRoute::Settings
            )
        ),
        "stale Usage, List modal, and picker facts are hidden on Settings"
    );

    state.stage =
        ManagerStage::Workflows(crate::tui::screens::workflows::model::WorkflowScreen::default());
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::Stage(
                crate::tui::model::ConsoleManagerStageRoute::Workflows
            )
        ),
        "stale Usage, List modal, and picker facts are hidden on Workflows"
    );
}

fn workspace_list_click(state: &ManagerState<'_>, term: Rect) -> (MouseEvent, usize) {
    let visual_rows = state.visual_rows_vec();
    (0..term.height)
        .flat_map(|row| (0..term.width).map(move |column| mouse_down_at(column, row)))
        .find_map(|event| {
            let selectable = |row| state.index_of_row(row).is_some();
            let row = crate::tui::screens::workspaces::update::workspace_list_hover_row_at_position(
                visual_rows.as_slice(),
                event.column,
                event.row,
                term,
                crate::tui::layout::split_seam_column(state.list_split_pct, term.width),
                selectable,
            );
            let target = row.and_then(|row| state.index_of_row(row))?;
            (target != state.selected).then_some((event, target))
        })
        .expect("workspace fixture has a selectable row")
}

#[test]
fn help_status_usage_and_workflows_do_not_reach_hidden_pointer_targets() {
    let mut state = list_state_with_saved(3);
    state.selected = 1;
    state.inline_new_session_picker = Some((
        "container".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
            jackin_core::Agent::Codex,
        ]),
        Vec::new(),
    ));
    let term = term_120x40();
    let (background_click, _target_selected) = workspace_list_click(&state, term);
    let original_selected = state.selected;
    let original_focus = state.list_focus_owner.clone();
    let original_list_scroll = state.list_names_scroll.offset_y();
    let original_picker_agent = state
        .inline_new_session_picker
        .as_ref()
        .map(|(_, picker, _)| picker.focused);

    let copyable_info = Modal::ContainerInfo {
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
    };
    state.list_modal = Some(copyable_info);
    let copy_click = (0..term.height)
        .flat_map(|row| (0..term.width).map(move |column| mouse_down_at(column, row)))
        .find(|event| container_info_copyable_row_at(&state, *event, term))
        .expect("container info fixture has one copyable row");

    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    state.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: 50,
        anchor_x: 40,
    });
    state.hover_target = Some(ManagerHoverTarget::ListRow(ManagerListRow::SavedWorkspace(
        0,
    )));
    assert!(!clickable_at(&state, copy_click, term, None));
    handle_mouse(&mut state, copy_click, term);
    handle_mouse(&mut state, mouse_kind_at(MouseEventKind::Moved, 8, 6), term);
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 8, 6),
        term,
    );
    assert!(state.drag_state.is_none());
    assert!(state.hover_target.is_none());
    assert!(
        state.drain_effects().is_empty(),
        "Help blocks the hidden copy action"
    );
    let hidden_list_modal = state.list_modal.take();
    assert!(!clickable_at(&state, background_click, term, None));
    handle_mouse(&mut state, background_click, term);
    assert_eq!(
        state.selected, original_selected,
        "Help blocks workspace selection"
    );
    assert_eq!(
        state.list_focus_owner, original_focus,
        "Help blocks list focus changes"
    );
    assert_eq!(state.list_names_scroll.offset_y(), original_list_scroll);
    state.list_modal = hidden_list_modal;

    state.keyboard_help = None;
    state.status_overlay = Some(StatusPopupState::new("Working", "Please wait"));
    state.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: 50,
        anchor_x: 40,
    });
    assert!(!clickable_at(&state, copy_click, term, None));
    handle_mouse(&mut state, copy_click, term);
    assert!(state.drag_state.is_none());
    assert!(
        state.drain_effects().is_empty(),
        "Status blocks the hidden copy action"
    );
    let hidden_list_modal = state.list_modal.take();
    assert!(!clickable_at(&state, background_click, term, None));
    handle_mouse(&mut state, background_click, term);
    assert_eq!(
        state.selected, original_selected,
        "Status blocks workspace selection"
    );
    assert_eq!(
        state.list_focus_owner, original_focus,
        "Status blocks list focus changes"
    );
    assert_eq!(state.list_names_scroll.offset_y(), original_list_scroll);
    state.list_modal = hidden_list_modal;

    state.status_overlay = None;
    let hidden_list_modal = state.list_modal.take();
    state.usage.visible = true;
    state.drag_state = Some(crate::tui::split::DragState {
        anchor_pct: 50,
        anchor_x: 40,
    });
    assert!(!clickable_at(&state, background_click, term, None));
    handle_mouse(&mut state, background_click, term);
    assert!(state.drag_state.is_none());
    assert!(
        state.drain_effects().is_empty(),
        "Usage remains pointer-inert"
    );
    assert_eq!(
        state.selected, original_selected,
        "Usage blocks workspace selection"
    );
    assert_eq!(
        state.list_focus_owner, original_focus,
        "Usage blocks list focus changes"
    );
    assert_eq!(state.list_names_scroll.offset_y(), original_list_scroll);

    state.list_modal = hidden_list_modal;
    state.usage.visible = false;
    state.stage =
        ManagerStage::Workflows(crate::tui::screens::workflows::model::WorkflowScreen::default());
    let hidden_modal_click = copy_click;
    assert!(!clickable_at(&state, hidden_modal_click, term, None));
    handle_mouse(&mut state, hidden_modal_click, term);
    assert!(
        state.list_modal.is_some(),
        "hidden stale List modal remains preserved"
    );
    assert!(
        state.drain_effects().is_empty(),
        "Workflows cannot reach hidden copy"
    );

    assert_eq!(state.selected, original_selected);
    assert_eq!(
        state
            .inline_new_session_picker
            .as_ref()
            .map(|(_, picker, _)| picker.focused),
        original_picker_agent,
        "blocked pointer events preserve the hidden picker selection"
    );
}

#[test]
fn painted_inert_owners_block_the_same_real_workspace_row_and_drag_target() {
    let term = term_120x40();
    let mut uncovered = list_state_with_saved(4);
    uncovered.selected = 1;
    let (background_click, expected_selection) = workspace_list_click(&uncovered, term);
    handle_mouse(&mut uncovered, background_click, term);
    assert_eq!(
        uncovered.selected, expected_selection,
        "the fixture must select a real row before testing the owner guard"
    );

    for owner in ["help", "status", "usage"] {
        let mut covered = list_state_with_saved(4);
        covered.selected = 1;
        let original_split = covered.list_split_pct;
        let seam = crate::tui::layout::split_seam_column(covered.list_split_pct, term.width);
        handle_mouse(
            &mut covered,
            mouse_kind_at(MouseEventKind::Down(MouseButton::Left), seam, 4),
            term,
        );
        assert!(
            covered.drag_state.is_some(),
            "the List fixture captures the seam"
        );
        match owner {
            "help" => {
                covered.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
            }
            "status" => {
                covered.status_overlay = Some(StatusPopupState::new("Working", "Please wait"));
            }
            "usage" => covered.usage.visible = true,
            _ => unreachable!(),
        }
        let drag_target = mouse_kind_at(MouseEventKind::Drag(MouseButton::Left), seam, 4);
        assert!(
            !clickable_at(&covered, background_click, term, None),
            "{owner} must make the real List row non-clickable"
        );
        handle_mouse(&mut covered, background_click, term);
        handle_mouse(&mut covered, drag_target, term);
        assert_eq!(covered.selected, 1, "{owner} blocks the real row target");
        assert_eq!(
            covered.list_split_pct, original_split,
            "{owner} blocks a captured seam drag"
        );
        assert!(
            covered.drag_state.is_none(),
            "{owner} clears captured drag state"
        );
    }
}

#[test]
fn painted_inert_owners_block_a_real_scroll_block() {
    let term = term(100);
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
    let mut uncovered = selected_demo_state(&config);
    let areas = list_scroll_areas(&uncovered, term, Some(&config)).expect("list scroll areas");
    let wheel = mouse_kind_at(
        MouseEventKind::ScrollDown,
        areas.global.area.x + 1,
        areas.global.area.y + 1,
    );
    handle_mouse_with_config(&mut uncovered, wheel, term, Some(&config));
    assert_eq!(
        uncovered.list_global_mounts_scroll.offset_y(),
        1,
        "the uncovered fixture must change a real scroll offset"
    );

    for owner in ["help", "status", "usage"] {
        let mut covered = selected_demo_state(&config);
        match owner {
            "help" => covered.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal()),
            "status" => {
                covered.status_overlay = Some(StatusPopupState::new("Working", "Please wait"));
            }
            "usage" => covered.usage.visible = true,
            _ => unreachable!(),
        }
        assert!(!clickable_at(&covered, wheel, term, Some(&config)));
        handle_mouse_with_config(&mut covered, wheel, term, Some(&config));
        assert_eq!(
            covered.list_global_mounts_scroll.offset_y(),
            0,
            "{owner} blocks the same real wheel target"
        );
    }
}

#[test]
fn help_blocks_a_hidden_file_browser_url_action() {
    let mut state = list_state();
    let temp = tempfile::tempdir().expect("tempdir");
    let parent = temp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).expect("git fixture");
    let mut browser = crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_at(temp.path().to_path_buf(), parent),
    );
    browser.handle_key(key(KeyCode::Down));
    browser.handle_key(key(KeyCode::Enter));
    browser.pending_git_prompt = Some(repo);
    browser.pending_git_url = Some("file:///tmp/hidden-console39-url".to_owned());
    let mut editor = EditorState::new_edit("workspace".to_owned(), WorkspaceConfig::default());
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: browser,
    });
    state.stage = ManagerStage::Editor(editor);

    let term = term_120x40();
    let url_click = (0..term.height)
        .flat_map(|row| (0..term.width).map(move |column| mouse_down_at(column, row)))
        .find(|event| file_browser_url_row_at(&state, *event, term))
        .expect("hidden file browser URL row");
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());

    assert!(!clickable_at(&state, url_click, term, None));
    handle_mouse(&mut state, url_click, term);
    assert!(
        state.drain_effects().is_empty(),
        "Help blocks the hidden browser URL action"
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("the hidden Editor route must remain preserved");
    };
    assert!(matches!(editor.modal, Some(Modal::FileBrowser { .. })));
}

#[test]
fn settings_error_popup_does_not_scroll_hidden_mount_modal_state() {
    let term = term_120x40();
    let wheel = mouse_kind_at(MouseEventKind::ScrollDown, 60, 20);

    let mut error_over_mount_picker = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Mounts;
    settings.mounts.modals.open(SettingsModal::MountRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    settings.error_popup = Some(crate::tui::components::ErrorPopupState::new(
        "Settings error",
        "The error popup owns pointer input",
    ));
    error_over_mount_picker.stage = ManagerStage::Settings(settings);
    assert!(matches!(
        error_over_mount_picker.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::SettingsErrorPopup
        )
    ));
    handle_mouse(&mut error_over_mount_picker, wheel, term);
    let ManagerStage::Settings(settings) = &error_over_mount_picker.stage else {
        panic!("Settings stage expected");
    };
    let Some(SettingsModal::MountRolePicker { state: picker }) = settings.mounts.modals.current()
    else {
        panic!("hidden MountRolePicker must remain stored");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(0));
    assert!(error_over_mount_picker.drain_effects().is_empty());

    let browser_root = tempfile::tempdir().expect("browser root");
    let mut error_over_mount_browser = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Mounts;
    settings
        .mounts
        .modals
        .open(SettingsModal::MountFileBrowser {
            state: Box::new(file_browser_with_dirs(browser_root.path(), 8)),
        });
    settings.error_popup = Some(crate::tui::components::ErrorPopupState::new(
        "Settings error",
        "The error popup owns pointer input",
    ));
    error_over_mount_browser.stage = ManagerStage::Settings(settings);
    handle_mouse(&mut error_over_mount_browser, wheel, term);
    let ManagerStage::Settings(settings) = &error_over_mount_browser.stage else {
        panic!("Settings stage expected");
    };
    let Some(SettingsModal::MountFileBrowser { state: browser }) = settings.mounts.modals.current()
    else {
        panic!("hidden MountFileBrowser must remain stored");
    };
    assert_eq!(browser.list_state.selected().copied(), Some(0));
    assert!(error_over_mount_browser.drain_effects().is_empty());
}

#[test]
fn settings_mounts_owner_does_not_scroll_hidden_auth_file_browser() {
    let term = term_120x40();
    let wheel = mouse_kind_at(MouseEventKind::ScrollDown, 60, 20);
    let browser_root = tempfile::tempdir().expect("browser root");
    let hidden_auth_browser = file_browser_with_dirs(browser_root.path(), 8);
    let mut mounts_confirm_over_auth_browser = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Mounts;
    settings.mounts.modals.open(SettingsModal::MountConfirm {
        action: GlobalMountConfirm::Remove,
        state: crate::tui::components::ConfirmState::new("Remove mount?"),
    });
    settings
        .auth
        .modals
        .open(SettingsModal::AuthSourceFolderPicker {
            state: hidden_auth_browser,
        });
    mounts_confirm_over_auth_browser.stage = ManagerStage::Settings(settings);
    assert!(matches!(
        mounts_confirm_over_auth_browser.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::SettingsMountsModal
        )
    ));
    handle_mouse(&mut mounts_confirm_over_auth_browser, wheel, term);
    let ManagerStage::Settings(settings) = &mounts_confirm_over_auth_browser.stage else {
        panic!("Settings stage expected");
    };
    let Some(SettingsModal::AuthSourceFolderPicker { state: browser }) =
        settings.auth.modals.current()
    else {
        panic!("hidden AuthSourceFolderPicker must remain stored");
    };
    assert_eq!(browser.list_state.selected().copied(), Some(0));
    assert!(matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountConfirm { .. })
    ));
    assert!(mounts_confirm_over_auth_browser.drain_effects().is_empty());
}

#[test]
fn settings_env_owner_scrolls_its_picker_without_auth_file_browser_fallback() {
    let term = term_120x40();
    let wheel = mouse_kind_at(MouseEventKind::ScrollDown, 60, 20);
    let browser_root = tempfile::tempdir().expect("browser root");
    let hidden_auth_browser = file_browser_with_dirs(browser_root.path(), 8);
    let mut env_over_auth_browser = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Environments;
    settings.env.modals.open(SettingsModal::EnvRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    settings
        .auth
        .modals
        .open(SettingsModal::AuthSourceFolderPicker {
            state: hidden_auth_browser,
        });
    env_over_auth_browser.stage = ManagerStage::Settings(settings);
    assert!(matches!(
        env_over_auth_browser.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::SettingsEnvDialog
        )
    ));
    handle_mouse(&mut env_over_auth_browser, wheel, term);
    let ManagerStage::Settings(settings) = &env_over_auth_browser.stage else {
        panic!("Settings stage expected");
    };
    let Some(SettingsModal::EnvRolePicker { state: picker }) = settings.env.modals.current() else {
        panic!("visible EnvRolePicker must remain stored");
    };
    assert_eq!(picker.list_state.selected().copied(), Some(1));
    let Some(SettingsModal::AuthSourceFolderPicker { state: browser }) =
        settings.auth.modals.current()
    else {
        panic!("hidden AuthSourceFolderPicker must remain stored");
    };
    assert_eq!(browser.list_state.selected().copied(), Some(0));
    assert!(env_over_auth_browser.drain_effects().is_empty());
}

#[test]
fn help_dismissal_restores_usage_without_replaying_the_key_to_a_picker() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut state = list_state();
    let mut config = jackin_config::AppConfig::default();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());
    state.inline_new_session_picker = Some((
        "container".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
        ]),
        Vec::new(),
    ));
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    let usage_before_dismissal = state.usage.screen.clone();
    let manager_selection_before_dismissal = state.selected;

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("help dismissal");
    assert!(state.keyboard_help.is_none());
    assert!(state.usage.visible, "Esc dismisses Help only");
    assert_eq!(state.usage.screen, usage_before_dismissal);
    assert_eq!(state.selected, manager_selection_before_dismissal);
    assert!(state.inline_new_session_picker.is_some());
    assert!(state.drain_effects().is_empty());

    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("Help q dismissal");
    assert!(state.keyboard_help.is_none());
    assert!(state.usage.visible, "q dismisses Help only");
    assert_eq!(state.usage.screen, usage_before_dismissal);
    assert_eq!(state.selected, manager_selection_before_dismissal);
    assert!(state.inline_new_session_picker.is_some());
    assert!(state.drain_effects().is_empty());

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("Usage key after Help dismissal");
    assert!(
        state.usage.screen.as_ref().expect("Usage screen").detail,
        "the next key reaches the visible Usage screen"
    );
    assert!(
        state.inline_new_session_picker.is_some(),
        "hidden picker remains preserved"
    );
    assert!(
        state.list_modal.is_none(),
        "hidden Enter cannot open a picker error"
    );
    assert!(state.drain_effects().is_empty());
}

#[test]
fn status_popup_consumes_keys_without_dismissing_or_replaying_them() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut state = list_state();
    let mut config = jackin_config::AppConfig::default();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());
    state.status_overlay = Some(StatusPopupState::new("Working", "Please wait"));
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    let usage_before_status = state.usage.screen.clone();

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("Help dismissal above Status");
    assert!(state.keyboard_help.is_none());
    assert!(
        state.status_overlay.is_some(),
        "Help closes without clearing Status"
    );
    assert_eq!(state.usage.screen, usage_before_status);

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("busy status owner consumes Enter");
    assert!(state.status_overlay.is_some());
    assert_eq!(state.usage.screen, usage_before_status);
    assert!(state.drain_effects().is_empty());

    state.status_overlay = None;
    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("Usage receives the next key after status clears");
    assert!(
        state.usage.screen.as_ref().expect("Usage screen").detail,
        "the key consumed by Status is not replayed after it clears"
    );
}

#[test]
fn painted_list_modal_keeps_its_copy_handler_above_usage() {
    let mut state = list_state();
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());
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
    assert_eq!(
        state.painted_input_owner(),
        crate::tui::model::ConsolePaintedInputOwner::Dispatch(
            crate::tui::model::ConsoleInputDispatchPlan::ListModal
        )
    );
    let term = term_120x40();
    let copy_click = (0..term.height)
        .flat_map(|row| (0..term.width).map(move |column| mouse_down_at(column, row)))
        .find(|event| container_info_copyable_row_at(&state, *event, term))
        .expect("container info fixture has one copyable row");

    handle_mouse(&mut state, copy_click, term);

    assert!(matches!(
        state.drain_effects().as_slice(),
        [ManagerEffect::CopyContainerInfoValue { payload, .. }] if payload == "run-123"
    ));
}

#[test]
fn stale_usage_does_not_replace_workflows_key_ownership() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut state = list_state();
    let mut config = jackin_config::AppConfig::default();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    state.stage =
        ManagerStage::Workflows(crate::tui::screens::workflows::model::WorkflowScreen::default());
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());
    state.inline_new_session_picker = Some((
        "container".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
        ]),
        Vec::new(),
    ));

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .expect("Workflows key");

    assert_eq!(
        state.stage.route(),
        crate::tui::model::ConsoleManagerStageRoute::List
    );
    assert!(state.usage.visible, "the stale Usage flag is preserved");
    assert!(state.inline_new_session_picker.is_some());
}

fn workflow_summary() -> jackin_workflow::RunSummary {
    jackin_workflow::RunSummary {
        run_id: "run-owner-test".to_owned(),
        plan_hash: "plan".to_owned(),
        status: "active".to_owned(),
        stage: "implement".to_owned(),
        round: 0,
        candidate: None,
        candidate_generation: 0,
        version: 0,
        owner_epoch: 1,
        pause_kind: None,
        max_repair_turns: 3,
    }
}

fn workflow_detail() -> jackin_workflow::RunDetail {
    jackin_workflow::RunDetail {
        summary: workflow_summary(),
        sessions: Vec::new(),
        effects: Vec::new(),
        events: Vec::new(),
        evidence: Vec::new(),
        checkpoints: Vec::new(),
    }
}

#[test]
fn help_and_workflow_confirmation_keep_the_current_effect_gate() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut state = list_state();
    let mut config = jackin_config::AppConfig::default();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let mut workflow =
        crate::tui::screens::workflows::model::WorkflowScreen::new(vec![workflow_summary()]);
    workflow.set_detail(workflow_detail());
    let requested = crate::tui::screens::workflows::update::apply_workflows_message(
        &mut workflow,
        crate::tui::screens::workflows::message::WorkflowsMessage::RequestAction(
            crate::tui::screens::workflows::model::WorkflowAction::Cancel,
        ),
    );
    assert!(requested.is_empty());
    state.stage = ManagerStage::Workflows(workflow);
    state.usage.visible = true;
    state.usage.screen = Some(crate::tui::state::UsageScreenState::default());
    state.inline_new_session_picker = Some((
        "container".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Claude,
        ]),
        Vec::new(),
    ));
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Char('y')),
        &|_, _| Ok(()),
    )
    .expect("covered confirmation key");
    let ManagerStage::Workflows(screen) = &state.stage else {
        panic!("Help must preserve the Workflows route");
    };
    assert_eq!(
        screen.confirm(),
        Some(crate::tui::screens::workflows::model::WorkflowAction::Cancel)
    );
    assert!(state.drain_effects().is_empty());

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Esc),
        &|_, _| Ok(()),
    )
    .expect("Help dismissal");
    assert!(state.keyboard_help.is_none());
    let ManagerStage::Workflows(screen) = &state.stage else {
        panic!("Help dismissal must not change the Workflows route");
    };
    assert_eq!(
        screen.confirm(),
        Some(crate::tui::screens::workflows::model::WorkflowAction::Cancel)
    );

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Char('q')),
        &|_, _| Ok(()),
    )
    .expect("inert confirmation key");
    let ManagerStage::Workflows(screen) = &state.stage else {
        panic!("confirmation must retain the Workflows route");
    };
    assert_eq!(
        screen.confirm(),
        Some(crate::tui::screens::workflows::model::WorkflowAction::Cancel)
    );
    assert!(state.drain_effects().is_empty());

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Esc),
        &|_, _| Ok(()),
    )
    .expect("confirmation cancel");
    let ManagerStage::Workflows(screen) = &state.stage else {
        panic!("confirmation cancel must retain the Workflows route");
    };
    assert!(screen.confirm().is_none());
    assert!(state.drain_effects().is_empty());

    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Char('c')),
        &|_, _| Ok(()),
    )
    .expect("request confirmation again");
    crate::tui::input::dispatch::handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Char('y')),
        &|_, _| Ok(()),
    )
    .expect("confirm current workflow action");
    assert!(matches!(
        state.drain_effects().as_slice(),
        [ManagerEffect::Workflows(
            crate::tui::screens::workflows::effect::WorkflowsEffect::Cancel { run_id }
        )] if run_id == "run-owner-test"
    ));
}
