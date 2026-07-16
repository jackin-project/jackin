// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `editor` input handlers.
//! Editor-stage tests: tab cycling, modal dispatch, role allow/default
//! bindings, and mount-row readonly toggle.
use super::super::test_support::{key, mount};
use super::{
    EditorModalOutcome, apply_file_browser_to_editor, apply_text_input_to_pending,
    env_key_input_state, handle_editor_modal, poll_role_load, role_load_input_state,
    secret_new_key_label,
};
use crate::console::tui::input::handle_key;
use crate::console::tui::state::{
    AuthRow, ConfirmTarget, EditorState, EditorTab, FieldFocus, FileBrowserTarget, ManagerStage,
    ManagerState, Modal, PendingRoleLoad, SecretsRow, SecretsScopeTag, TextInputTarget,
};
use crossterm::event::KeyCode;
use jackin_config::{AgentAuthConfig, AppConfig, AuthForwardMode};
use jackin_config::{MountConfig, WorkspaceConfig};
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use jackin_env::OpCache;
use jackin_manifest::repo::CachedRepo;
use jackin_test_support::{FakeRunner, first_temp_role_repo, seed_valid_role_repo};
use ratatui::layout::Rect;
use tempfile::TempDir;

/// Test helper: invoke `handle_editor_modal` with default plumbing
/// for the new `op_available` / `op_cache` parameters. Existing
/// editor-modal tests don't exercise the `SourcePicker` /
/// `OpPicker` branches that need real wiring; defaults are fine.
fn handle_modal(editor: &mut EditorState<'_>, k: crossterm::event::KeyEvent) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    handle_modal_with(editor, k, &mut config, &paths);
}

fn handle_modal_with(
    editor: &mut EditorState<'_>,
    k: crossterm::event::KeyEvent,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    let outcome = handle_editor_modal(
        editor,
        k,
        false,
        std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
        config,
        paths,
        Rect::new(0, 0, 120, 40),
    );
    match outcome {
        EditorModalOutcome::PersistTrustedRoleSource { key, mut source } => {
            source.trusted = true;
            crate::console::effects::persist_trusted_role_source_for_tests(
                editor, config, paths, &key, &source,
            );
        }
        EditorModalOutcome::OpenUrl(_) => panic!("test helper did not expect URL-open"),
        _ => {}
    }
}

/// Test helper: invoke `apply_text_input_to_pending` with
/// `op_available = false`. Tests that don't open the `SourcePicker`
/// don't care about the flag.
fn apply_text_input(target: &TextInputTarget, editor: &mut EditorState<'_>, value: &str) {
    apply_text_input_to_pending(target, editor, value, false);
}

fn empty_ws() -> WorkspaceConfig {
    WorkspaceConfig::default()
}

fn config_with_agents(names: &[&str]) -> AppConfig {
    use jackin_config::test_support::config_with_agents as make_config;
    let mut config = make_config(names);
    for name in names {
        if let Some(role) = config.roles.get_mut(*name) {
            role.git = format!("https://example.test/{name}.git");
        }
    }
    config.workspaces.insert("ws".into(), empty_ws());
    config
}

fn seed_first_temp_valid_role_repo(data_dir: &std::path::Path) {
    seed_valid_role_repo(&first_temp_role_repo(data_dir));
}

fn editor_on_agents_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(row);
    state.stage = ManagerStage::Editor(editor);
    state
}

fn editor_on_mounts_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(row);
    state.stage = ManagerStage::Editor(editor);
    state
}

fn ws_with_one_mount(readonly: bool) -> WorkspaceConfig {
    WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    }
}

fn press(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    code: KeyCode,
) -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs()?;
    handle_key(state, config, &paths, tmp.path(), key(code))?;
    Ok(())
}

fn pending_allowed(state: &ManagerState<'_>) -> Vec<String> {
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    e.pending.allowed_roles.clone()
}

/// Build an editor sitting on the Mounts tab with an empty mount list,
/// and simulate the commit of a `FileBrowser` at `/host/path`. The bridge
/// function is `apply_file_browser_to_editor`, which opens the new
/// `MountDstChoice` modal instead of the old "push + `TextInput`" chain.
fn editor_with_browser_committed(src: &str) -> EditorState<'static> {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from(src),
    );
    editor
}

fn editor_with_file_browser_parent_committed(src: &str) -> EditorState<'static> {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: jackin_console::tui::components::file_browser::FileBrowserState::from_listing(
            jackin_console::services::file_browser::listing_from_home().unwrap(),
        ),
    });
    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from(src),
    );
    editor
}

/// Build a minimal `(ManagerState, AppConfig, JackinPaths, TempDir)` with
/// the state stage parked in an Editor on the given `start_tab`. Used
/// to drive `handle_key` through `handle_editor_key`'s tab-cycle branch.
fn editor_state_on_tab(
    start_tab: EditorTab,
) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = start_tab;
    editor.set_tab_bar_focused(false);
    state.stage = ManagerStage::Editor(editor);
    (state, config, paths, tmp)
}

// ── Editor: rename modal entry on the name row ────────────────────

#[test]
fn create_mode_enter_on_name_row_opens_rename_modal() {
    // In Create mode, pressing Enter on row 0 (Name) must open the
    // rename TextInput modal pre-filled with the current pending_name
    // — the same flow Edit mode uses. This is the operator's escape
    // hatch from a prelude-captured name they mistyped.
    let (tmp, paths, mut config) = {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let toml = toml::to_string(&config).unwrap();
        std::fs::write(&paths.config_file, toml).unwrap();
        let loaded = AppConfig::load_or_init(&paths).unwrap();
        (tmp, paths, loaded)
    };
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("typo-name".into());
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("still in editor after Enter on name row");
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Name);
            assert_eq!(
                state.value(),
                "typo-name",
                "TextInput must be pre-filled with current pending_name"
            );
        }
        other => panic!("expected TextInput(Name); got {other:?}"),
    }
}

#[test]
fn create_mode_rename_commit_updates_pending_name() {
    // After the TextInput commits a new value, pending_name should
    // reflect the operator's edit. Same code path as Edit mode —
    // apply_text_input_to_pending doesn't distinguish modes.
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("old-name".into());

    apply_text_input(&TextInputTarget::Name, &mut editor, "new-name");

    assert_eq!(editor.pending_name.as_deref(), Some("new-name"));
}

#[test]
fn edit_mode_enter_on_name_row_still_opens_rename_modal() {
    // Regression guard: the Create-mode extension to row 0 Enter must
    // not break the Edit-mode path that already worked.
    let ws = WorkspaceConfig {
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert("keep-me".into(), ws.clone());
    let toml = toml::to_string(&config).unwrap();
    std::fs::write(&paths.config_file, toml).unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("keep-me".into(), ws);
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Name);
            assert_eq!(state.value(), "keep-me");
        }
        other => panic!("expected TextInput(Name); got {other:?}"),
    }
}

