// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mouse drag-resize tests for the console TUI.
//! Unit tests for `handle_mouse`: the list/details seam is a
//! mouse-draggable resize affordance driven entirely from `ManagerState`.
//! These build `MouseEvent` values directly and bypass the ratatui
//! event loop — enough to pin the seam hit-test + drag math without a
//! real terminal.
use super::{handle_mouse, handle_mouse_with_config, list_scroll_areas};
use crate::tui::auth::AuthKind;
use crate::tui::components::save_discard::editor_exit_save_discard_state;
use crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP;
use crate::tui::screens::settings::view::global_mount_confirm_state;
use crate::tui::state::ManagerEffect;
use crate::tui::state::{
    DEFAULT_SPLIT_PCT, EditorHoverTarget, EditorState, EditorTab, FieldFocus, GlobalMountConfirm,
    MAX_SPLIT_PCT, MIN_SPLIT_PCT, ManagerHoverTarget, ManagerListRow, ManagerStage, ManagerState,
    Modal, MountScrollFocus, SecretsScopeTag, SettingsHoverTarget, SettingsModal, SettingsState,
    SettingsTab, SettingsTrustRow,
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use jackin_config::{AgentAuthConfig, AuthForwardMode};
use jackin_config::{MountConfig, WorkspaceConfig};
use ratatui::layout::Rect;

/// Build a `ManagerState` in the List stage at the default split,
/// with no workspaces and no modal.
fn list_state() -> ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    ManagerState::from_config(&config, tmp.path())
}

fn file_browser_with_dirs(
    root: &std::path::Path,
    count: usize,
) -> crate::tui::components::file_browser::FileBrowserState {
    for i in 0..count {
        std::fs::create_dir_all(root.join(format!("dir-{i}"))).unwrap();
    }
    crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_at(root.to_path_buf(), root.to_path_buf()),
    )
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// The mouse content-area helpers must subtract the renderer's cached
/// dynamic footer height, so a click in the footer never maps to content
/// (a footer-height of 2 was hard-coded while the renderer went dynamic).
#[test]
fn content_areas_exclude_the_cached_footer() {
    use super::{SCREEN_HEADER_HEIGHT, TAB_STRIP_HEIGHT};
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

/// Build a `MouseEvent` at column `col`, row 0.
const fn mouse(kind: MouseEventKind, col: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

/// A 100-col-wide terminal area.
const fn term(width: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height: 30,
    }
}

#[test]
fn mouse_down_on_seam_starts_drag() {
    // Default split on a 100-col terminal => seam at column
    // `DEFAULT_SPLIT_PCT`.
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
    // Anchor at DEFAULT_SPLIT_PCT. Drag +10 columns on a 100-col
    // terminal ⇒ +10%.
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
    // Drag far left ⇒ clamp to MIN_SPLIT_PCT.
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

    // Drag far right ⇒ clamp to MAX_SPLIT_PCT.
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
    // Clicks in the middle of either pane must be ignored — the
    // operator's intent is "click a row/button", not "start a resize".
    let mut state = list_state();
    // Seam at column `DEFAULT_SPLIT_PCT`; columns near either border
    // are far enough from the seam to be rejected.
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
    // GithubPicker is the only list-level modal today. Any mouse event
    // while it's up must be a silent no-op — the picker owns the
    // keyboard + (implicitly) the mouse focus.
    let mut state = list_state();
    // Use the github_mounts resolver indirectly — easier to
    // just synthesize a GithubPicker state with an arbitrary choice.
    // The picker's exact contents don't matter; only `list_modal.is_some()`.
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
    // Ensure the helper signature compiles (guards against future refactors).
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
        state: crate::tui::components::github_picker::GithubPickerState::new(vec![
            crate::github_mounts::GithubChoice {
                src: "/one".into(),
                branch: "main".into(),
                url: "https://github.com/o/one".into(),
            },
            crate::github_mounts::GithubChoice {
                src: "/two".into(),
                branch: "main".into(),
                url: "https://github.com/o/two".into(),
            },
        ]),
    });

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 60, 20),
        term_120x40(),
    );

    let Some(Modal::GithubPicker { state: picker }) = &state.list_modal else {
        panic!("github picker modal expected");
    };
    assert_eq!(picker.list_state.selected, Some(1));
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

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 60, 20),
        term_120x40(),
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0, "background editor must not scroll");
    let Some(Modal::WorkdirPick { state: picker }) = &editor.modal else {
        panic!("workdir picker modal expected");
    };
    assert_eq!(picker.list_state.selected, Some(1));
}

