//! Tests for `save` — tests.
use super::super::test_support::{key, mount};
use super::{begin_editor_save, commit_editor_save};
use crate::config::AppConfig;
use crate::console::tui::input::handle_key;
use crate::console::tui::state::{
    EditorMode, EditorSaveFlow, EditorState, ManagerStage, ManagerState, Modal,
};
use crate::paths::JackinPaths;
use crate::workspace::{KeepAwakeConfig, MountConfig, WorkspaceConfig};
use crossterm::event::KeyCode;
use tempfile::TempDir;

fn ro_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: true,
        isolation: crate::isolation::MountIsolation::Shared,
    }
}

fn setup_with_workspace(
    name: &str,
    ws: WorkspaceConfig,
) -> anyhow::Result<(TempDir, JackinPaths, AppConfig)> {
    let tmp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs()?;

    let mut config = AppConfig::default();
    config.workspaces.insert(name.to_owned(), ws);
    let toml = toml::to_string(&config)?;
    std::fs::write(&paths.config_file, toml)?;

    let reloaded = AppConfig::load_or_init(&paths)?;
    Ok((tmp, paths, reloaded))
}

fn press_s(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) {
    handle_key(state, config, paths, cwd, key(KeyCode::Char('s'))).unwrap();
}

fn run_pending_save_commit(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) {
    crate::console::effects::execute_pending_workspace_save_commit(state, config, paths, cwd)
        .unwrap();
}