fn auth_cursor_rows() -> Vec<AuthRow> {
    vec![
        AuthRow::WorkspaceMode {
            kind: AuthKind::Claude,
        },
        AuthRow::WorkspaceSource {
            kind: AuthKind::Claude,
        },
        AuthRow::WorkspaceSourceFolder {
            kind: AuthKind::Claude,
        },
        AuthRow::Spacer,
        AuthRow::RoleHeader {
            role: "smith".into(),
            expanded: true,
        },
        AuthRow::RoleMode {
            role: "smith".into(),
            kind: AuthKind::Claude,
        },
        AuthRow::RoleSource {
            role: "smith".into(),
            kind: AuthKind::Claude,
        },
        AuthRow::RoleSourceFolder {
            role: "smith".into(),
            kind: AuthKind::Claude,
        },
        AuthRow::Spacer,
        AuthRow::AddSentinel { eligible: 1 },
    ]
}

#[test]
fn down_skips_workspace_preview_rows_and_spacer() {
    let rows = auth_cursor_rows();
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_down(&skipped, 1, rows.len() - 1),
        4
    );
}

#[test]
fn down_skips_role_preview_rows_and_spacer() {
    let rows = auth_cursor_rows();
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_down(&skipped, 6, rows.len() - 1),
        9
    );
}

#[test]
fn down_at_max_with_only_preview_remaining_returns_candidate() {
    let rows = vec![
        AuthRow::WorkspaceMode {
            kind: AuthKind::Claude,
        },
        AuthRow::WorkspaceSourceFolder {
            kind: AuthKind::Claude,
        },
    ];
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_down(&skipped, 1, 1),
        1
    );
}

#[test]
fn up_skips_workspace_preview_rows_to_workspace_mode() {
    let rows = auth_cursor_rows();
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_up(&skipped, 3),
        0
    );
}

#[test]
fn up_skips_role_preview_rows_to_role_mode() {
    let rows = auth_cursor_rows();
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_up(&skipped, 7),
        5
    );
}

#[test]
fn up_at_zero_preview_clamps_to_zero() {
    let rows = vec![
        AuthRow::WorkspaceSourceFolder {
            kind: AuthKind::Claude,
        },
        AuthRow::WorkspaceMode {
            kind: AuthKind::Claude,
        },
    ];
    let skipped = jackin_console::tui::screens::editor::update::auth_skipped_rows(&rows);
    assert_eq!(
        jackin_console::tui::screens::editor::update::step_cursor_up(&skipped, 0),
        0
    );
}

#[test]
fn enter_on_auth_workspace_source_preview_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let workspace = WorkspaceConfig {
        workdir: tmp.path().display().to_string(),
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };
    config.workspaces.insert("proj".into(), workspace.clone());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("proj".into(), workspace);
    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let source_idx = editor
        .auth_flat_rows(&config)
        .iter()
        .position(|row| {
            matches!(
                row,
                AuthRow::WorkspaceSource {
                    kind: AuthKind::Claude
                }
            )
        })
        .expect("api_key mode must render a workspace source preview row");
    editor.active_field = FieldFocus::Row(source_idx);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("still in editor after Enter on source preview row");
    };
    assert!(editor.modal.is_none());
    assert_eq!(editor.active_field, FieldFocus::Row(source_idx));
}

#[test]
fn enter_on_auth_workspace_source_folder_preview_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let workspace = WorkspaceConfig {
        workdir: tmp.path().display().to_string(),
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(std::path::PathBuf::from("/host/claude")),
        }),
        ..Default::default()
    };
    config.workspaces.insert("proj".into(), workspace.clone());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("proj".into(), workspace);
    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(AuthKind::Claude);
    let source_folder_idx = editor
        .auth_flat_rows(&config)
        .iter()
        .position(|row| {
            matches!(
                row,
                AuthRow::WorkspaceSourceFolder {
                    kind: AuthKind::Claude
                }
            )
        })
        .expect("sync mode must render a workspace source-folder preview row");
    editor.active_field = FieldFocus::Row(source_folder_idx);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("still in editor after Enter on source-folder preview row");
    };
    assert!(editor.modal.is_none());
    assert_eq!(editor.active_field, FieldFocus::Row(source_folder_idx));
}

// ── Editor FileBrowser → MountDstChoice behavioral tests ────────────

#[test]
fn filebrowser_commit_opens_mount_dst_choice_not_text_input() {
    // Pin: the FileBrowser→TextInput chain is replaced by
    // FileBrowser→MountDstChoice. No mount should be pushed yet — the
    // push is deferred to the choice modal's commit handler.
    let editor = editor_with_browser_committed("/host/path");
    assert!(
        matches!(editor.modal, Some(Modal::MountDstChoice { .. })),
        "expected MountDstChoice modal; got {:?}",
        editor.modal
    );
    assert_eq!(
        editor.pending.mounts.len(),
        0,
        "no mount must be pushed until the operator commits in the choice modal"
    );
}

#[test]
fn filebrowser_child_esc_restores_filebrowser_parent() {
    let mut editor = editor_with_file_browser_parent_committed("/host/path");
    assert!(matches!(editor.modal, Some(Modal::MountDstChoice { .. })));
    assert_eq!(editor.modal_parents.len(), 1);

    handle_modal(&mut editor, key(KeyCode::Esc));

    assert!(
        matches!(editor.modal, Some(Modal::FileBrowser { .. })),
        "Esc from MountDstChoice should restore FileBrowser; got {:?}",
        editor.modal
    );
    assert!(editor.modal_parents.is_empty());
}

#[test]
fn filebrowser_open_git_url_returns_typed_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut browser = jackin_console::tui::components::file_browser::FileBrowserState::from_listing(
        jackin_console::services::file_browser::listing_from_home().unwrap(),
    );
    browser.pending_git_prompt = Some(tmp.path().to_path_buf());
    browser.pending_git_url = Some("file:///tmp/editor-url".into());
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: browser,
    });

    let outcome = handle_editor_modal(
        &mut editor,
        key(KeyCode::Char('O')),
        false,
        std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
        &mut config,
        &paths,
        Rect::new(0, 0, 120, 40),
    );

    assert!(matches!(
        outcome,
        EditorModalOutcome::OpenUrl(url) if url == "file:///tmp/editor-url"
    ));
}

#[test]
fn mounts_add_starts_file_browser_listing_worker() {
    let ws = WorkspaceConfig::default();
    let mut state = editor_on_mounts_tab(ws, 0);
    let mut config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('a')),
    )
    .unwrap();
    for effect in state.drain_effects() {
        crate::console::effects::execute_manager_effect(&mut state, &mut config, &paths, effect);
    }

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(
        editor.modal.is_none(),
        "file browser modal should wait for the listing worker; got {:?}",
        editor.modal
    );
    assert!(state.file_browser_listing_in_flight());
}