#[test]
fn settings_role_picker_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.mounts.scroll_y = 4;
    settings.mounts.modal = Some(SettingsModal::MountRolePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap(),
            jackin_core::RoleSelector::parse("scentbird/agent-jones").unwrap(),
        ]),
    });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 60, 20),
        term_120x40(),
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(
        settings.mounts.scroll_y, 4,
        "background settings must not scroll"
    );
    let Some(SettingsModal::MountRolePicker { state: picker }) = &settings.mounts.modal else {
        panic!("settings role picker modal expected");
    };
    assert_eq!(picker.list_state.selected, Some(1));
}

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

// ── File-browser URL-click integration ─────────────────────────────
//
// When a FileBrowser modal with a git-prompt + resolved URL is open
// during the Editor or CreatePrelude stages, Down(Left) on the URL
// row must be consumed into a typed open-URL outcome for the run loop —
// observable state effect: the drag-anchor never latches.

/// Term of 120x40 ⇒ `FileBrowser` modal at (18, 9, 84, 22); URL row at
/// y = 17, column range ≈ 19..=100. Mirrors the reference geometry
/// used in `file_browser::tests::manufactured_modal_area`.
fn term_120x40() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    }
}

/// Mouse event at `(col, row)`, left-button Down.
const fn mouse_down_at(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn container_info_copy_click_queues_typed_effect() {
    let mut state = list_state();
    state.list_modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Debug info",
            vec![
                termrock::components::ContainerInfoRow::new("Run ID", "run-123")
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
            if super::container_info_copyable_row_at(&state, mouse, term) {
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
fn click_on_editor_auth_preview_row_does_not_focus_or_activate() {
    let mut state = list_state();
    let mut config = jackin_config::AppConfig::default();
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(std::path::PathBuf::from("/host/claude")),
        }),
        ..Default::default()
    };
    config.workspaces.insert("x".into(), ws.clone());
    let mut editor = EditorState::new_edit("x".into(), ws);
    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(AuthKind::Claude);
    editor.active_field = FieldFocus::Row(0);
    let row_idx = editor
        .auth_flat_rows(&config)
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::tui::state::AuthRow::WorkspaceSourceFolder {
                    kind: AuthKind::Claude
                }
            )
        })
        .expect("sync mode must render a source-folder preview row");
    let area = editor.content_area(term(100));
    let click_row = area
        .y
        .saturating_add(1)
        .saturating_add(u16::try_from(row_idx).unwrap());
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(
            MouseEventKind::Down(MouseButton::Left),
            area.x.saturating_add(2),
            click_row,
        ),
        term(100),
        Some(&config),
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert!(editor.modal.is_none());
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
            if super::file_browser_url_row_at(&state, mouse, term) {
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
    assert!(matches!(outcome, super::super::InputOutcome::Continue));
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

// ── Click-to-select tests ──────────────────────────────────────
//
// Layout (100x30 terminal, header=2 footer=2 body=26):
//   y = 0       → header brand pill (chunks[0])
//   y = 1       → header spacer row
//   y = 2       → body top border (list block)
//   y = 3       → list item 0 ("Current directory")
//   y = 4       → list item 1 (first saved workspace)
//   ...
//   y = 27      → body bottom border
//   y = 28..=29 → footer (chunks[2])
//
// Left pane (default split = DEFAULT_SPLIT_PCT%): x = 0..=(seam-1)
// with x=0 = left border and x=seam-1 inclusive = last interior col.
// The seam column itself is the drag-handle.

/// Mouse event at `(col, row)`, left-button Down.
const fn mouse_at(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

const fn mouse_kind_at(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Build a list state with `n` saved workspaces (row 0 + n + spacer + sentinel).
fn list_state_with_saved(n: usize) -> ManagerState<'static> {
    let mut config = jackin_config::AppConfig::default();
    for i in 0..n {
        config.workspaces.insert(
            format!("ws-{i:02}"),
            WorkspaceConfig {
                workdir: format!("/w/{i}"),
                mounts: vec![],
                ..Default::default()
            },
        );
    }
    let tmp = tempfile::tempdir().unwrap();
    ManagerState::from_config(&config, tmp.path())
}

fn config_with_scrollable_workspace_and_global_mounts() -> jackin_config::AppConfig {
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: "/workspace/demo".into(),
                mounts: vec![MountConfig {
                    src: "/host/source/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    dst: "/container/destination/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    readonly: false,
                    isolation: jackin_config::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
    config.add_mount(
        "global-long",
        MountConfig {
            src: "/host/source/with/a/very/long/path/that/forces/global/mount/scrolling".into(),
            dst: "/container/destination/with/a/very/long/path/that/forces/global/mount/scrolling"
                .into(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        },
        None,
    );
    config
}

fn selected_demo_state(config: &jackin_config::AppConfig) -> ManagerState<'static> {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(config, tmp.path());
    state.selected = 1;
    state
}

fn current_dir_state_at(path: &std::path::Path) -> ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    ManagerState::from_config(&config, path)
}

fn config_with_long_git_type_mount(source: &std::path::Path) -> jackin_config::AppConfig {
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/workspace/demo".into(),
            mounts: vec![MountConfig {
                src: source.display().to_string(),
                dst: source.display().to_string(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    config
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

    assert_eq!(state.list_mounts_scroll_x, MOUSE_HORIZONTAL_SCROLL_STEP);
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

    assert_eq!(state.list_mounts_scroll_x, 0);
    assert_eq!(
        state.list_global_mounts_scroll_x,
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
        state.list_global_mounts_scroll_x, 0,
        "ScrollDown must not change horizontal scroll on a horizontal-only block"
    );

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(state.list_global_mounts_scroll_x, 0);
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
    assert_eq!(state.list_mounts_scroll_y, 0);
    assert_eq!(state.list_global_mounts_scroll_y, 1);
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
    let expected_max = super::max_scroll_offset(
        super::global_mounts_content_width(global_mounts.as_slice()),
        super::scroll_viewport_width(global_area),
    );
    assert_eq!(state.list_global_mounts_scroll_x, expected_max);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollLeft, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_global_mounts_scroll_x,
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
    let expected_max = super::max_scroll_offset(
        super::workspace_mounts_content_width(workspace.mounts.as_slice()),
        super::scroll_viewport_width(workspace_area),
    );

    assert_eq!(
        state.list_mounts_scroll_x, expected_max,
        "mouse/touch scroll must clamp at the same rendered width keyboard scrolling reaches"
    );
}

#[test]
fn horizontal_mouse_wheel_clamps_before_applying_left_delta() {
    let config = config_with_scrollable_workspace_and_global_mounts();
    let mut state = selected_demo_state(&config);
    state.list_global_mounts_scroll_x = u16::MAX;

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
    let expected_max = super::max_scroll_offset(
        super::global_mounts_content_width(global_mounts.as_slice()),
        super::scroll_viewport_width(global_area),
    );

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollLeft, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(
        state.list_global_mounts_scroll_x,
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
        editor.workspace_mounts_scroll_x,
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
        editor.workspace_mounts_scroll_x,
        MOUSE_HORIZONTAL_SCROLL_STEP
    );
}

#[test]
fn editor_non_mounts_tab_click_focuses_horizontal_scroll_block() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_width = 80;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(&mut state, mouse_at(10, 6), term(42), None);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(editor.tab_content_scroll_focused());

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
        term(42),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_x, MOUSE_HORIZONTAL_SCROLL_STEP);
    assert!(editor.tab_content_scroll_focused());
}

#[test]
fn editor_vertical_wheel_scrolls_only_inside_content_area() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 1),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 1);
}

#[test]
fn editor_general_tab_vertical_wheel_uses_shared_scroll_path() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::General;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        Rect::new(0, 0, 100, 9),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll_y, 1,
        "General must use the same vertical wheel path as every editor tab"
    );
}