#[test]
fn apply_auth_forward_diff_persists_amp_workspace_and_role_modes() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/project".into(),
        mounts: vec![mount(tmp.path().to_str().unwrap(), "/workspace/project")],
        ..WorkspaceConfig::default()
    };
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("proj".to_owned(), original.clone());
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut pending = original.clone();
    pending.amp = Some(crate::config::AgentAuthConfig {
        auth_forward: crate::config::AuthForwardMode::ApiKey,
        ..Default::default()
    });
    pending.roles.insert(
        "smith".into(),
        crate::workspace::WorkspaceRoleOverride {
            amp: Some(crate::config::AgentAuthConfig {
                auth_forward: crate::config::AuthForwardMode::Ignore,
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut editor = crate::config::ConfigEditor::open(&paths).unwrap();
    crate::console::services::config::apply_auth_forward_diff(
        &mut editor,
        "proj",
        &original,
        &pending,
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(paths.workspaces_dir.join("proj.toml")).unwrap();
    assert!(out.contains("[amp]"), "{out}");
    assert!(out.contains(r#"auth_forward = "api_key""#), "{out}");
    assert!(out.contains("[roles.smith.amp]"), "{out}");
    assert!(out.contains(r#"auth_forward = "ignore""#), "{out}");
}

#[test]
fn build_workspace_edit_emits_keep_awake_change_only_when_diffed() {
    // The TUI save path leans on `build_workspace_edit` to discover
    // what fields the operator touched. If keep_awake's diff path
    // ever regresses to "always emit," the resulting WorkspaceEdit
    // would clobber the field on every save — breaking the "edit
    // workdir doesn't flip keep_awake" contract that
    // `edit_workspace_toggles_keep_awake_when_set` enforces.
    use crate::workspace::KeepAwakeConfig;
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/proj".into(),
        mounts: vec![mount("/work", "/workspace/proj")],
        keep_awake: KeepAwakeConfig { enabled: false },
        ..Default::default()
    };

    // No change → no field set.
    let pending_unchanged = original.clone();
    let edit = crate::console::domain::build_workspace_edit(&original, &pending_unchanged);
    assert_eq!(edit.keep_awake_enabled, None);

    // Flip on → Some(true).
    let pending_on = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: true },
        ..original.clone()
    };
    let edit = crate::console::domain::build_workspace_edit(&original, &pending_on);
    assert_eq!(edit.keep_awake_enabled, Some(true));

    // Flip off (when original was on) → Some(false).
    let original_on = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: true },
        ..original.clone()
    };
    let pending_off = WorkspaceConfig {
        keep_awake: KeepAwakeConfig { enabled: false },
        ..original
    };
    let edit = crate::console::domain::build_workspace_edit(&original_on, &pending_off);
    assert_eq!(edit.keep_awake_enabled, Some(false));
}

#[test]
fn save_editor_opens_confirm_save_on_edit_driven_collapse() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work/sub".into(),
        mounts: vec![mount("/work/sub", "/work/sub")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
    editor.pending.mounts.insert(0, mount("/work", "/work"));
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!("expected ConfirmSave modal; got {:?}", e.modal);
    };
    assert!(
        modal.has_collapses,
        "modal must flag the collapse for the display layer"
    );
    assert!(!e.save_flow.is_error(), "no error state expected");
    // The on-disk config should not have been touched yet.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
    assert_eq!(
        ws_on_disk.mounts.len(),
        1,
        "write must be deferred until confirm"
    );
}

#[test]
fn confirming_collapse_writes_collapsed_set() {
    // Same setup, then simulate the operator pressing Enter on the
    // ConfirmSave modal — this should transition save_flow to
    // PendingCommit, drive commit_editor_save, and write the
    // collapsed mount set.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work/sub".into(),
        mounts: vec![mount("/work/sub", "/work/sub")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
    editor.pending.mounts.insert(0, mount("/work", "/work"));
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    // Step 2: Tab moves focus Cancel -> Save; Enter then commits the save.
    // Default focus = Cancel (TUI design decisions: confirmation dialog rule).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "s + confirm should exit to list; got {:?}",
        state.stage
    );

    // On-disk config now contains only the collapsed parent.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
    assert_eq!(ws_on_disk.mounts.len(), 1);
    assert_eq!(ws_on_disk.mounts[0].dst, "/work");
}

#[test]
fn cancelling_confirm_save_keeps_pending_intact() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work/sub".into(),
        mounts: vec![mount("/work/sub", "/work/sub")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
    editor.pending.mounts.insert(0, mount("/work", "/work"));
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    // Press C — cancel the ConfirmSave dialog.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('c')),
    )
    .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(e.modal.is_none(), "modal should close on cancel");
    assert_eq!(
        e.pending.mounts.len(),
        2,
        "pending mounts stay so operator can fix by hand"
    );
    assert!(
        matches!(e.save_flow, EditorSaveFlow::Idle),
        "save flow must return to Idle on cancel; got {:?}",
        e.save_flow,
    );

    // On-disk config unchanged.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
    assert_eq!(ws_on_disk.mounts.len(), 1);
}

#[test]
fn readonly_mismatch_produces_error_popup_no_write() {
    // Add a rw /work that would subsume an existing ro /work/sub —
    // plan_edit must reject with ReadonlyMismatch. Per spec, hard
    // planner errors surface through ErrorPopup, not ConfirmSave.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work/sub".into(),
        mounts: vec![ro_mount("/work/sub", "/work/sub")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
    editor.pending.mounts.insert(0, mount("/work", "/work")); // rw
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "hard planner errors must use ErrorPopup",
    );
    let message = e
        .save_flow
        .error_message()
        .expect("readonly mismatch should produce save error");
    assert!(
        message.contains("readonly"),
        "popup should mention readonly: {message}"
    );
    // On-disk config unchanged.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
    assert_eq!(ws_on_disk.mounts.len(), 1);
}

#[test]
fn editor_save_create_with_no_name_routes_to_error_flow() {
    // begin_editor_save in Create mode must gate ConfirmSave on
    // pending_name being set. Without a name the ErrorPopup reads
    // "missing workspace name" - gating prevents the operator from
    // committing a nameless workspace.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/seed".into(),
        mounts: vec![mount("/seed", "/seed")],
        ..Default::default()
    };
    let (tmp, paths, config) = setup_with_workspace("seed", ws).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_create();
    editor.pending.workdir = "/w".into();
    editor.pending.mounts = vec![mount("/w", "/w")];
    // pending_name intentionally None → save must route to Error.
    state.stage = ManagerStage::Editor(editor);

    begin_editor_save(&mut state, &config, false).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "missing name must use ErrorPopup",
    );
    assert_eq!(
        e.save_flow.error_message().expect("save error expected"),
        "missing workspace name"
    );
    // On-disk config unchanged.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(!reloaded.workspaces.contains_key("w"));
}