#[test]
fn o_on_folder_mount_opens_error_popup() {
    // A plain folder mount has no GitHub URL — O must open an ErrorPopup
    // explaining why, not silently do nothing.
    let ws = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/plain-dir".into(),
            dst: "/host/plain-dir".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let mut state = editor_on_mounts_tab(ws, 0);
    let mut config = AppConfig::default();

    press(&mut state, &mut config, KeyCode::Char('o')).unwrap();

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert!(
        matches!(editor.modal, Some(Modal::ErrorPopup { .. })),
        "O on a folder mount must open an ErrorPopup; got {:?}",
        editor.modal,
    );
}

#[test]
fn added_mount_defaults_to_shared_isolation() {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;

    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from("/host/path"),
    );
    handle_modal(&mut editor, key(KeyCode::Char('m')));

    assert_eq!(editor.pending.mounts.len(), 1);
    assert_eq!(
        editor.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Shared
    );
}

#[test]
fn editor_mount_same_path_commits_mount_with_dst_equal_src() {
    // Mount-at-same-path shortcut on the choice modal → push MountConfig with dst = src
    // and close the modal. No TextInput should appear.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('m')));
    assert!(
        editor.modal.is_none(),
        "Mount at same path must close the modal; got {:?}",
        editor.modal
    );
    assert_eq!(editor.pending.mounts.len(), 1, "exactly one mount pushed");
    let m = &editor.pending.mounts[0];
    assert_eq!(m.src, "/host/path");
    assert_eq!(
        m.dst, "/host/path",
        "Mount-at-same-path fast path sets dst = src"
    );
    assert!(!m.readonly);
}

#[test]
fn editor_edit_opens_textinput_and_pushes_provisional() {
    // Edit destination → push provisional mount (dst = src) + open
    // the TextInput pre-filled with src. Mirrors today's flow so the
    // operator can edit dst in place.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('e')));
    match &editor.modal {
        Some(Modal::TextInput { target, .. }) => {
            assert_eq!(target, &TextInputTarget::MountDst);
        }
        other => panic!("expected TextInput(MountDst); got {other:?}"),
    }
    assert_eq!(
        editor.pending.mounts.len(),
        1,
        "provisional mount pushed for the TextInput to mutate"
    );
    let m = &editor.pending.mounts[0];
    assert_eq!(m.src, "/host/path");
    assert_eq!(m.dst, "/host/path", "provisional dst mirrors src");
}

#[test]
fn editor_mount_destination_esc_walks_back_one_step() {
    let mut editor = editor_with_file_browser_parent_committed("/host/path");

    handle_modal(&mut editor, key(KeyCode::Char('e')));
    assert!(matches!(
        editor.modal,
        Some(Modal::TextInput {
            target: TextInputTarget::MountDst,
            ..
        })
    ));
    assert_eq!(editor.modal_parents.len(), 2);

    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(
        matches!(editor.modal, Some(Modal::MountDstChoice { .. })),
        "Esc from destination input should restore MountDstChoice; got {:?}",
        editor.modal
    );

    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(
        matches!(editor.modal, Some(Modal::FileBrowser { .. })),
        "second Esc should restore FileBrowser; got {:?}",
        editor.modal
    );
}

#[test]
fn editor_env_source_picker_esc_restores_key_input() {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    let scope = SecretsScopeTag::Workspace;
    let target = TextInputTarget::EnvKey {
        scope: scope.clone(),
    };
    editor.modal = Some(Modal::TextInput {
        target: target.clone(),
        state: env_key_input_state(&editor, &scope, secret_new_key_label(&scope), "API_KEY"),
    });

    apply_text_input_to_pending(&target, &mut editor, "API_KEY", false);
    assert!(matches!(editor.modal, Some(Modal::SourcePicker { .. })));
    assert_eq!(editor.modal_parents.len(), 1);

    handle_modal(&mut editor, key(KeyCode::Esc));

    assert!(
        matches!(
            editor.modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc from SourcePicker should restore EnvKey input; got {:?}",
        editor.modal
    );
    assert!(editor.modal_parents.is_empty());
}

#[test]
fn editor_cancel_does_not_push_mount() {
    // C / Esc dismisses the choice modal without touching pending.mounts.
    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Esc));
    assert!(editor.modal.is_none(), "Esc closes the modal");
    assert_eq!(
        editor.pending.mounts.len(),
        0,
        "Cancel must not push a mount"
    );

    let mut editor = editor_with_browser_committed("/host/path");
    handle_modal(&mut editor, key(KeyCode::Char('c')));
    assert!(editor.modal.is_none(), "`c` closes the modal");
    assert_eq!(editor.pending.mounts.len(), 0, "`c` must not push a mount");
}

// ── Editor tab navigation: Tab = forward, Left/Right = no-op on non-header rows ─────

#[test]
fn editor_right_arrow_is_noop_on_non_header_row() {
    // Right must not cycle tabs — it is an intra-area horizontal key.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Right),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.active_tab,
        EditorTab::General,
        "Right must not advance tab"
    );
}

#[test]
fn editor_left_arrow_is_noop_on_non_header_row() {
    // Left must not cycle tabs — it is an intra-area horizontal key.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::Mounts);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Left),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(e.active_tab, EditorTab::Mounts, "Left must not rewind tab");
}

// ── Roles tab: `*` default-toggle binding ───────────────────────

#[test]
fn roles_tab_enter_on_load_role_row_opens_role_input() {
    let (tmp, paths, mut config) = {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = config_with_agents(&["agent-smith"]);
        (tmp, paths, config)
    };
    let cwd = tmp.path();
    let mut state = editor_on_agents_tab(empty_ws(), config.roles.len());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    match &e.modal {
        Some(Modal::TextInput { target, state }) => {
            assert_eq!(target, &TextInputTarget::Role);
            assert_eq!(state.label, "Load role");
        }
        other => panic!("expected TextInput(Role); got {other:?}"),
    }
}