#[test]
fn editor_general_tab_vertical_scrollbar_drag_uses_shared_scroll_path() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::General;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), 99, 7),
        Rect::new(0, 0, 100, 10),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        editor.tab_scroll_y > 0,
        "General scrollbar dragging must use the same vertical path as every editor tab"
    );
}

#[test]
fn editor_vertical_wheel_ignores_background_when_modal_open() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::SaveDiscardCancel {
        state: editor_exit_save_discard_state(),
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        term(100),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0);
}

#[test]
fn editor_file_browser_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0, "background editor must not scroll");
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(1));
}

#[test]
fn editor_file_browser_smoke_hints_pagedown_and_wheel_share_modal_context() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 10);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    let hints = format!(
        "{:?}",
        crate::tui::components::footer_hints::editor_footer_items(
            editor,
            &config,
            false,
            Rect::new(0, 0, 120, 40),
        )
    );
    assert!(
        hints.contains(termrock::keymap::glyph::PGUP_PGDN),
        "footer hints missing page keys: {hints}"
    );

    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("editor stage expected");
    };
    let Some(Modal::FileBrowser { state: fb, .. }) = &mut editor.modal else {
        panic!("file browser modal expected");
    };
    drop(fb.handle_key_with_page_rows(key(KeyCode::PageDown), Some(4)));
    assert_eq!(fb.list_state.selected, Some(4));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        Some(&config),
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0, "background editor must not scroll");
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(5));
}