#[test]
fn editor_save_create_with_invalid_mount_routes_to_error_flow() {
    // Create-mode planner errors (here a ReadonlyMismatch - `/work/sub`
    // ro under `/work` rw) surface through ErrorPopup, mirroring the
    // edit-mode behavior covered by
    // `readonly_mismatch_produces_error_popup_no_write`.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/seed".into(),
        mounts: vec![mount("/seed", "/seed")],
        ..Default::default()
    };
    let (tmp, paths, config) = setup_with_workspace("seed", ws).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("test".into());
    editor.pending.workdir = "/work".into();
    editor.pending.mounts = vec![mount("/work", "/work"), ro_mount("/work/sub", "/work/sub")];
    state.stage = ManagerStage::Editor(editor);

    begin_editor_save(&mut state, &config, false).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "planner rejects must use ErrorPopup",
    );
    let message = e
        .save_flow
        .error_message()
        .expect("readonly mismatch should produce save error");
    assert!(
        message.contains("readonly"),
        "popup should mention readonly: {message}"
    );
    // On-disk config unchanged.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(!reloaded.workspaces.contains_key("test"));
}

#[test]
fn pre_existing_collapse_produces_prune_error_popup() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work".into(),
        mounts: vec![
            mount("/work", "/work"),
            mount("/work/sub", "/work/sub"), // already redundant
        ],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("legacy-workspace", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("legacy-workspace".into(), ws);
    // The editor must be dirty to trigger the save path — bump workdir
    // so change_count > 0. Previously the test relied on save_editor
    // running unconditionally; under the new no-op-on-clean rule we
    // have to force a change.
    editor.pending.workdir = "/work/altered".into();
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "pre-existing-only case must use ErrorPopup",
    );
    let message = e
        .save_flow
        .error_message()
        .expect("pre-existing collapse should produce save error");
    assert!(
        message.contains("prune"),
        "popup should reference `workspace prune`: {message}"
    );
    assert!(
        message.contains("legacy-workspace"),
        "popup should name the workspace: {message}"
    );
}

#[test]
fn s_with_zero_changes_is_noop() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("clean-ws", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let editor = EditorState::new_edit("clean-ws".into(), ws);
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        e.modal.is_none(),
        "no ConfirmSave should open when change_count is 0"
    );
    assert!(!e.save_flow.is_error());
}

#[test]
fn s_with_changes_opens_confirm_save_modal() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("edit-me", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("edit-me".into(), ws);
    editor.pending.workdir = "/w/elsewhere".into();
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ConfirmSave { .. })),
        "expected ConfirmSave; got {:?}",
        e.modal
    );
}

#[test]
fn confirm_save_save_exits_editor_on_success_from_save_discard_path() {
    // Call `begin_editor_save` with `exit_on_success = true` directly
    // (as the SaveDiscardCancel Save path would, via the outer
    // `ExitIntent::Save` dispatcher). After Enter on the resulting
    // ConfirmSave modal, we should land back on ManagerStage::List.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("exit-me", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("exit-me".into(), ws);
    editor.pending.workdir = "/w/elsewhere".into();
    state.stage = ManagerStage::Editor(editor);

    begin_editor_save(&mut state, &config, true).unwrap();
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "save with exit_on_success = true should return to the list stage"
    );
}

#[test]
fn exit_on_success_selects_just_saved_workspace_on_return_to_list() {
    // Two workspaces: "a-first" (index 0) and "z-second" (index 1) in
    // BTreeMap order. Editing "z-second" and saving must land the cursor
    // on "z-second" (screen index 2 = 1 + 1), not on "a-first" or the
    // CWD row.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("z-second", ws.clone()).unwrap();
    config.workspaces.insert(
        "a-first".to_owned(),
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/a".into(),
            mounts: vec![mount("/a", "/a")],
            ..Default::default()
        },
    );
    let toml = toml::to_string(&config).unwrap();
    std::fs::write(&paths.config_file, toml).unwrap();
    config = AppConfig::load_or_init(&paths).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("z-second".into(), ws);
    editor.pending.workdir = "/w/sub".into();
    state.stage = ManagerStage::Editor(editor);

    begin_editor_save(&mut state, &config, true).unwrap();
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(matches!(state.stage, ManagerStage::List));
    // BTreeMap order: ["a-first"=0, "z-second"=1]; screen index = i + 1.
    // "z-second" is at saved_index 1, so screen index = 2.
    assert_eq!(
        state.selected, 2,
        "cursor must land on the just-saved workspace; got selected={}",
        state.selected
    );
    assert_eq!(state.workspaces[state.selected - 1].name, "z-second");
}