#[tokio::test]
async fn role_input_resolves_then_persists_namespaced_role_after_trust() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let selector = jackin_core::RoleSelector::parse("chainargos/agent-brown").unwrap();
    let cached_repo = CachedRepo::new(&paths, &selector);
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;

    assert!(
        runner
            .recorded
            .iter()
            .any(|cmd| cmd
                .contains("git clone https://github.com/chainargos/jackin-agent-brown.git")),
        "role add must clone through the normal repo resolver; got {:?}",
        runner.recorded
    );
    let clone_cmd = runner
        .recorded
        .iter()
        .find(|cmd| cmd.contains("git clone https://github.com/chainargos/jackin-agent-brown.git"))
        .expect("clone command should be recorded");
    assert!(
        clone_cmd.contains(paths.data_dir.to_str().unwrap()),
        "role add should clone into a temp dir under data_dir first: {clone_cmd}"
    );
    assert!(
        !clone_cmd.contains(paths.roles_dir.to_str().unwrap()),
        "role add must not clone directly into the final role cache: {clone_cmd}"
    );
    assert!(
        cached_repo.repo_dir.join("jackin.role.toml").is_file(),
        "validated clone should be moved into the role cache"
    );

    match &editor.modal {
        Some(Modal::Confirm { target, state }) => {
            assert_eq!(state.title(), "Trust role source");
            let termrock::components::ConfirmKind::Details { rows, notes, .. } = state.kind()
            else {
                panic!("expected Details kind, got {:?}", state.kind());
            };
            assert!(
                rows.iter()
                    .any(|(label, value)| label == "Role" && value == "chainargos/agent-brown")
            );
            assert!(
                rows.iter().any(|(label, value)| label == "Repository"
                    && value == "https://github.com/chainargos/jackin-agent-brown.git"),
                "trust prompt should show the repository URL",
            );
            assert!(
                notes
                    .iter()
                    .any(|note| note == "Dockerfile can run during image builds.")
            );
            match target {
                ConfirmTarget::TrustRoleSource { key, source } => {
                    assert_eq!(key, "chainargos/agent-brown");
                    assert_eq!(
                        source.git,
                        "https://github.com/chainargos/jackin-agent-brown.git"
                    );
                    assert!(
                        !source.trusted,
                        "newly resolved third-party role should require explicit trust first"
                    );
                }
                other => panic!("expected TrustRoleSource target; got {other:?}"),
            }
        }
        other => panic!("expected trust Confirm modal; got {other:?}"),
    }
    assert!(
        !editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "role should not be allowed before trust confirmation"
    );
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "validated role source should be registered untrusted before trust confirmation"
    );
    let before_trust = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        before_trust.contains("[roles.\"chainargos/agent-brown\"]"),
        "validated role source should be persisted before trust confirmation:\n{before_trust}"
    );
    assert!(
        !before_trust.contains("trusted = true"),
        "role source should remain untrusted before trust confirmation:\n{before_trust}"
    );

    handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

    assert!(editor.modal.is_none(), "trust confirmation should close");
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "custom allow-list should include the newly resolved role"
    );
    let source = config
        .roles
        .get("chainargos/agent-brown")
        .expect("role source must be added to config");
    assert_eq!(
        source.git,
        "https://github.com/chainargos/jackin-agent-brown.git"
    );
    assert!(source.trusted, "trusted role should be marked trusted");
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("[roles.\"chainargos/agent-brown\"]"),
        "new role source should be persisted:\n{persisted}"
    );
    assert!(
        persisted.contains("trusted = true"),
        "trusted role should be persisted with trusted = true:\n{persisted}"
    );
}

#[test]
fn role_load_poll_success_replaces_loading_popup_with_trust_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let rx = jackin_console::tui::runtime::ready_blocking_subscription(Ok(()));
    let source = jackin_config::RoleSource {
        git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
        trusted: false,
        ..Default::default()
    };
    editor.pending_role_load = Some(PendingRoleLoad {
        raw: "chainargos/agent-brown".into(),
        key: "chainargos/agent-brown".into(),
        source,
        rx,
    });
    editor.modal = Some(Modal::StatusPopup {
        state: jackin_console::tui::components::status_popup::role_loading_status_popup_state(
            "chainargos/agent-brown",
        ),
    });
    poll_role_load(&mut editor, &mut config, &paths);

    assert!(
        editor.pending_role_load.is_none(),
        "completed role load should clear pending state"
    );
    match &editor.modal {
        Some(Modal::Confirm { target, state }) => {
            assert_eq!(state.title(), "Trust role source");
            match target {
                ConfirmTarget::TrustRoleSource { key, source } => {
                    assert_eq!(key, "chainargos/agent-brown");
                    assert_eq!(
                        source.git,
                        "https://github.com/chainargos/jackin-agent-brown.git"
                    );
                    assert!(!source.trusted);
                }
                other => panic!("expected TrustRoleSource target; got {other:?}"),
            }
        }
        other => panic!("expected trust Confirm modal; got {other:?}"),
    }
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "completed load should register the untrusted role source"
    );
}

#[tokio::test]
async fn role_input_trust_decline_keeps_registered_role_untrusted() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;
    assert!(matches!(editor.modal, Some(Modal::Confirm { .. })));

    handle_modal_with(&mut editor, key(KeyCode::Char('n')), &mut config, &paths);

    assert!(editor.modal.is_none(), "decline should close trust prompt");
    assert!(
        !editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "declined role must not be added to the custom allow-list"
    );
    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| !source.trusted),
        "declined role should remain registered but untrusted"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("[roles.\"chainargos/agent-brown\"]"),
        "declined role source should remain registered:\n{persisted}"
    );
    assert!(
        !persisted.contains("trusted = true"),
        "declined role must not be persisted as trusted:\n{persisted}"
    );
}

#[tokio::test]
async fn role_input_existing_untrusted_role_can_be_validated_and_trusted() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    config.roles.insert(
        "chainargos/agent-brown".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
            trusted: false,
            ..Default::default()
        },
    );
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;
    assert!(matches!(
        editor.modal,
        Some(Modal::Confirm {
            target: ConfirmTarget::TrustRoleSource { .. },
            ..
        })
    ));

    handle_modal_with(&mut editor, key(KeyCode::Char('y')), &mut config, &paths);

    assert!(
        config
            .roles
            .get("chainargos/agent-brown")
            .is_some_and(|source| source.trusted),
        "existing untrusted role should become trusted after confirmation"
    );
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "trusted role should be added to the custom allow-list"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        persisted.contains("trusted = true"),
        "confirmed role should persist trust:\n{persisted}"
    );
}

#[tokio::test]
async fn role_input_trusted_existing_role_skips_trust_prompt() {
    // When the config already has a trusted role source the editor
    // must register the cached repo and add it to the workspace
    // *without* re-prompting for trust (`Ok(source) if
    // source.trusted` branch in the role-registration effect executor).
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    config.roles.insert(
        "chainargos/agent-brown".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/jackin-agent-brown.git".into(),
            trusted: true,
            ..Default::default()
        },
    );
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.pending.allowed_roles = vec!["agent-smith".into()];
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || seed_first_temp_valid_role_repo(&data_dir)),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;

    assert!(
        editor.modal.is_none(),
        "trusted existing role must not open the trust-confirm modal: {:?}",
        editor.modal
    );
    assert!(
        editor
            .pending
            .allowed_roles
            .contains(&"chainargos/agent-brown".to_owned()),
        "trusted role should be added to the custom allow-list directly: {:?}",
        editor.pending.allowed_roles
    );
}