#[test]
fn create_prelude_file_browser_wheel_scrolls_modal_selection() {
    use crate::tui::state::CreatePreludeState;

    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    state.stage = ManagerStage::CreatePrelude(CreatePreludeState {
        modal: Some(Modal::FileBrowser {
            target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        }),
        ..CreatePreludeState::default()
    });

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::CreatePrelude(prelude) = &state.stage else {
        panic!("create prelude stage expected");
    };
    let Some(Modal::FileBrowser { state: fb, .. }) = &prelude.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(1));
}

#[test]
fn settings_mounts_file_browser_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.mounts.scroll_y = 4;
    settings.mounts.modal = Some(SettingsModal::MountFileBrowser {
        state: Box::new(fb),
    });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(
        settings.mounts.scroll_y, 4,
        "background settings must not scroll"
    );
    let Some(SettingsModal::MountFileBrowser { state: fb }) = &settings.mounts.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(1));
}

#[test]
fn settings_auth_source_folder_wheel_scrolls_modal_selection() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.auth.modal = Some(SettingsModal::AuthSourceFolderPicker { state: fb });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    let Some(SettingsModal::AuthSourceFolderPicker { state: fb }) = &settings.auth.modal else {
        panic!("source-folder file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(1));
}

#[test]
fn file_browser_wheel_at_edge_is_consumed_before_background_scroll() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll_y, 0,
        "saturated modal wheel must not leak"
    );
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected, Some(0));
}

#[test]
fn editor_vertical_scrollbar_drag_ignores_background_when_modal_open() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::SaveDiscardCancel {
        state: editor_exit_save_discard_state(),
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), 99, 7),
        term(100),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll_y, 0);
}

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
    settings.mounts.modal = Some(SettingsModal::MountConfirm {
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
    assert_eq!(settings.mounts.scroll_y, 0);
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
    state.list_global_mounts_scroll_y = 3;

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 31, 11),
        term(100),
        Some(&config),
    );

    assert_eq!(state.list_global_mounts_scroll_y, 0);
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