#[test]
fn exit_on_success_save_returns_to_list() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("toast-me", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("toast-me".into(), ws);
    editor.pending.workdir = "/w/elsewhere".into();
    state.stage = ManagerStage::Editor(editor);

    begin_editor_save(&mut state, &config, true).unwrap();
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "exit_on_success should land us in the list; got {:?}",
        state.stage,
    );
}

#[test]
fn failed_post_rename_edit_leaves_editor_mode_on_original_name() {
    // Finding #4: if `ce.rename_workspace` succeeds but the subsequent
    // `ce.edit_workspace` fails, the old code already mutated
    // `editor.mode` to the new name — leaving the editor UI advertising
    // a rename that never reached disk. The fix defers the mode
    // mutation to the `ce.save()` success arm; a pre-save failure
    // must leave `editor.mode` on the original name.
    //
    // We trigger a post-rename failure by calling `commit_editor_save`
    // directly with a hand-built plan whose `effective_removals`
    // references a destination that doesn't exist on the workspace.
    // `AppConfig::edit_workspace` validates `remove_destinations`
    // against the live mount list and bails out with
    // "unknown workspace mount destination".
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("original-name", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("original-name".into(), ws);
    editor.pending_name = Some("renamed-in-memory".into());
    state.stage = ManagerStage::Editor(editor);

    // Drive commit_editor_save directly with a plan that will make
    // `ce.edit_workspace` fail AFTER `ce.rename_workspace` has already
    // moved the workspace inside ConfigEditor's in-memory buffer.
    let bad_plan = crate::console::tui::state::PendingSaveCommit {
        effective_removals: vec!["/does/not/exist".to_owned()],
        final_mounts: None,
        delete_isolated_acknowledged: false,
        isolated_cleanup_complete: false,
    };
    if let Some(effect) = commit_editor_save(&mut state, &mut config, bad_plan, false).unwrap() {
        crate::console::effects::execute_workspace_save_effect(
            &mut state,
            &mut config,
            &paths,
            cwd,
            effect,
        );
    }

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected after failed save");
    };
    if let EditorMode::Edit { name } = &e.mode {
        assert_eq!(
            name, "original-name",
            "editor.mode must stay on the original name when the save \
                 fails after rename — got {name:?}",
        );
    } else {
        panic!("expected EditorMode::Edit; got {:?}", e.mode);
    }

    // The error popup must have been opened so the operator knows.
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "post-rename edit_workspace failure should surface via ErrorPopup; \
             got {:?}",
        e.modal,
    );

    // And the on-disk config must not have been touched.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        reloaded.workspaces.contains_key("original-name"),
        "on-disk config should still have the original name; got {:?}",
        reloaded.workspaces.keys().collect::<Vec<_>>(),
    );
    assert!(
        !reloaded.workspaces.contains_key("renamed-in-memory"),
        "rename must not have reached disk after the edit_workspace failure",
    );
}

#[test]
fn create_mode_save_returns_to_list() {
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
    editor.pending_name = Some("toasty-create".into());
    editor.pending.workdir = "/code/proj".into();
    editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "create save should return to the list; got {:?}",
        state.stage,
    );
}

#[test]
fn confirm_save_s_exits_to_list_on_success() {
    // `s` + Enter on ConfirmSave returns the operator to the list,
    // consistent with the Esc→Save path.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("save-me", ws.clone()).unwrap();

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("save-me".into(), ws);
    editor.pending.workdir = "/w/new".into();
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    assert!(
        matches!(state.stage, ManagerStage::List),
        "s + confirm must return to the list; got {:?}",
        state.stage
    );
}