#[tokio::test]
async fn role_input_clone_failure_reports_candidate_repository_url() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let mut runner = FakeRunner::default();
    runner
        .fail_with
        .push(("git clone".into(), "repository not found".into()));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "the-architect2",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
            assert!(
                state
                    .message
                    .contains("https://github.com/jackin-project/jackin-the-architect2.git"),
                "message should show the repository URL that was tried:\n{}",
                state.message
            );
            assert!(
                state
                    .message
                    .contains("Repository is not available, or you do not have access."),
                "message should explain the repository is unavailable:\n{}",
                state.message
            );
            assert!(
                !state.message.contains("git clone"),
                "user-facing popup should not include raw clone commands:\n{}",
                state.message
            );
            assert!(
                !state.message.contains(paths.roles_dir.to_str().unwrap()),
                "user-facing popup should not expose the final role cache path:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for failed clone; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("the-architect2"),
        "failed clone must not add the role to in-memory config"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        !persisted.contains("the-architect2"),
        "failed clone must not persist the role:\n{persisted}"
    );
}

#[tokio::test]
async fn role_input_invalid_repo_reports_role_contract_error() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let data_dir = paths.data_dir.clone();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(move || {
            let repo_dir = first_temp_role_repo(&data_dir);
            std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
            std::fs::write(
                repo_dir.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
        }),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "chainargos/agent-brown",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(
                state
                    .message
                    .contains("Repository is not a valid jackin❯ role: missing jackin.role.toml."),
                "message should explain the failed role validation:\n{}",
                state.message
            );
            assert!(
                state
                    .message
                    .contains("https://github.com/chainargos/jackin-agent-brown.git"),
                "message should show the repository URL that was tried:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for invalid role repo; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("chainargos/agent-brown"),
        "invalid role repo must not register the role source"
    );
    let persisted = std::fs::read_to_string(paths.config_file).unwrap();
    assert!(
        !persisted.contains("chainargos/agent-brown"),
        "invalid role repo must not persist the role:\n{persisted}"
    );
}

#[test]
fn role_input_rejects_invalid_selector_with_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::Role,
        state: role_load_input_state(Vec::new()),
    });
    if let Some(Modal::TextInput { state, .. }) = editor.modal.as_mut() {
        for ch in "Chain Argus Agent Brown".chars() {
            state.handle_key(key(KeyCode::Char(ch)).into());
        }
    }

    handle_modal_with(&mut editor, key(KeyCode::Enter), &mut config, &paths);

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
        }
        other => panic!("expected ErrorPopup for invalid selector; got {other:?}"),
    }
    assert!(
        config.roles.is_empty(),
        "invalid selector must not mutate config"
    );
}

#[tokio::test]
async fn role_input_panic_in_registration_is_converted_to_error_popup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = config_with_agents(&["agent-smith"]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut editor = EditorState::new_edit("ws".into(), empty_ws());
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "git clone".to_owned(),
        Box::new(|| panic!("test panic while cloning role repo")),
    ));

    crate::console::effects::apply_role_input_with_runner_for_tests(
        &mut editor,
        &mut config,
        &paths,
        "the-architect2",
        &mut runner,
    )
    .await;

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(state.message.contains("Could not load role"));
            assert!(
                state.message.contains("test panic while cloning role repo"),
                "panic payload should be visible in the error dialog:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for registration panic; got {other:?}"),
    }
    assert!(
        !config.roles.contains_key("the-architect2"),
        "panic must not register the role source"
    );
}

#[test]
fn role_text_input_misroute_uses_error_popup_instead_of_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut editor = EditorState::new_edit("ws".into(), empty_ws());

    apply_text_input_to_pending(&TextInputTarget::Role, &mut editor, "agent-smith", false);

    match &editor.modal {
        Some(Modal::ErrorPopup { state }) => {
            assert_eq!(state.title, "Load role failed");
            assert!(
                state.message.contains("generic text-input handler"),
                "message should explain the misrouted role input:\n{}",
                state.message
            );
        }
        other => panic!("expected ErrorPopup for role misroute; got {other:?}"),
    }
    assert!(config.roles.is_empty());
    let _unused = paths;
}

#[test]
fn agents_tab_star_sets_default_on_allowed_agent() {
    // Cursor on row 1 (role "beta"), no default set yet. Workspace
    // starts in "all roles allowed" shorthand, so beta is
    // effectively allowed. Pressing `*` pins it as default while
    // preserving the shorthand (empty allow list).
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.default_role.as_deref(),
        Some("beta"),
        "`*` on row 1 should pin role `beta` as default",
    );
    assert!(
        e.pending.allowed_roles.is_empty(),
        "default-role pick must preserve the all-roles shorthand; \
             got {:?}",
        e.pending.allowed_roles,
    );
}

#[test]
fn agents_tab_star_on_current_default_clears_it() {
    // With default = "alpha" (effectively allowed under shorthand),
    // pressing `*` on the same row clears the default. Toggle-off is
    // symmetric with the Space allow/disallow toggle.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.default_role = Some("alpha".into());
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`*` on the current default must clear it; got {:?}",
        e.pending.default_role,
    );
}

#[test]
fn agents_tab_star_on_unallowed_agent_is_noop() {
    // Workspace in "custom" mode with only `alpha` allowed; cursor
    // on row 1 (`beta`, NOT in the allow list). `*` must not set
    // beta as default — defaults are meaningless on disallowed
    // roles and the operator should `Space` to allow first.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 1);

    press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`*` on a disallowed role must be a no-op; got {:?}",
        e.pending.default_role,
    );
    assert_eq!(
        e.pending.allowed_roles,
        vec!["alpha".to_owned()],
        "`*` must not silently extend the allow list; got {:?}",
        e.pending.allowed_roles,
    );
}

#[test]
fn agents_tab_disallow_default_clears_default() {
    // With "alpha" pinned as default (custom allow list = [alpha]),
    // pressing Space on alpha to disallow it must also clear the
    // default — defaults are only meaningful on allowed roles.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    ws.default_role = Some("alpha".into());
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.allowed_roles.contains(&"alpha".to_owned()),
        "alpha must be removed from allowed_roles after Space; got {:?}",
        e.pending.allowed_roles,
    );
    assert!(
        e.pending.default_role.is_none(),
        "disallowing the current default must clear default_role; got {:?}",
        e.pending.default_role,
    );
}

#[test]
fn d_key_no_longer_sets_default_agent_on_agents_tab() {
    // Regression guard: the `D` binding was removed in favour of `*`.
    // Pressing `D` on an role row must now be a no-op (no other
    // Roles-tab binding listens for `D`).
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.default_role.is_none(),
        "`D` must no longer set the default role on the Roles tab",
    );
}

