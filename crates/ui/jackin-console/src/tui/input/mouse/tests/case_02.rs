// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn drag_ignored_on_non_list_stage() {
    // While in the Editor (or any non-List stage), mouse events are
    // ignored outright — no seam to drag.
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
    // Terminals narrower than MIN_DRAGGABLE_WIDTH skip hit-testing
    // entirely — below that the clamp bounds already leave the right
    // pane implausibly small.
    let mut state = list_state();
    // 30-col terminal is below the 40-col threshold.
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
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

    // Rendered tab spans start at x=0:
    // " General " (0..9), space, " Mounts " (10..18), space,
    // " Roles " (19..26), space, " Environments " (27..41).
    handle_mouse(&mut state, mouse_down_at(33, 3), term(100));

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::Secrets);
    assert!(matches!(editor.active_field, FieldFocus::Row(0)));
}

#[test]
fn mouse_motion_sets_and_clears_editor_tab_hover() {
    let mut state = list_state();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![],
        ..Default::default()
    };
    state.stage = ManagerStage::Editor(EditorState::new_edit("x".into(), ws));

    // Motion inside " Roles " (cols 19..26 on the strip row) highlights the
    // third cell without changing the active tab.
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 22, 3),
        term(100),
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.hovered_tab(), Some(2));
    assert_eq!(editor.hover_target, Some(EditorHoverTarget::Tab(2)));
    assert_eq!(editor.active_tab, EditorTab::General);

    // Motion off the strip (header row) clears the highlight.
    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 22, 0),
        term(100),
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
    settings.trust.pending = vec![SettingsTrustRow {
        role: "agent-smith".into(),
        git: "/repo".into(),
        trusted: true,
    }];
    state.stage = ManagerStage::Settings(settings);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 10, 7),
        term(100),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(
        settings.hover_target,
        Some(SettingsHoverTarget::TrustRow(0))
    );
    assert_eq!(settings.hovered_trust_row(), Some(0));

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::Moved, 10, 0),
        term(100),
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.hover_target, None);
}

#[test]
fn mouse_down_on_editor_tab_clears_secrets_view_when_leaving() {
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

    handle_mouse(&mut state, mouse_down_at(3, 3), term(100));

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

    // Build a FileBrowser at `parent`, select the repo, open git prompt,
    // and inject a URL so the URL row renders.
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
    // No drag latched — URL click is consumed before the seam path.
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

    // Row 0 is well outside the URL row (17) and the modal entirely.
    handle_mouse(&mut state, mouse_down_at(60, 0), term_120x40());
    // CreatePrelude is not the List stage, so the list-drag path is
    // also inert — no drag latched regardless of the URL branch.
    assert!(state.drag_state.is_none());
}

#[test]
fn click_on_first_row_sets_selected_to_zero() {
    // y=3 = first list item (index 0, "Current directory").
    let mut state = list_state_with_saved(3);
    state.selected = 2;
    handle_mouse(&mut state, mouse_at(10, 3), term(100));
    assert_eq!(state.selected, 0);
}

#[test]
fn click_on_fifth_row_sets_selected_to_four() {
    // y=7 = fifth list row (index 4). Needs enough saved workspaces
    // to make index 4 a valid selection target.
    let mut state = list_state_with_saved(5);
    state.selected = 0;
    handle_mouse(&mut state, mouse_at(10, 7), term(100));
    assert_eq!(state.selected, 4);
}