#[test]
fn confirm_save_save_opens_error_popup_on_duplicate_name() {
    // Two workspaces on disk; rename one to the other's name. The
    // write hits ConfigEditor::rename_workspace's duplicate-name
    // guard and we expect an ErrorPopup.
    let ws_a = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/a".into(),
        mounts: vec![mount("/a", "/a")],
        ..Default::default()
    };
    let ws_b = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/b".into(),
        mounts: vec![mount("/b", "/b")],
        ..Default::default()
    };
    let (tmp, paths, _) = setup_with_workspace("alpha", ws_a.clone()).unwrap();
    // Add the second workspace on disk.
    let mut config = {
        let mut ce = crate::config::ConfigEditor::open(&paths).unwrap();
        ce.create_workspace("beta", ws_b).unwrap();
        ce.save().unwrap()
    };

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("alpha".into(), ws_a);
    editor.pending_name = Some("beta".into()); // collides
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("stay in editor when save fails");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "expected ErrorPopup on duplicate-name; got {:?}",
        e.modal
    );
}

#[test]
fn error_popup_dismiss_returns_to_editor_with_changes_intact() {
    let ws_a = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/a".into(),
        mounts: vec![mount("/a", "/a")],
        ..Default::default()
    };
    let ws_b = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/b".into(),
        mounts: vec![mount("/b", "/b")],
        ..Default::default()
    };
    let (tmp, paths, _) = setup_with_workspace("alpha", ws_a.clone()).unwrap();
    let mut config = {
        let mut ce = crate::config::ConfigEditor::open(&paths).unwrap();
        ce.create_workspace("beta", ws_b).unwrap();
        ce.save().unwrap()
    };

    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("alpha".into(), ws_a);
    editor.pending_name = Some("beta".into());
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);
    // Default focus = Cancel; Tab -> Save, then Enter fires the save attempt
    // which collides with "beta" and opens the ErrorPopup.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    run_pending_save_commit(&mut state, &mut config, &paths, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc)).unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("stay in editor after ErrorPopup dismiss");
    };
    assert!(e.modal.is_none(), "popup should be closed on Esc");
    assert_eq!(
        e.pending_name.as_deref(),
        Some("beta"),
        "pending rename must survive the popup so operator can adjust"
    );
}

#[test]
fn create_mode_confirm_save_includes_mounts_in_lines() {
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
    editor.pending_name = Some("new-one".into());
    editor.pending.workdir = "/code/proj".into();
    editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!("expected ConfirmSave");
    };
    // Crude assertion: at least one line mentions the mount path.
    let joined: String = modal
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        joined.contains("/code/proj"),
        "mount path must appear in ConfirmSave lines: {joined}"
    );
    assert!(
        joined.contains("new-one"),
        "workspace name must appear: {joined}"
    );
}

#[test]
fn create_mode_confirm_save_reflects_renamed_workspace_name() {
    // The ConfirmSave dialog's first line reads
    // "Create workspace: <name>" — after an in-editor rename, the
    // summary must pick up the edited name, not the prelude-captured one.
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
    editor.pending_name = Some("prelude-captured".into());
    editor.pending.workdir = "/code/proj".into();
    editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
    state.stage = ManagerStage::Editor(editor);

    // Operator renames mid-edit.
    super::super::editor::apply_text_input_to_pending(
        &crate::console::tui::state::TextInputTarget::Name,
        match &mut state.stage {
            ManagerStage::Editor(e) => e,
            _ => unreachable!(),
        },
        "edited-in-place",
        false,
    );

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!("expected ConfirmSave");
    };
    let joined: String = modal
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        joined.contains("edited-in-place"),
        "ConfirmSave must reflect the edited name: {joined}"
    );
    assert!(
        !joined.contains("prelude-captured"),
        "prelude-captured name must not leak into the summary: {joined}"
    );
}

#[test]
fn edit_mode_confirm_save_shows_diff() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/old".into(),
        mounts: vec![mount("/old", "/old")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("diff-me", ws.clone()).unwrap();
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("diff-me".into(), ws);
    editor.pending.workdir = "/new".into();
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!("expected ConfirmSave");
    };
    let joined: String = modal
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(joined.contains("/old"), "old value shown: {joined}");
    assert!(joined.contains("/new"), "new value shown: {joined}");
}