#[test]
fn roles_tab_enter_does_not_toggle_allowed_agent() {
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Enter).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "Enter on Roles row must not toggle allowed_roles",
    );
}

#[test]
fn editor_tab_bar_follows_aria_key_pattern() {
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut state = ManagerState::from_config(&config, std::path::Path::new("/"));
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "ws".into(),
        WorkspaceConfig::default(),
    ));

    press(&mut state, &mut config, KeyCode::Right).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused() && editor.active_tab == EditorTab::Mounts)
    );

    press(&mut state, &mut config, KeyCode::Left).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused() && editor.active_tab == EditorTab::General)
    );

    press(&mut state, &mut config, KeyCode::Down).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if !editor.tab_bar_focused()),
        "Down from focused tab bar must enter content",
    );

    press(&mut state, &mut config, KeyCode::BackTab).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused()),
        "ShiftTab from content must return to tab bar",
    );

    press(&mut state, &mut config, KeyCode::Down).unwrap();
    press(&mut state, &mut config, KeyCode::Esc).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(editor) if editor.tab_bar_focused()),
        "Esc from content must return to tab bar",
    );
}

// ── Tab switch clears stale content focus (Defect 19) ────────────

#[test]
fn tab_switch_via_tab_clears_content_scroll_focus() {
    // When content owns focus and the operator presses Tab to cycle tabs,
    // tab_bar_focused must become true and the stale content scroll focus
    // must be cleared so no green border appears on the new tab's content.
    let mut config = config_with_agents(&["alpha"]);
    let mut state = ManagerState::from_config(&config, std::path::Path::new("/"));
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "ws".into(),
        WorkspaceConfig::default(),
    ));

    // Enter content from tab bar.
    press(&mut state, &mut config, KeyCode::Down).unwrap();
    let tab_bar_cleared = matches!(&state.stage, ManagerStage::Editor(e) if !e.tab_bar_focused());
    assert!(tab_bar_cleared, "Down must enter content");

    // Tab while content is focused cycles to the next tab AND returns focus to tab bar.
    press(&mut state, &mut config, KeyCode::Tab).unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::Editor(e)
            if e.tab_bar_focused()
                && !e.tab_content_scroll_focused()
                && !e.workspace_mounts_scroll_focused()),
        "Tab from content must return focus to tab bar and clear content scroll focus"
    );
}

// ── Roles tab: Space toggle matches effective allow-state ────────

#[test]
fn toggle_in_all_mode_demotes_to_custom_without_this_agent() {
    // Starting state: "all" mode (empty list), three roles. Pressing
    // Space on row 1 (`beta`) must produce a custom list containing
    // every other role — i.e. `[alpha, gamma]` — so that `beta`
    // flips from `[x]` to `[ ]` and the status line reads
    // `custom (2 of 3 allowed)`.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut state = editor_on_agents_tab(empty_ws(), 1);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let list = pending_allowed(&state);
    assert_eq!(
        list,
        vec!["alpha".to_owned(), "gamma".to_owned()],
        "list must be populated with every other role when demoting from 'all'"
    );
}

#[test]
fn toggle_custom_last_item_clears_to_empty() {
    // Starting state: "custom" mode with a single allowed role.
    // Toggling that role off must leave the list empty (reverting
    // to the "all" shorthand) — NOT pinning it at a phantom
    // `custom (0 of N allowed)` state.
    let mut config = config_with_agents(&["alpha", "beta"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 0);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "removing the last custom entry must leave the list empty (= all allowed)",
    );
}

#[test]
fn toggle_adds_back_to_custom() {
    // Starting state: "custom" mode with `[alpha]` (so `beta` reads
    // `[ ]`). Pressing Space on `beta` (row 1) must add it, producing
    // `[alpha, beta]` — and since that still doesn't cover every
    // role (`gamma` is missing), the list must stay non-empty.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into()];
    let mut state = editor_on_agents_tab(ws, 1);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    let mut list = pending_allowed(&state);
    list.sort();
    assert_eq!(
        list,
        vec!["alpha".to_owned(), "beta".to_owned()],
        "adding `beta` with `gamma` still missing must produce a 2-of-3 custom list",
    );
}

#[test]
fn toggle_refills_custom_to_all_when_last_agent_added_makes_it_complete() {
    // Starting state: "custom" mode with all-but-one role present.
    // Adding the missing one would yield `custom (N of N allowed)` —
    // semantically identical to "all allowed". The toggle must
    // collapse back to the empty-list shorthand so the status badge
    // reads `all`, not `custom (3 of 3 allowed)`.
    let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = empty_ws();
    ws.allowed_roles = vec!["alpha".into(), "beta".into()];
    // Cursor on row 2 (role `gamma`, the missing one).
    let mut state = editor_on_agents_tab(ws, 2);

    press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

    assert_eq!(
        pending_allowed(&state),
        Vec::<String>::new(),
        "filling the custom list must collapse it to empty (= all allowed)",
    );
}

// ── Mounts tab: R toggles readonly (rw ↔ ro) ──────────────────────

#[test]
fn r_key_toggles_readonly_on_current_mount_row() {
    // Start rw → one R press should flip to ro and register as a change.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.mounts[0].readonly,
        "R on rw mount must flip to ro",
    );
    assert!(
        e.change_count() > 0,
        "flipping readonly must surface as a change; got change_count={}",
        e.change_count()
    );
}

#[test]
fn r_key_lowercase_also_toggles_readonly() {
    // Operators often hit `r` without holding shift; both cases must work.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('r')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(e.pending.mounts[0].readonly);
}

#[test]
fn rows_beyond_workspace_mounts_are_noop_in_workspace_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("cache");
    std::fs::create_dir_all(&src).unwrap();
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".into(), jackin_config::RoleSource::default());
    config.add_mount(
        "cache",
        MountConfig {
            src: src.display().to_string(),
            dst: "/cache".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
        None,
    );
    let mut ws = ws_with_one_mount(false);
    ws.allowed_roles = vec!["agent-smith".into()];
    // Global mounts are intentionally not rendered/editable here; row 2
    // simulates stale input beyond the workspace mount + add sentinel.
    let mut state = editor_on_mounts_tab(ws, 2);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('D')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('I')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(e.pending.mounts.len(), 1);
    assert!(!e.pending.mounts[0].readonly);
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Shared
    );
}