#[test]
fn edit_mode_confirm_save_shows_keep_awake_toggle() {
    // A keep_awake toggle in the TUI must surface in the ConfirmSave
    // preview so the operator can see what they are confirming. The
    // on-disk write was already correct; this pins the modal preview
    // so a future refactor cannot silently re-omit the diff line.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/w".into(),
        mounts: vec![mount("/w", "/w")],
        keep_awake: KeepAwakeConfig { enabled: false },
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("ka-toggle", ws.clone()).unwrap();
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("ka-toggle".into(), ws);
    editor.pending.keep_awake.enabled = true;
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!("expected ConfirmSave");
    };
    let joined: String = modal
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        joined.contains("Keep awake"),
        "keep_awake heading shown: {joined}"
    );
    assert!(
        joined.contains("disabled") && joined.contains("enabled"),
        "both old and new keep_awake states shown: {joined}"
    );
}

// ── Source-drift safeguard (Task 10.3) ────────────────────────────

/// Stand up a workspace with a single mount whose isolated state has
/// been recorded for `container`, with `original_src` set to the
/// pre-edit value. The fixture lets the source-drift tests trigger
/// the safeguard by simply changing `editor.pending.mounts[0].src`.
fn setup_with_isolated_record(
    ws_name: &str,
    original_src: &str,
    dst: &str,
    container: &str,
) -> (TempDir, JackinPaths, AppConfig, WorkspaceConfig) {
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, IsolationRecord, write_records};

    // workdir must match a mount destination per workspace
    // validation, so anchor it on `dst`. The drift safeguard cares
    // about `src`, not `workdir`, so this doesn't perturb the test.
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: dst.into(),
        mounts: vec![MountConfig {
            src: original_src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }],
        allowed_roles: vec![],
        default_role: None,
        default_agent: None,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: KeepAwakeConfig::default(),
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        docker: None,
    };
    let (tmp, paths, config) = setup_with_workspace(ws_name, ws.clone()).unwrap();

    // Pre-write an isolation record under data_dir/<container>/.
    let cdir = paths.data_dir.join(container);
    std::fs::create_dir_all(&cdir).unwrap();
    let rec = IsolationRecord {
        workspace: ws_name.into(),
        mount_dst: dst.into(),
        original_src: original_src.into(),
        isolation: MountIsolation::Worktree,
        worktree_path: cdir.join("isolated").join(dst).display().to_string(),
        scratch_branch: format!("jackin/scratch/{container}"),
        base_commit: "deadbeef".into(),
        selector_key: container
            .trim_start_matches(crate::instance::naming::CONTAINER_PREFIX_DASH)
            .into(),
        container_name: container.into(),
        cleanup_status: CleanupStatus::Active,
    };
    write_records(&cdir, std::slice::from_ref(&rec)).unwrap();

    (tmp, paths, config, ws)
}

#[tokio::test]
async fn save_blocks_with_error_popup_when_running_container_has_drifted_state() {
    let (tmp, paths, mut config, ws) =
        setup_with_isolated_record("driftws", "/old/src", "/workspace/x", "jk-a1b2c3d4-driftws");
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("driftws".into(), ws);
    // Operator changes the src — this drifts the recorded original_src.
    editor.pending.mounts[0].src = "/new/src".into();
    state.stage = ManagerStage::Editor(editor);

    // Drive the save flow: `s` opens ConfirmSave. The test feeds an
    // async drift result into the same continuation handler the event
    // loop uses.
    press_s(&mut state, &mut config, &paths, cwd);
    let plan = match &mut state.stage {
        ManagerStage::Editor(e) => match &e.modal {
            Some(Modal::ConfirmSave { state: m }) => {
                crate::console::tui::state::PendingSaveCommit {
                    effective_removals: m.effective_removals.clone(),
                    final_mounts: m.final_mounts.clone(),
                    delete_isolated_acknowledged: false,
                    isolated_cleanup_complete: false,
                }
            }
            other => panic!("expected ConfirmSave modal; got {other:?}"),
        },
        _ => panic!("editor stage expected"),
    };
    // Drop the modal so the commit runs cleanly.
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.modal = None;
    }

    // inject FakeDockerClient: container is running
    let fake_docker = crate::docker_client::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            crate::docker_client::ContainerRow {
                name: "jk-a1b2c3d4-driftws".to_owned(),
                labels: std::collections::HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let prospective_mounts = match &state.stage {
        ManagerStage::Editor(e) => e.pending.mounts.clone(),
        _ => panic!("editor stage expected"),
    };
    let detection = crate::runtime::drift::detect_workspace_edit_drift(
        &paths,
        "driftws",
        &prospective_mounts,
        &fake_docker,
    )
    .await;
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let drift_check = crate::console::tui::state::PendingDriftCheck {
        rx,
        plan,
        exit_on_success: false,
        original_name: "driftws".into(),
    };
    super::continue_save_after_drift_check(&mut state, &mut config, drift_check, detection)
        .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        matches!(e.modal, Some(Modal::ErrorPopup { .. })),
        "running-container drift must surface as ErrorPopup; got {:?}",
        e.modal,
    );
    // On-disk config must be unchanged.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let on_disk = reloaded.workspaces.get("driftws").unwrap();
    assert_eq!(
        on_disk.mounts[0].src, "/old/src",
        "source-drift block must abort the write",
    );
}

#[tokio::test]
async fn save_opens_confirm_modal_when_stopped_container_has_drifted_state() {
    let (tmp, paths, mut config, ws) = setup_with_isolated_record(
        "driftws2",
        "/old/src",
        "/workspace/x",
        "jk-b2c3d4e5-driftws2",
    );
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("driftws2".into(), ws);
    editor.pending.mounts[0].src = "/new/src".into();
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);
    let plan = match &mut state.stage {
        ManagerStage::Editor(e) => match &e.modal {
            Some(Modal::ConfirmSave { state: m }) => {
                crate::console::tui::state::PendingSaveCommit {
                    effective_removals: m.effective_removals.clone(),
                    final_mounts: m.final_mounts.clone(),
                    delete_isolated_acknowledged: false,
                    isolated_cleanup_complete: false,
                }
            }
            other => panic!("expected ConfirmSave modal; got {other:?}"),
        },
        _ => panic!("editor stage expected"),
    };
    if let ManagerStage::Editor(e) = &mut state.stage {
        e.modal = None;
    }

    // No running container — drift lands on stopped_records and we
    // expect the confirm modal.
    // inject FakeDockerClient: no running containers
    let fake_docker = crate::docker_client::FakeDockerClient::default();
    let prospective_mounts = match &state.stage {
        ManagerStage::Editor(e) => e.pending.mounts.clone(),
        _ => panic!("editor stage expected"),
    };
    let detection = crate::runtime::drift::detect_workspace_edit_drift(
        &paths,
        "driftws2",
        &prospective_mounts,
        &fake_docker,
    )
    .await;
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let drift_check = crate::console::tui::state::PendingDriftCheck {
        rx,
        plan,
        exit_on_success: false,
        original_name: "driftws2".into(),
    };
    super::continue_save_after_drift_check(&mut state, &mut config, drift_check, detection)
        .unwrap();

    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    match &e.modal {
        Some(Modal::Confirm {
            target:
                crate::console::tui::state::ConfirmTarget::DeleteIsolatedAndSave {
                    affected_containers,
                    ..
                },
            ..
        }) => {
            assert_eq!(
                affected_containers,
                &vec!["jk-b2c3d4e5-driftws2".to_owned()],
                "modal must carry the affected container names",
            );
        }
        other => panic!("expected DeleteIsolatedAndSave Confirm modal; got {other:?}"),
    }
    // On-disk config still unchanged — we're parked on the modal.
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let on_disk = reloaded.workspaces.get("driftws2").unwrap();
    assert_eq!(on_disk.mounts[0].src, "/old/src");
}