#[test]
fn r_key_on_sentinel_is_noop() {
    // Cursor on the `+ Add mount` sentinel (row == mounts.len()) — R must
    // not mutate mounts or trigger a change.
    let mut config = AppConfig::default();
    let ws = ws_with_one_mount(false);
    let before = ws.mounts.clone();
    let mut state = editor_on_mounts_tab(ws, 1); // sentinel row

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts, before,
        "R on sentinel must leave mounts untouched"
    );
    assert_eq!(
        e.change_count(),
        0,
        "R on sentinel must not mark editor dirty"
    );
}

#[test]
fn r_key_twice_restores_original() {
    // Flipping twice must bring `readonly` back to the starting value AND
    // net out to zero changes — the diff-based change_count treats
    // identical mounts as unchanged.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();
    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.mounts[0].readonly,
        "two R presses must restore original rw state"
    );
    assert_eq!(
        e.change_count(),
        0,
        "two R presses must net zero changes; got {}",
        e.change_count()
    );
}

#[test]
fn r_key_on_non_mounts_tab_is_noop() {
    // Cursor set to row 0 on General tab with a mount present; pressing R
    // must not mutate the mount list (the handler is gated on
    // `active_tab == EditorTab::Mounts`).
    let mut config = AppConfig::default();
    let ws = ws_with_one_mount(false);
    let before = ws.mounts.clone();
    let mut state = editor_on_mounts_tab(ws, 0);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_tab = EditorTab::General;
    }

    press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts, before,
        "R on non-Mounts tab must leave mounts untouched"
    );
}

// ── Mounts tab: I cycles isolation (shared ↔ worktree) ────────────

#[test]
fn i_key_cycles_isolation_on_current_mount_row() {
    // Start Shared → one I press should flip to Worktree and register
    // as a change. Mirrors `r_key_toggles_readonly_on_current_mount_row`.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('I')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Worktree,
        "I on a Shared mount must cycle to Worktree",
    );
    assert!(
        e.change_count() > 0,
        "cycling isolation must surface as a change; got change_count={}",
        e.change_count(),
    );
}

#[test]
fn i_key_lowercase_also_cycles_isolation() {
    // Operators often hit `i` without holding shift; both cases must work.
    let mut config = AppConfig::default();
    let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

    press(&mut state, &mut config, KeyCode::Char('i')).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        e.pending.mounts[0].isolation,
        jackin_config::MountIsolation::Worktree,
    );
}

// ── Caps-Lock parity: SHIFT-modified letter shortcuts ──────────────

/// Enter on an op:// key row must NOT open the `EnvValue` text-edit
/// modal. The breadcrumb is a path, not a credential, and hand-
/// editing the path is error-prone — the operator deletes via D
/// and re-adds via the source picker (`P`).
#[test]
fn enter_on_op_workspace_key_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );

    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0); // the only key row
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.modal.is_none(),
        "Enter on an op:// row must not open any modal; got {:?}",
        e.modal
    );
}

/// Same guard for an role-override row: Enter on an op:// value in
/// an expanded role section is also a no-op.
#[test]
fn enter_on_op_agent_key_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert(
        "API_TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/api-token".into(),
            path: "Personal/api/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    ws.roles.insert(
        "smith".into(),
        jackin_config::WorkspaceRoleOverride {
            env: ag_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );

    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.secrets_expanded.insert("smith".into());
    // Rows: WorkspaceAddSentinel(0), SectionSpacer(1), AgentHeader(2),
    //       AgentKeyRow(3), AgentAddSentinel(4). Focus the key row.
    editor.active_field = FieldFocus::Row(3);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.modal.is_none(),
        "Enter on an role op:// row must not open any modal; got {:?}",
        e.modal
    );
}

/// Caps Lock causes terminals to send letter keys with the SHIFT
/// modifier set. The Secrets-tab `M` (mask toggle) and `P` (1Password
/// picker) bindings must accept SHIFT just like NONE — otherwise an
/// operator with Caps Lock on sees a silent no-op.
#[test]
fn secrets_tab_m_accepts_shift_modifier_for_caps_lock_parity() {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("DB_URL".into(), "literal-value".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0); // the only key row
    state.stage = ManagerStage::Editor(editor);

    let shift_m = KeyEvent {
        code: KeyCode::Char('M'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    handle_key(&mut state, &mut config, &paths, tmp.path(), shift_m).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "DB_URL".into())),
        "M with SHIFT modifier (Caps Lock parity) must add the focused \
             row to unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

/// `M` on a focused workspace key row toggles only that row's mask
/// state — sibling rows stay masked. This is the operator's core
/// commit-32 ask: never reveal an unintended row.
#[test]
fn m_on_focused_workspace_key_unmasks_only_that_row() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first-value".into());
    ws.env.insert("BETA".into(), "second-value".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    // Rows are alphabetically ordered: ALPHA(0), BETA(1), Sentinel(2).
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "ALPHA".into())),
        "ALPHA must be unmasked"
    );
    assert!(
        !e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "BETA".into())),
        "BETA must remain masked"
    );
}

/// Pressing M twice on the same row toggles the mask back on —
/// the per-row state is a flip, not a one-way reveal.
#[test]
fn m_on_already_unmasked_row_re_masks_it() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert!(
        e.unmasked_rows.is_empty(),
        "second M must remove the row from unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

/// M on an op:// row is a no-op — those rows render as breadcrumbs
/// regardless of the mask state, so adding them to `unmasked_rows`
/// would be visually inert and confuse the operator.
#[test]
fn m_on_op_reference_row_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert!(
        e.unmasked_rows.is_empty(),
        "M on an op:// row must not modify unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

/// Leaving and re-entering the Secrets tab clears `unmasked_rows`
/// — the all-masked baseline is restored each visit.
#[test]
fn tab_leave_resets_unmasked_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    ws.env.insert("ALPHA".into(), "first".into());
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    // Unmask ALPHA.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();
    // Tab from content → tab bar + advances tab to Auth (leaves Secrets).
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Tab),
    )
    .unwrap();
    // Now on tab bar (Auth). Right × 4: General → Mounts → Roles → Secrets.
    for _ in 0..4 {
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Right),
        )
        .unwrap();
    }

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert_eq!(e.active_tab, EditorTab::Secrets);
    assert!(
        e.unmasked_rows.is_empty(),
        "tab-leave must clear unmasked_rows; got {:?}",
        e.unmasked_rows
    );
}