#[test]
fn confirm_save_integrates_mount_collapse_section_when_plan_has_collapses() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/work/sub".into(),
        mounts: vec![mount("/work/sub", "/work/sub")],
        ..Default::default()
    };
    let (tmp, paths, mut config) = setup_with_workspace("collapsy", ws.clone()).unwrap();
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_edit("collapsy".into(), ws);
    editor.pending.mounts.insert(0, mount("/work", "/work"));
    state.stage = ManagerStage::Editor(editor);

    press_s(&mut state, &mut config, &paths, cwd);

    let ManagerStage::Editor(e) = &state.stage else {
        panic!();
    };
    let Some(Modal::ConfirmSave { state: modal }) = &e.modal else {
        panic!();
    };
    assert!(modal.has_collapses);
    let joined: String = modal
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        joined.contains("Mount collapse required:"),
        "collapse section heading must appear: {joined}"
    );
    assert!(
        joined.contains("will be subsumed under"),
        "collapse detail must appear: {joined}"
    );
}

#[test]
fn pre_save_diff_renders_op_ref_via_breadcrumb_not_uuid() {
    use crate::operator_env::{EnvValue, OpRef};
    use ratatui::style::Style;

    let original = std::collections::BTreeMap::new();
    let mut pending = std::collections::BTreeMap::new();
    pending.insert(
        "TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://abc/def/fld".to_owned(),
            path: "Private/Claude/auth".to_owned(),
            account: None,
        }),
    );

    let value_style = Style::default();
    let dim_style = Style::default();
    let mut lines = Vec::new();
    super::append_env_map_diff_lines(
        &mut lines,
        None,
        &original,
        &pending,
        value_style,
        dim_style,
    );

    let joined: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect::<String>();

    assert!(
        joined.contains("Private/Claude/auth"),
        "pre-save diff must render breadcrumb path; got: {joined}"
    );
    assert!(
        !joined.contains("op://abc/def/fld"),
        "UUID URI must NOT appear in pre-save diff; got: {joined}"
    );
}

#[test]
fn settings_save_general_dirty_shows_summary_and_diff() {
    use crate::config::AppConfig;
    use crate::console::tui::state::{SettingsTab, settings_state_from_config};

    let config = AppConfig::default();
    let mut settings = settings_state_from_config(&config);
    settings.active_tab = SettingsTab::General;
    // Toggle coauthor_trailer: disabled → enabled
    settings.general.pending_coauthor_trailer = true;

    let lines = super::build_settings_save_lines(&settings);
    let joined: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect();

    assert!(
        joined.contains("General"),
        "summary must mention General: {joined}"
    );
    assert!(
        joined.contains("co-author trailer"),
        "diff must show field name: {joined}"
    );
    assert!(
        joined.contains("disabled"),
        "diff must show old value: {joined}"
    );
    assert!(
        joined.contains("enabled"),
        "diff must show new value: {joined}"
    );
}

#[test]
fn settings_save_general_dco_dirty_shows_diff() {
    use crate::config::AppConfig;
    use crate::console::tui::state::{SettingsTab, settings_state_from_config};

    let config = AppConfig::default();
    let mut settings = settings_state_from_config(&config);
    settings.active_tab = SettingsTab::General;
    // Toggle dco: disabled → enabled
    settings.general.pending_dco = true;

    let lines = super::build_settings_save_lines(&settings);
    let joined: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect();

    assert!(
        joined.contains("General"),
        "summary must mention General: {joined}"
    );
    assert!(
        joined.contains("dco"),
        "diff must show dco field name: {joined}"
    );
}

#[test]
fn settings_save_general_clean_shows_no_general_section() {
    use crate::config::AppConfig;
    use crate::console::tui::state::settings_state_from_config;

    let config = AppConfig::default();
    let settings = settings_state_from_config(&config);
    // pending == original → not dirty

    let lines = super::build_settings_save_lines(&settings);
    let joined: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
        .collect();

    // When nothing changed, neither the summary row nor the detail row appears.
    assert!(
        !joined.contains("co-author trailer"),
        "clean state must not render co-author trailer field: {joined}"
    );
    assert!(
        !joined.contains("dco"),
        "clean state must not render dco field: {joined}"
    );
}