/// Workspace and role scopes have separate mask state. M on an
/// role row unmasks only the role row even when a workspace row
/// shares the same key name.
#[test]
fn m_on_agent_key_unmasks_only_that_row_in_that_agent_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    // Same key name in both scopes.
    ws.env.insert("API_TOKEN".into(), "ws-value".into());
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert("API_TOKEN".into(), "role-value".into());
    ws.roles.insert(
        "smith".into(),
        jackin_config::WorkspaceRoleOverride {
            env: ag_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.secrets_expanded.insert("smith".into());
    let role_key_row = editor
        .secrets_flat_rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                SecretsRow::RoleKeyRow { role, key } if role == "smith" && key == "API_TOKEN"
            )
        })
        .expect("role API_TOKEN row");
    editor.active_field = FieldFocus::Row(role_key_row);
    state.stage = ManagerStage::Editor(editor);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('m')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    assert!(
        e.unmasked_rows
            .contains(&(SecretsScopeTag::Role("smith".into()), "API_TOKEN".into())),
        "role-scope API_TOKEN must be unmasked"
    );
    assert!(
        !e.unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "API_TOKEN".into())),
        "workspace-scope API_TOKEN with same key name must remain masked"
    );
}

/// Pressing `↓` from the workspace `+ Add` sentinel must skip past
/// the `SectionSpacer` and land directly on the first focusable row
/// of the role section (the `AgentHeader`). Same in reverse with
/// `↑`. Regression guard for the cursor-skip logic added with the
/// blank-line-between-sections layout polish.
#[test]
fn cursor_skips_section_spacer_on_down_arrow() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut ws = empty_ws();
    let mut ag_env = std::collections::BTreeMap::new();
    ag_env.insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert(
        "agent-smith".into(),
        jackin_config::WorkspaceRoleOverride {
            env: ag_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );

    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    // Rows with no workspace env keys + one collapsed role section:
    //   0 WorkspaceAddSentinel
    //   1 SectionSpacer
    //   2 AgentHeader
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    // Sanity-check the row layout matches the comment above before
    // exercising the navigation.
    if let ManagerStage::Editor(e) = &state.stage {
        let rows = e.secrets_flat_rows();
        assert!(matches!(
            rows.first(),
            Some(SecretsRow::WorkspaceAddSentinel)
        ));
        assert!(matches!(rows.get(1), Some(SecretsRow::SectionSpacer)));
        assert!(matches!(rows.get(2), Some(SecretsRow::RoleHeader { .. })));
    }

    // ↓ from row 0 must land on row 2, skipping the spacer at row 1.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(2)),
        "↓ from sentinel(0) must skip spacer(1) and land on header(2); \
             got {:?}",
        e.active_field
    );

    // ↑ from row 2 must land back on row 0, skipping the spacer.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Up),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(0)),
        "↑ from header(2) must skip spacer(1) and land on sentinel(0); \
             got {:?}",
        e.active_field
    );
}

// ── General tab: keep_awake Space toggle ──────────────────────────

#[test]
fn space_on_general_keep_awake_row_toggles_pending_flag() {
    // Row 2 of the General tab is the keep_awake toggle. Space
    // flips pending.keep_awake.enabled; subsequent Space flips
    // back. The change lives only on `pending` (not `original`)
    // until the operator saves — that's what build_workspace_edit
    // detects to populate WorkspaceEdit.keep_awake_enabled.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_field = FieldFocus::Row(2);
    }

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char(' ')),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.pending.keep_awake.enabled,
        "first Space on row 2 must enable keep_awake"
    );
    assert!(
        !e.original.keep_awake.enabled,
        "Space must mutate pending only, not original (so the diff is visible to save)"
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char(' ')),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        !e.pending.keep_awake.enabled,
        "second Space must toggle keep_awake back off",
    );
}

#[test]
fn enter_on_general_toggle_rows_does_not_toggle_flags() {
    for (row, label) in [(2usize, "keep_awake"), (3usize, "git_pull_on_entry")] {
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(row);
            assert!(!e.pending.keep_awake.enabled);
            assert!(!e.pending.git_pull_on_entry);
        }

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.keep_awake.enabled,
            "Enter on {label} row must not toggle keep_awake",
        );
        assert!(
            !e.pending.git_pull_on_entry,
            "Enter on {label} row must not toggle git_pull_on_entry",
        );
    }
}

#[test]
fn space_on_general_non_toggle_rows_does_not_flip_keep_awake() {
    // Row 0 (Name) and row 1 (Working dir) ignore Space — those
    // are modal-opening fields driven by Enter. A regression that
    // applied the toggle from any General row would flip the flag
    // when the operator was just typing a Space in a name input.
    for row in [0usize, 1usize] {
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
        if let ManagerStage::Editor(e) = &mut state.stage {
            e.active_field = FieldFocus::Row(row);
        }
        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(' ')),
        )
        .unwrap();
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            !e.pending.keep_awake.enabled,
            "Space on General row {row} must NOT toggle keep_awake",
        );
    }
}

#[test]
fn down_arrow_on_general_can_reach_keep_awake_row() {
    // max_row_for_tab(General) must allow the cursor to navigate
    // to row 2; otherwise the toggle would be reachable only via
    // direct mutation, defeating the operator-discoverable
    // workflow.
    let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.active_field = FieldFocus::Row(0);
    }
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Down),
    )
    .unwrap();
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.active_field, FieldFocus::Row(2)),
        "two ↓ presses from row 0 must land on row 2 (Keep awake); got {:?}",
        e.active_field,
    );
}

// ── TUI text-entry regression: typing or pasting op:// must stay Plain ──

/// Typing or pasting `op://...` into a value cell (text-entry path)
/// must always commit as `EnvValue::Plain`. The picker is the ONLY
/// TUI path that produces `EnvValue::OpRef`; this test pins that
/// invariant so an accidental auto-resolve can never sneak in.
///
/// The structural guarantee: `apply_text_input_to_pending` for the
/// `EnvValue` target calls `set_pending_env_value`, which
/// unconditionally wraps its `&str` argument in
/// `EnvValue::Plain(value.to_string())`. There is no `op://` pattern
/// match in the text-entry commit path.
#[test]
fn tui_text_entry_op_uri_always_commits_as_plain() {
    let mut editor = EditorState::new_edit("CLAUDE_TOKEN_WS".into(), WorkspaceConfig::default());

    let target = TextInputTarget::EnvValue {
        scope: SecretsScopeTag::Workspace,
        key: "CLAUDE_TOKEN".into(),
    };

    // Simulate committing a typed/pasted op:// string via the
    // text-entry path (Enter in the EnvValue modal).
    apply_text_input(&target, &mut editor, "op://Vault/Item/Field");

    let stored = editor
        .pending
        .env
        .get("CLAUDE_TOKEN")
        .expect("CLAUDE_TOKEN must be present after commit");

    assert_eq!(
        stored,
        &jackin_core::EnvValue::Plain("op://Vault/Item/Field".into()),
        "text-entry commit of op:// string must store EnvValue::Plain, \
             not EnvValue::OpRef — the picker is the only path to OpRef"
    );
    // Belt-and-suspenders: confirm it is NOT an OpRef.
    assert!(
        !matches!(stored, jackin_core::EnvValue::OpRef(_)),
        "text entry must never produce EnvValue::OpRef"
    );
}
