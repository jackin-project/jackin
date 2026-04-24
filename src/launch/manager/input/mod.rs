//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

pub(super) mod editor;
pub(super) mod list;
pub(super) mod mouse;
pub(super) mod prelude;
pub(super) mod save;

use crossterm::event::KeyEvent;

use super::super::widgets::ModalOutcome;
use super::state::{
    EditorSaveFlow, EditorState, ExitIntent, ManagerStage, ManagerState, Toast, ToastKind,
};
use crate::config::AppConfig;
use crate::paths::JackinPaths;

pub use mouse::handle_mouse;

#[derive(Debug)]
pub enum InputOutcome {
    /// Stay in the manager.
    Continue,
    /// Exit jackin entirely (Esc/q from the manager list).
    ExitJackin,
    /// Launch the named workspace — resolved by name in `run_launch`.
    LaunchNamed(String),
    /// Launch against the synthetic "Current directory" choice (row 0).
    /// `run_launch` routes this through the same agent-picker path as
    /// `LaunchNamed`, using `LaunchState::workspaces[0]` which is built
    /// in `LaunchState::new` from the current cwd.
    LaunchCurrentDir,
}

#[allow(clippy::too_many_lines)]
pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // List-level modal precedence (e.g. GithubPicker opened from `o` on a
    // workspace row). Handled before stage-specific modals so the dispatch
    // stays uniform whatever stage the state thinks it's in.
    if state.list_modal.is_some() {
        list::handle_list_modal(state, key);
        return Ok(InputOutcome::Continue);
    }
    // Modal precedence: if a modal is open, it gets the event.
    // Use a discriminant check so we can take &mut without keeping an
    // immutable borrow alive across the call.
    if let ManagerStage::Editor(editor) = &mut state.stage
        && editor.modal.is_some()
    {
        editor::handle_editor_modal(editor, key);

        // Drain the ConfirmSave → commit signal FIRST. The modal handler
        // only closes the modal and stashes the plan; this outer layer
        // has `paths`/`cwd` and actually performs the write.
        let pending = if let ManagerStage::Editor(editor) = &mut state.stage {
            match std::mem::replace(&mut editor.save_flow, EditorSaveFlow::Idle) {
                EditorSaveFlow::PendingCommit {
                    plan,
                    exit_on_success,
                } => Some((plan, exit_on_success)),
                other => {
                    // Not a commit transition — put the flow back untouched.
                    editor.save_flow = other;
                    None
                }
            }
        } else {
            None
        };
        if let Some((plan, exit_on_success)) = pending {
            save::commit_editor_save(state, config, paths, cwd, plan, exit_on_success)?;
            return Ok(InputOutcome::Continue);
        }

        // After modal handling, check if an exit intent was signalled by
        // the SaveDiscardCancel modal.
        let intent = if let ManagerStage::Editor(editor) = &state.stage {
            editor.exit_after_save
        } else {
            None
        };
        if let Some(intent) = intent {
            match intent {
                ExitIntent::Save => {
                    // Route through the two-phase save: ConfirmSave opens
                    // first; the eventual commit is the one that exits.
                    // Pass `exit_on_success = true` so that, if the operator
                    // picks Save in the confirm dialog and the write
                    // succeeds, we bounce out to the workspace list.
                    if let ManagerStage::Editor(e) = &mut state.stage {
                        e.exit_after_save = None;
                    }
                    save::begin_editor_save(state, config, true)?;
                }
                ExitIntent::Discard => {
                    *state = ManagerState::from_config(config, cwd);
                }
            }
            return Ok(InputOutcome::Continue);
        }
        return Ok(InputOutcome::Continue);
    }
    if matches!(state.stage, ManagerStage::CreatePrelude(_)) {
        let has_modal = if let ManagerStage::CreatePrelude(p) = &state.stage {
            p.modal.is_some()
        } else {
            false
        };
        if has_modal {
            if let ManagerStage::CreatePrelude(p) = &mut state.stage {
                prelude::handle_prelude_modal(p, key);
            }
            // After the modal handler runs, the prelude is in one of three states:
            // - still in a modal (user pressed a non-commit/cancel key): continue
            // - modal cleared + completed() Some: wizard done → transition to Editor
            // - modal cleared + completed() None: wizard cancelled → back to List
            //
            // `completed()` checks every required field together and
            // returns the owned (name, ws) pair so we don't need a
            // separate `pending_name.is_some()` flag plus an
            // `expect("prelude complete")` to keep the two invariants
            // in sync.
            #[allow(clippy::items_after_statements)]
            enum PreludeStatus {
                InProgress,
                Complete(String, crate::workspace::WorkspaceConfig),
                Cancelled,
            }
            let status = if let ManagerStage::CreatePrelude(p) = &state.stage {
                if p.modal.is_some() {
                    PreludeStatus::InProgress
                } else if let Some((name, ws)) = p.completed() {
                    PreludeStatus::Complete(name, ws)
                } else {
                    PreludeStatus::Cancelled
                }
            } else {
                PreludeStatus::InProgress
            };
            match status {
                PreludeStatus::Complete(name, ws) => {
                    let mut editor = EditorState::new_create();
                    editor.pending = ws;
                    editor.pending_name = Some(name);
                    state.stage = ManagerStage::Editor(editor);
                }
                PreludeStatus::Cancelled => {
                    *state = ManagerState::from_config(config, cwd);
                }
                PreludeStatus::InProgress => {}
            }
            return Ok(InputOutcome::Continue);
        }
    }

    // Non-modal routing per stage — capture which stage we're in as a
    // simple enum discriminant so the immutable borrow ends before we
    // pass &mut state into the stage handler.
    #[allow(clippy::items_after_statements)]
    enum StageDis {
        List,
        Editor,
        CreatePrelude,
        ConfirmDelete,
    }
    let dis = match &state.stage {
        ManagerStage::List => StageDis::List,
        ManagerStage::Editor(_) => StageDis::Editor,
        ManagerStage::CreatePrelude(_) => StageDis::CreatePrelude,
        ManagerStage::ConfirmDelete { .. } => StageDis::ConfirmDelete,
    };

    match dis {
        StageDis::List => list::handle_list_key(state, config, paths, cwd, key),
        StageDis::Editor => editor::handle_editor_key(state, config, paths, cwd, key),
        StageDis::CreatePrelude => Ok(prelude::handle_prelude_key(state, config, paths, cwd, key)),
        StageDis::ConfirmDelete => handle_confirm_delete_key(state, config, paths, cwd, key),
    }
}

fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    let ManagerStage::ConfirmDelete {
        name,
        state: confirm_state,
    } = &mut state.stage
    else {
        return Ok(InputOutcome::Continue);
    };
    let outcome = confirm_state.handle_key(key);
    let ws_name = name.clone();
    match outcome {
        ModalOutcome::Commit(true) => {
            let mut editor = crate::config::ConfigEditor::open(paths)?;
            editor.remove_workspace(&ws_name)?;
            *config = editor.save()?;
            *state = ManagerState::from_config(config, cwd);
            state.toast = Some(Toast {
                message: format!("deleted \"{ws_name}\""),
                kind: ToastKind::Success,
                shown_at: std::time::Instant::now(),
            });
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            state.stage = ManagerStage::List;
            Ok(InputOutcome::Continue)
        }
        ModalOutcome::Continue => Ok(InputOutcome::Continue),
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    //! Tests for the mount-collapse confirm flow in `save_editor`.
    //!
    //! These exercise the editor-side integration of
    //! `workspace::planner::plan_edit`: the editor must intercept collapse
    //! decisions before calling into `ConfigEditor::edit_workspace`, prompt
    //! the operator, and write only on approval.
    use super::super::state::EditorTab;
    use super::super::state::{
        EditorMode, EditorSaveFlow, EditorState, FieldFocus, FileBrowserTarget, ManagerStage,
        ManagerState, Modal, ToastKind,
    };
    use super::editor::{
        apply_file_browser_to_editor, apply_text_input_to_pending, handle_editor_modal,
    };
    use super::prelude::handle_prelude_modal;
    use super::save::{begin_editor_save, commit_editor_save};
    use super::*;
    use crate::config::AppConfig;
    use crate::paths::JackinPaths;
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tempfile::TempDir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
        }
    }

    fn ro_mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: true,
        }
    }

    /// Persist an `AppConfig` with one workspace to a test `JackinPaths`.
    fn setup_with_workspace(
        name: &str,
        ws: WorkspaceConfig,
    ) -> anyhow::Result<(TempDir, JackinPaths, AppConfig)> {
        let tmp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs()?;

        let mut config = AppConfig::default();
        config.workspaces.insert(name.to_string(), ws);
        let toml = toml::to_string(&config)?;
        std::fs::write(&paths.config_file, toml)?;

        let reloaded = AppConfig::load_or_init(&paths)?;
        Ok((tmp, paths, reloaded))
    }

    /// Press `s` in the editor. Convenience helper that routes through
    /// the public `handle_key` to mirror real operator input.
    fn press_s(
        state: &mut ManagerState<'_>,
        config: &mut AppConfig,
        paths: &JackinPaths,
        cwd: &std::path::Path,
    ) {
        handle_key(state, config, paths, cwd, key(KeyCode::Char('s'))).unwrap();
    }

    #[test]
    fn save_editor_opens_confirm_save_on_edit_driven_collapse() {
        // Existing workspace with /work/sub; operator adds /work which
        // subsumes the child. Expected: ConfirmSave modal opens with a
        // "Mount collapse required" section; no write yet.
        let ws = WorkspaceConfig {
            workdir: "/work/sub".into(),
            mounts: vec![mount("/work/sub", "/work/sub")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

        let cwd = _tmp.path();
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
            workdir: "/work/sub".into(),
            mounts: vec![mount("/work/sub", "/work/sub")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        editor.pending.mounts.insert(0, mount("/work", "/work"));
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);

        // Step 2: Enter on the ConfirmSave modal (default focus = Save)
        // commits the save.
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.modal.is_none(),
            "modal should be closed after confirm; got {:?}",
            e.modal
        );
        assert!(
            !e.save_flow.is_error(),
            "save should have succeeded: {:?}",
            e.save_flow.error_message()
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
            workdir: "/work/sub".into(),
            mounts: vec![mount("/work/sub", "/work/sub")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

        let cwd = _tmp.path();
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
    fn readonly_mismatch_produces_error_banner_no_write() {
        // Add a rw /work that would subsume an existing ro /work/sub —
        // plan_edit must reject with ReadonlyMismatch. Per spec, hard
        // planner errors surface as an inline banner, NOT as the new
        // ErrorPopup (which is reserved for commit-time failures).
        let ws = WorkspaceConfig {
            workdir: "/work/sub".into(),
            mounts: vec![ro_mount("/work/sub", "/work/sub")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("big-monorepo", ws.clone()).unwrap();

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        editor.pending.mounts.insert(0, mount("/work", "/work")); // rw
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(e.modal.is_none(), "no modal for hard planner errors");
        let banner = e
            .save_flow
            .error_message()
            .expect("readonly mismatch should produce banner");
        assert!(
            banner.contains("readonly"),
            "banner should mention readonly: {banner}"
        );
        // On-disk config unchanged.
        let reloaded = AppConfig::load_or_init(&paths).unwrap();
        let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
        assert_eq!(ws_on_disk.mounts.len(), 1);
    }

    #[test]
    fn pre_existing_collapse_produces_prune_error_banner() {
        let ws = WorkspaceConfig {
            workdir: "/work".into(),
            mounts: vec![
                mount("/work", "/work"),
                mount("/work/sub", "/work/sub"), // already redundant
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) =
            setup_with_workspace("legacy-workspace", ws.clone()).unwrap();

        let cwd = _tmp.path();
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
        assert!(e.modal.is_none(), "no confirm for pre-existing-only case");
        let banner = e
            .save_flow
            .error_message()
            .expect("pre-existing collapse should produce banner");
        assert!(
            banner.contains("prune"),
            "banner should reference `workspace prune`: {banner}"
        );
        assert!(
            banner.contains("legacy-workspace"),
            "banner should name the workspace: {banner}"
        );
    }

    // ── New behavioural tests for the two-phase save flow ─────────────

    #[test]
    fn s_with_zero_changes_is_noop() {
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("clean-ws", ws.clone()).unwrap();

        let cwd = _tmp.path();
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
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("edit-me", ws.clone()).unwrap();

        let cwd = _tmp.path();
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
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("exit-me", ws.clone()).unwrap();

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("exit-me".into(), ws);
        editor.pending.workdir = "/w/elsewhere".into();
        state.stage = ManagerStage::Editor(editor);

        begin_editor_save(&mut state, &config, true).unwrap();
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        assert!(
            matches!(state.stage, ManagerStage::List),
            "save with exit_on_success = true should return to the list stage"
        );
    }

    #[test]
    fn exit_on_success_save_preserves_success_toast_across_state_refresh() {
        // Finding #3: when `commit_editor_save` exits to the list view,
        // it reinitialises the whole `ManagerState` via
        // `ManagerState::from_config` — which allocates `toast: None`.
        // That discarded the success toast the same function had just
        // set. Verify the carry-across keeps it intact so the operator
        // still sees the positive feedback after the reset.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (tmp, paths, mut config) = setup_with_workspace("toast-me", ws.clone()).unwrap();

        let cwd = tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("toast-me".into(), ws);
        editor.pending.workdir = "/w/elsewhere".into();
        state.stage = ManagerStage::Editor(editor);

        begin_editor_save(&mut state, &config, true).unwrap();
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        assert!(
            matches!(state.stage, ManagerStage::List),
            "exit_on_success should land us in the list; got {:?}",
            state.stage,
        );
        let toast = state
            .toast
            .as_ref()
            .expect("success toast must survive the exit-to-list reset");
        assert!(
            matches!(toast.kind, ToastKind::Success),
            "carried-across toast must be the Success kind; got {:?}",
            toast.kind,
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
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
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
        let bad_plan = crate::launch::manager::state::PendingSaveCommit {
            effective_removals: vec!["/does/not/exist".to_string()],
            final_mounts: None,
        };
        commit_editor_save(&mut state, &mut config, &paths, cwd, bad_plan, false).unwrap();

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
    fn create_mode_save_preserves_success_toast_across_state_refresh() {
        // Create mode also goes through the `ManagerState::from_config`
        // reset. Same regression guard as the Edit-with-exit flow above.
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
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        assert!(
            matches!(state.stage, ManagerStage::List),
            "create save should return to the list; got {:?}",
            state.stage,
        );
        let toast = state
            .toast
            .as_ref()
            .expect("create-save success toast must survive the reset");
        assert!(matches!(toast.kind, ToastKind::Success));
    }

    #[test]
    fn confirm_save_save_stays_in_editor_on_success_from_direct_s() {
        // Bare `s` press (not from SaveDiscardCancel) keeps the operator
        // in the editor after a successful save.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("stay-here", ws.clone()).unwrap();

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("stay-here".into(), ws);
        editor.pending.workdir = "/w/new".into();
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("should stay in editor on direct `s` save");
        };
        assert!(e.modal.is_none());
        // Origin-of-truth refreshed so the editor is clean again.
        assert_eq!(e.change_count(), 0);
    }

    #[test]
    fn confirm_save_save_opens_error_popup_on_duplicate_name() {
        // Two workspaces on disk; rename one to the other's name. The
        // write hits ConfigEditor::rename_workspace's duplicate-name
        // guard and we expect an ErrorPopup.
        let ws_a = WorkspaceConfig {
            workdir: "/a".into(),
            mounts: vec![mount("/a", "/a")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let ws_b = WorkspaceConfig {
            workdir: "/b".into(),
            mounts: vec![mount("/b", "/b")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, _config0) = setup_with_workspace("alpha", ws_a.clone()).unwrap();
        // Add the second workspace on disk.
        let mut config = {
            let mut ce = crate::config::ConfigEditor::open(&paths).unwrap();
            ce.create_workspace("beta", ws_b.clone()).unwrap();
            ce.save().unwrap()
        };

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("alpha".into(), ws_a);
        editor.pending_name = Some("beta".into()); // collides
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

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
            workdir: "/a".into(),
            mounts: vec![mount("/a", "/a")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let ws_b = WorkspaceConfig {
            workdir: "/b".into(),
            mounts: vec![mount("/b", "/b")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, _config0) = setup_with_workspace("alpha", ws_a.clone()).unwrap();
        let mut config = {
            let mut ce = crate::config::ConfigEditor::open(&paths).unwrap();
            ce.create_workspace("beta", ws_b.clone()).unwrap();
            ce.save().unwrap()
        };

        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("alpha".into(), ws_a);
        editor.pending_name = Some("beta".into());
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
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
        let (_tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = _tmp.path();
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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
    fn create_mode_enter_on_name_row_opens_rename_modal() {
        // In Create mode, pressing Enter on row 0 (Name) must open the
        // rename TextInput modal pre-filled with the current pending_name
        // — the same flow Edit mode uses. This is the operator's escape
        // hatch from a prelude-captured name they mistyped.
        let (_tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_create();
        editor.pending_name = Some("typo-name".into());
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("still in editor after Enter on name row");
        };
        match &e.modal {
            Some(Modal::TextInput { target, state }) => {
                assert_eq!(*target, super::super::state::TextInputTarget::Name);
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

        apply_text_input_to_pending(
            super::super::state::TextInputTarget::Name,
            &mut editor,
            "new-name",
        );

        assert_eq!(editor.pending_name.as_deref(), Some("new-name"));
    }

    #[test]
    fn create_mode_save_uses_updated_pending_name() {
        // End-to-end: start Create, rename via Enter-on-row-0, commit the
        // save, and verify the workspace on disk has the updated name.
        let (_tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_create();
        editor.pending_name = Some("original".into());
        editor.pending.workdir = "/code/proj".into();
        editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        // Open rename modal via Enter on row 0.
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        // Clear the pre-filled "original" and type "renamed".
        for _ in 0..8 {
            handle_key(
                &mut state,
                &mut config,
                &paths,
                cwd,
                key(KeyCode::Backspace),
            )
            .unwrap();
        }
        for ch in "renamed".chars() {
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch))).unwrap();
        }
        // Commit the TextInput.
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        // Kick off the save: `s` → ConfirmSave → Enter commits.
        press_s(&mut state, &mut config, &paths, cwd);
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let reloaded = AppConfig::load_or_init(&paths).unwrap();
        assert!(
            reloaded.workspaces.contains_key("renamed"),
            "save must persist the edited name; got workspaces={:?}",
            reloaded.workspaces.keys().collect::<Vec<_>>()
        );
        assert!(
            !reloaded.workspaces.contains_key("original"),
            "the original (pre-edit) name must not end up on disk"
        );
    }

    #[test]
    fn create_mode_confirm_save_reflects_renamed_workspace_name() {
        // The ConfirmSave dialog's first line reads
        // "Create workspace: <name>" — after an in-editor rename, the
        // summary must pick up the edited name, not the prelude-captured one.
        let (_tmp, paths, mut config) = {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let toml = toml::to_string(&config).unwrap();
            std::fs::write(&paths.config_file, toml).unwrap();
            let loaded = AppConfig::load_or_init(&paths).unwrap();
            (tmp, paths, loaded)
        };
        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_create();
        editor.pending_name = Some("prelude-captured".into());
        editor.pending.workdir = "/code/proj".into();
        editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
        state.stage = ManagerStage::Editor(editor);

        // Operator renames mid-edit.
        apply_text_input_to_pending(
            super::super::state::TextInputTarget::Name,
            match &mut state.stage {
                ManagerStage::Editor(e) => e,
                _ => unreachable!(),
            },
            "edited-in-place",
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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
    fn edit_mode_enter_on_name_row_still_opens_rename_modal() {
        // Regression guard: the Create-mode extension to row 0 Enter must
        // not break the Edit-mode path that already worked.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("keep-me", ws.clone()).unwrap();
        let cwd = _tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("keep-me".into(), ws);
        editor.active_field = FieldFocus::Row(0);
        state.stage = ManagerStage::Editor(editor);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!();
        };
        match &e.modal {
            Some(Modal::TextInput { target, state }) => {
                assert_eq!(*target, super::super::state::TextInputTarget::Name);
                assert_eq!(state.value(), "keep-me");
            }
            other => panic!("expected TextInput(Name); got {other:?}"),
        }
    }

    #[test]
    fn edit_mode_confirm_save_shows_diff() {
        let ws = WorkspaceConfig {
            workdir: "/old".into(),
            mounts: vec![mount("/old", "/old")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("diff-me", ws.clone()).unwrap();
        let cwd = _tmp.path();
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("|");
        assert!(joined.contains("/old"), "old value shown: {joined}");
        assert!(joined.contains("/new"), "new value shown: {joined}");
    }

    #[test]
    fn confirm_save_integrates_mount_collapse_section_when_plan_has_collapses() {
        let ws = WorkspaceConfig {
            workdir: "/work/sub".into(),
            mounts: vec![mount("/work/sub", "/work/sub")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (_tmp, paths, mut config) = setup_with_workspace("collapsy", ws.clone()).unwrap();
        let cwd = _tmp.path();
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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

    /// Current-directory row (index 0) must reject the `e` edit shortcut and
    /// the `d` delete shortcut with a toast, without entering the Editor or
    /// ConfirmDelete stages. Paired with the render-side assertion that row 0
    /// is labelled "Current directory".
    #[test]
    fn current_directory_row_rejects_edit_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        // Minimal config with one saved workspace so the list has a non-
        // trivial shape (current-dir + one saved + sentinel).
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "some-ws".into(),
            WorkspaceConfig {
                workdir: "/unrelated".into(),
                mounts: vec![],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        // cwd is unrelated to /unrelated, so preselect falls back to row 0.
        assert_eq!(state.selected, 0);

        // Press `e` — must produce a toast and remain in the List stage.
        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('e')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "e on row 0 must not open the Editor; got {:?}",
            state.stage
        );
        let toast = state.toast.as_ref().expect("edit rejection must toast");
        assert!(
            matches!(toast.kind, ToastKind::Error),
            "edit rejection must be an error toast"
        );
        assert!(
            toast.message.contains("edit"),
            "toast should mention edit: {}",
            toast.message
        );
        state.toast = None;

        // Press `d` — must produce a toast and remain in the List stage
        // (no ConfirmDelete transition).
        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('d')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "d on row 0 must not open ConfirmDelete; got {:?}",
            state.stage
        );
        let toast = state.toast.as_ref().expect("delete rejection must toast");
        assert!(
            matches!(toast.kind, ToastKind::Error),
            "delete rejection must be an error toast"
        );
        assert!(
            toast.message.contains("delete"),
            "toast should mention delete: {}",
            toast.message
        );
    }

    /// Enter on row 0 returns `LaunchCurrentDir`; Enter on row 1 returns
    /// `LaunchNamed(<name>)`. Pins the index arithmetic that maps list-row
    /// indices to launch targets.
    #[test]
    fn enter_on_current_directory_returns_launch_current_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        assert!(
            matches!(outcome, InputOutcome::LaunchCurrentDir),
            "row 0 Enter must produce LaunchCurrentDir"
        );

        state.selected = 1;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        match outcome {
            InputOutcome::LaunchNamed(name) => assert_eq!(name, "alpha"),
            other => panic!("row 1 Enter must produce LaunchNamed(\"alpha\"); got {other:?}"),
        }
    }

    // ── Editor FileBrowser → MountDstChoice behavioral tests ────────────

    /// Build an editor sitting on the Mounts tab with an empty mount list,
    /// and simulate the commit of a FileBrowser at `/host/path`. The bridge
    /// function is `apply_file_browser_to_editor`, which opens the new
    /// `MountDstChoice` modal instead of the old "push + TextInput" chain.
    fn editor_with_browser_committed(src: &str) -> EditorState<'static> {
        use crate::launch::manager::state::{EditorTab, FieldFocus};
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        apply_file_browser_to_editor(
            FileBrowserTarget::EditAddMountSrc,
            &mut editor,
            std::path::PathBuf::from(src),
        );
        editor
    }

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
    fn editor_ok_commits_mount_with_dst_equal_src() {
        // OK shortcut on the choice modal → push MountConfig with dst = src
        // and close the modal. No TextInput should appear.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_editor_modal(&mut editor, key(KeyCode::Char('o')));
        assert!(
            editor.modal.is_none(),
            "OK must close the modal; got {:?}",
            editor.modal
        );
        assert_eq!(editor.pending.mounts.len(), 1, "exactly one mount pushed");
        let m = &editor.pending.mounts[0];
        assert_eq!(m.src, "/host/path");
        assert_eq!(m.dst, "/host/path", "OK fast-path sets dst = src");
        assert!(!m.readonly);
    }

    #[test]
    fn editor_edit_opens_textinput_and_pushes_provisional() {
        // Edit destination → push provisional mount (dst = src) + open
        // the TextInput pre-filled with src. Mirrors today's flow so the
        // operator can edit dst in place.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_editor_modal(&mut editor, key(KeyCode::Char('e')));
        match &editor.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(*target, super::super::state::TextInputTarget::MountDst);
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

    // ── Prelude FileBrowser → MountDstChoice behavioral tests ──────────

    /// Seed a `CreatePreludeState` whose `MountDstChoice` modal is open
    /// for `src`. Mirrors the state the `FileBrowserSrc::Commit` branch of
    /// `handle_prelude_modal` leaves the prelude in, without needing to
    /// synthesise a FileBrowser `Commit(path)` event (no public way to do
    /// that cleanly from outside the widget).
    fn prelude_with_browser_committed(
        src: &str,
    ) -> super::super::state::CreatePreludeState<'static> {
        let mut prelude = super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(std::path::PathBuf::from(src));
        prelude.modal = Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(src),
        });
        prelude
    }

    #[test]
    fn prelude_ok_chains_to_workdir_pick_with_dst_equal_src() {
        // OK on the choice modal should: (a) set prelude.pending_mount_dst
        // to src, (b) advance the step to PickWorkdir, (c) open the
        // WorkdirPick modal pre-loaded with the staged mount.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('o')));

        assert!(
            matches!(prelude.modal, Some(Modal::WorkdirPick { .. })),
            "OK must chain to WorkdirPick; got {:?}",
            prelude.modal
        );
        assert_eq!(
            prelude.pending_mount_dst.as_deref(),
            Some("/home/user/project"),
            "OK fast-path stores dst = src on the prelude"
        );
        assert!(!prelude.pending_readonly);
        assert!(matches!(
            prelude.step,
            super::super::state::CreateStep::PickWorkdir
        ));
    }

    #[test]
    fn prelude_edit_opens_textinput_preserving_chain_to_workdir_pick() {
        // Edit destination on the choice modal must open a TextInput
        // pre-filled with the src (today's flow). The TextInputDst
        // commit branch then advances to WorkdirPick — so this test pins
        // that the Edit-path does not short-circuit; the chain continues
        // through TextInput like before.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));

        match &prelude.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(*target, super::super::state::TextInputTarget::MountDst);
            }
            other => panic!("expected TextInput(MountDst); got {other:?}"),
        }
        // Edit must not itself store a dst — the TextInput commit will.
        assert!(prelude.pending_mount_dst.is_none());
        // The prelude's internal step is still PickFirstMountDst (not
        // advanced yet) — TextInput commit is what calls accept_mount_dst.
        assert!(matches!(
            prelude.step,
            super::super::state::CreateStep::PickFirstMountDst
        ));
    }

    #[test]
    fn prelude_cancel_on_mount_dst_choice_rewinds_to_file_browser() {
        // Esc on MountDstChoice must not close the wizard — it must
        // step back to FileBrowserSrc so the operator can pick a
        // different source folder without losing state.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::FileBrowser { .. })),
            "Esc on MountDstChoice must reopen FileBrowser; got {:?}",
            prelude.modal
        );
        assert!(
            prelude.pending_mount_dst.is_none(),
            "Cancel must not store a dst"
        );
    }

    #[test]
    fn prelude_esc_at_mount_dst_choice_returns_to_file_browser_at_last_cwd() {
        // Step-back from MountDstChoice must reopen FileBrowser seeded at
        // the last cwd the browser was pointing at when src was committed.
        // The FileBrowser root is always `$HOME`, so the restored cwd has
        // to live inside `$HOME` — we use `$HOME` itself which is always
        // a valid target for `set_cwd` to honour.
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .expect("resolve $HOME");

        let mut prelude = super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(home.clone());
        prelude.last_browser_cwd = Some(home.clone());
        prelude.modal = Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(
                &home.display().to_string(),
            ),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));

        match &prelude.modal {
            Some(Modal::FileBrowser { state, .. }) => {
                let cwd = state.cwd().to_path_buf();
                assert!(
                    cwd == home || cwd.starts_with(&home),
                    "FileBrowser should restore a cwd inside $HOME (got {cwd:?})"
                );
            }
            other => panic!("expected FileBrowser, got {other:?}"),
        }
    }

    #[test]
    fn prelude_esc_at_text_input_dst_returns_to_mount_dst_choice() {
        // Tapping "Edit destination" opens TextInputDst; Esc inside that
        // TextInput must rewind to the MountDstChoice modal — not close
        // the wizard.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        // Choose the Edit branch to open the TextInput.
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));
        assert!(matches!(prelude.modal, Some(Modal::TextInput { .. })));

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::MountDstChoice { .. })),
            "Esc on TextInputDst must reopen MountDstChoice; got {:?}",
            prelude.modal
        );
    }

    #[test]
    fn prelude_esc_at_workdir_pick_returns_to_mount_dst_choice_fast_path() {
        // When the operator took the OK (fast path) for dst, Esc on
        // WorkdirPick must step back to MountDstChoice.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('o'))); // OK → WorkdirPick
        assert!(matches!(prelude.modal, Some(Modal::WorkdirPick { .. })));

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::MountDstChoice { .. })),
            "Esc on WorkdirPick (fast-path) must rewind to MountDstChoice; got {:?}",
            prelude.modal
        );
    }

    #[test]
    fn prelude_esc_at_workdir_pick_returns_to_text_input_dst_when_edit_used() {
        // When the operator took the Edit branch, Esc on WorkdirPick must
        // rewind to the TextInputDst step so they can retry the typed dst.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e'))); // open TextInputDst
        // Simulate commit of typed dst (Enter closes TextInput) by
        // advancing the modal directly to WorkdirPick — we only care
        // about `used_edit_dst` state at this point.
        prelude.used_edit_dst = true;
        prelude.accept_mount_dst("/home/user/project".into(), false);
        prelude.modal = Some(Modal::WorkdirPick {
            state: crate::launch::widgets::workdir_pick::WorkdirPickState::from_mounts(&[
                crate::workspace::MountConfig {
                    src: "/home/user/project".into(),
                    dst: "/home/user/project".into(),
                    readonly: false,
                },
            ]),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        match &prelude.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(*target, super::super::state::TextInputTarget::MountDst);
            }
            other => panic!("expected TextInput(MountDst); got {other:?}"),
        }
    }

    #[test]
    fn prelude_esc_at_name_step_returns_to_workdir_pick() {
        // Name is the last step in the wizard — Esc on TextInputName
        // must rewind to WorkdirPick so the operator can change the
        // workdir without abandoning the partial workspace.
        let mut prelude = super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(std::path::PathBuf::from("/home/user/project"));
        prelude.accept_mount_dst("/home/user/project".into(), false);
        prelude.accept_workdir("/home/user/project".into());
        prelude.modal = Some(Modal::TextInput {
            target: super::super::state::TextInputTarget::Name,
            state: crate::launch::widgets::text_input::TextInputState::new(
                "Name this workspace",
                "project",
            ),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::WorkdirPick { .. })),
            "Esc on TextInputName must reopen WorkdirPick; got {:?}",
            prelude.modal
        );
        assert!(prelude.pending_name.is_none(), "Esc must not commit a name");
    }

    #[test]
    fn prelude_esc_at_file_browser_src_returns_to_list() {
        // Step 1 (FileBrowserSrc) has no prior state to restore — Esc
        // must close the modal so the outer dispatcher drops back to
        // the workspace list (today's "cancelled" contract).
        let mut prelude = super::super::state::CreatePreludeState::new();
        let fb = crate::launch::widgets::file_browser::FileBrowserState::new_from_home()
            .expect("file browser should build in test env");
        prelude.modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            prelude.modal.is_none(),
            "Esc on FileBrowserSrc must close the modal; got {:?}",
            prelude.modal
        );
        assert!(prelude.pending_name.is_none());
    }

    #[test]
    fn editor_cancel_does_not_push_mount() {
        // C / Esc dismisses the choice modal without touching pending.mounts.
        let mut editor = editor_with_browser_committed("/host/path");
        handle_editor_modal(&mut editor, key(KeyCode::Esc));
        assert!(editor.modal.is_none(), "Esc closes the modal");
        assert_eq!(
            editor.pending.mounts.len(),
            0,
            "Cancel must not push a mount"
        );

        let mut editor = editor_with_browser_committed("/host/path");
        handle_editor_modal(&mut editor, key(KeyCode::Char('c')));
        assert!(editor.modal.is_none(), "`c` closes the modal");
        assert_eq!(editor.pending.mounts.len(), 0, "`c` must not push a mount");
    }

    // ── Editor Left/Right = prev/next tab ──────────────────────────────

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
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = start_tab;
        state.stage = ManagerStage::Editor(editor);
        (state, config, paths, tmp)
    }

    #[test]
    fn editor_right_arrow_advances_tab() {
        // Right should match Tab's forward cycle: General → Mounts.
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
        assert_eq!(e.active_tab, EditorTab::Mounts);
    }

    #[test]
    fn editor_left_arrow_rewinds_tab() {
        // Left should match BackTab's reverse cycle: Mounts → General.
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
        assert_eq!(e.active_tab, EditorTab::General);
    }

    #[test]
    fn editor_left_wraps_to_last_tab_from_first() {
        // Match Tab's wrap contract: Left from General → Secrets.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::General);
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
        assert_eq!(e.active_tab, EditorTab::Secrets);
    }

    #[test]
    fn editor_right_wraps_to_first_tab_from_last() {
        // Match Tab's wrap contract: Right from Secrets → General.
        let (mut state, mut config, paths, tmp) = editor_state_on_tab(EditorTab::Secrets);
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
        assert_eq!(e.active_tab, EditorTab::General);
    }

    // ── List-view `o` key → GitHub resolver + picker ──────────────────

    /// Build a git repo under `root` with a `github.com` origin remote on
    /// `branch`. Returns the path so callers can use it as a mount src.
    fn make_github_repo(root: &std::path::Path, name: &str, branch: &str) -> std::path::PathBuf {
        let path = root.join(name);
        let git_dir = path.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
        std::fs::write(
            git_dir.join("config"),
            format!("[remote \"origin\"]\n    url = git@github.com:owner/{name}.git\n"),
        )
        .unwrap();
        path
    }

    #[test]
    fn resolve_github_mounts_returns_one_per_github_repo() {
        // A workspace with two github mounts + one folder + one gitlab repo
        // should yield exactly two picker choices.
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "dev");
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        // Gitlab repo should be skipped.
        let gitlab = tmp.path().join("gl");
        let gl_git = gitlab.join(".git");
        std::fs::create_dir_all(&gl_git).unwrap();
        std::fs::write(gl_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            gl_git.join("config"),
            "[remote \"origin\"]\n    url = git@gitlab.com:owner/repo.git\n",
        )
        .unwrap();

        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(plain.to_str().unwrap(), "/p"),
                mount(repo_b.to_str().unwrap(), "/b"),
                mount(gitlab.to_str().unwrap(), "/g"),
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };

        let choices = crate::launch::manager::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 2);
        // URLs track the HEAD ref per-repo.
        let urls: Vec<&str> = choices.iter().map(|c| c.url.as_str()).collect();
        assert!(urls.contains(&"https://github.com/owner/repo-a/tree/main"));
        assert!(urls.contains(&"https://github.com/owner/repo-b/tree/dev"));
        // Branch label matches Named variant.
        let branches: Vec<&str> = choices.iter().map(|c| c.branch.as_str()).collect();
        assert!(branches.contains(&"main"));
        assert!(branches.contains(&"dev"));
    }

    /// Helper: seed an AppConfig + ManagerState with `ws` as a saved workspace,
    /// cwd far away so selection lands on row 1 (the saved workspace).
    fn list_state_selecting_ws(
        ws: WorkspaceConfig,
    ) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.workspaces.insert("demo".into(), ws);
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1; // force selection onto the saved workspace row
        (state, config, paths, tmp)
    }

    #[test]
    fn list_o_with_single_github_mount_has_one_resolved_url() {
        // Resolver-side check — we can't cleanly assert `open::that_detached`
        // ran, but we can pin that there's exactly one URL to hand to it so
        // the 1-mount branch's immediate-open path is taken.
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_github_repo(tmp.path(), "solo", "trunk");
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![mount(repo.to_str().unwrap(), "/solo")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let choices = crate::launch::manager::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].url, "https://github.com/owner/solo/tree/trunk");
    }

    #[test]
    fn list_o_with_multiple_github_mounts_opens_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "main");
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(repo_b.to_str().unwrap(), "/b"),
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        match &state.list_modal {
            Some(Modal::GithubPicker { state: picker }) => {
                assert_eq!(picker.choices.len(), 2);
            }
            other => panic!("expected GithubPicker modal; got {other:?}"),
        }
    }

    #[test]
    fn list_o_with_zero_github_mounts_shows_toast() {
        let tmp_src = tempfile::tempdir().unwrap();
        let plain = tmp_src.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![mount(plain.to_str().unwrap(), "/p")],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        assert!(
            state.list_modal.is_none(),
            "no modal should open when there are no github mounts"
        );
        let toast = state.toast.as_ref().expect("expected a toast");
        assert!(
            toast.message.contains("no GitHub URL"),
            "toast should explain the no-mounts state: {}",
            toast.message
        );
    }

    #[test]
    fn list_o_on_row_zero_toasts_no_workspace_selected() {
        // Row 0 is the synthetic "Current directory" — no saved workspace
        // to read mounts from; hint should nudge the operator, not crash.
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: String::new(),
                mounts: vec![],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 0;

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        let toast = state.toast.as_ref().expect("expected a toast");
        assert!(toast.message.contains("no workspace selected"));
        assert!(state.list_modal.is_none());
    }

    #[test]
    fn picker_commit_closes_list_modal_and_clears_state() {
        // Seed the state directly with an open GithubPicker, then commit.
        // We can't assert `open::that_detached` ran, but we *can* pin that
        // the modal closes (no lingering state) and no error toast appears
        // when the underlying call path doesn't error out synchronously.
        use crate::launch::widgets::github_picker::{GithubChoice, GithubPickerState};
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        // Use an unreachable file:// URL so `open::that_detached` is a
        // cheap no-op on most platforms (still spawns the browser handler
        // but doesn't block on network).
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "file:///dev/null".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        assert!(
            state.list_modal.is_none(),
            "picker Enter must close the modal"
        );
    }

    #[test]
    fn picker_esc_closes_without_opening_url() {
        use crate::launch::widgets::github_picker::{GithubChoice, GithubPickerState};
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "https://github.com/owner/repo/tree/main".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Esc),
        )
        .unwrap();

        assert!(state.list_modal.is_none());
        assert!(
            state.toast.is_none(),
            "Esc must not toast: {:?}",
            state.toast
        );
    }

    // ── Agents tab: D-key default binding ──────────────────────────────
    //
    // Operators set the default agent for a workspace with `D` / `d` on
    // the Agents tab. The previous `*` binding (shift+8) was dropped in
    // favour of a single canonical keystroke.

    /// Freshly-constructed `WorkspaceConfig` (no `Default` impl on the
    /// struct; see `src/workspace/mod.rs`).
    fn empty_ws() -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: Vec::new(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    /// Build an `AppConfig` whose `agents` map has the given names, plus
    /// a single empty workspace so tests can construct an editor.
    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.agents.insert(
                (*name).into(),
                crate::config::AgentSource {
                    git: format!("https://example.test/{name}.git"),
                    ..Default::default()
                },
            );
        }
        config.workspaces.insert("ws".into(), empty_ws());
        config
    }

    /// Build a `ManagerState` sitting in an editor over a workspace with
    /// the Agents tab active and the cursor pointed at `row`.
    fn editor_on_agents_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
        let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Agents;
        editor.active_field = FieldFocus::Row(row);
        state.stage = ManagerStage::Editor(editor);
        state
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

    #[test]
    fn d_key_sets_default_agent_on_current_row() {
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        // Cursor on row 1 (agent "beta"), no default set yet. The
        // workspace starts in the "all agents allowed" shorthand (empty
        // `allowed_agents`), so picking a default must NOT collapse the
        // shorthand into a single-agent allow list — see finding #1.
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(
            e.pending.default_agent.as_deref(),
            Some("beta"),
            "D on row 1 should pin agent `beta` as default",
        );
        assert!(
            e.pending.allowed_agents.is_empty(),
            "default-agent pick must preserve the all-agents shorthand \
             (empty allowed_agents); got {:?}",
            e.pending.allowed_agents,
        );
    }

    #[test]
    fn d_key_preserves_all_agents_shorthand() {
        // Explicit guard on the shorthand-preservation behavior: setting
        // a default on a workspace in "all agents" mode must leave the
        // allow list empty, not switch it to a one-agent custom list.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 2);
        {
            let ManagerStage::Editor(e) = &state.stage else {
                panic!("editor stage expected");
            };
            assert!(
                e.pending.allowed_agents.is_empty(),
                "precondition: workspace should start in all-agents mode",
            );
        }

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.allowed_agents.is_empty(),
            "all-agents shorthand must survive D; got {:?}",
            e.pending.allowed_agents,
        );
        assert_eq!(e.pending.default_agent.as_deref(), Some("gamma"));
    }

    #[test]
    fn d_key_appends_to_custom_allow_list_when_missing() {
        // Complementary case: when the workspace is already in "custom"
        // mode (non-empty allow list) and the chosen default is NOT in
        // the list, pressing D must append it — otherwise the config
        // would reference a forbidden default.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
        // Cursor on row 1 (agent "beta"), which is NOT in the allow list.
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char('D')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.pending.default_agent.as_deref(), Some("beta"));
        assert_eq!(
            e.pending.allowed_agents,
            vec!["alpha".to_string(), "beta".to_string()],
            "custom allow list must pick up the new default when missing",
        );
    }

    #[test]
    fn lowercase_d_key_sets_default_agent_on_current_row() {
        // Operators often hit `d` without holding shift; the binding
        // must accept both cases.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 0);

        press(&mut state, &mut config, KeyCode::Char('d')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert_eq!(e.pending.default_agent.as_deref(), Some("alpha"));
    }

    #[test]
    fn star_key_no_longer_sets_default_agent() {
        // Regression guard: the legacy `*` binding was removed in favour
        // of `D`. Pressing `*` on an agent row must now be a no-op.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char('*')).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.pending.default_agent.is_none(),
            "`*` must no longer set the default agent",
        );
    }

    // ── Agents tab: Space toggle matches effective allow-state ────────
    //
    // An empty `allowed_agents` list is the "all allowed" shorthand. The
    // UI renders `[x]` on every row in that mode, so toggling must
    // preserve the operator's mental model: Space on an `[x]` row flips
    // it to `[ ]`, Space on a `[ ]` row flips it to `[x]`, and the list
    // collapses to the empty shorthand whenever the resulting set
    // covers every agent in `config.agents`.

    fn pending_allowed(state: &ManagerState<'_>) -> Vec<String> {
        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        e.pending.allowed_agents.clone()
    }

    #[test]
    fn toggle_in_all_mode_demotes_to_custom_without_this_agent() {
        // Starting state: "all" mode (empty list), three agents. Pressing
        // Space on row 1 (`beta`) must produce a custom list containing
        // every other agent — i.e. `[alpha, gamma]` — so that `beta`
        // flips from `[x]` to `[ ]` and the status line reads
        // `custom (2 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut state = editor_on_agents_tab(empty_ws(), 1);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let list = pending_allowed(&state);
        assert_eq!(
            list,
            vec!["alpha".to_string(), "gamma".to_string()],
            "list must be populated with every other agent when demoting from 'all'"
        );
    }

    #[test]
    fn toggle_custom_last_item_clears_to_empty() {
        // Starting state: "custom" mode with a single allowed agent.
        // Toggling that agent off must leave the list empty (reverting
        // to the "all" shorthand) — NOT pinning it at a phantom
        // `custom (0 of N allowed)` state.
        let mut config = config_with_agents(&["alpha", "beta"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
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
        // agent (`gamma` is missing), the list must stay non-empty.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into()];
        let mut state = editor_on_agents_tab(ws, 1);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        let mut list = pending_allowed(&state);
        list.sort();
        assert_eq!(
            list,
            vec!["alpha".to_string(), "beta".to_string()],
            "adding `beta` with `gamma` still missing must produce a 2-of-3 custom list",
        );
    }

    #[test]
    fn toggle_refills_custom_to_all_when_last_agent_added_makes_it_complete() {
        // Starting state: "custom" mode with all-but-one agent present.
        // Adding the missing one would yield `custom (N of N allowed)` —
        // semantically identical to "all allowed". The toggle must
        // collapse back to the empty-list shorthand so the status badge
        // reads `all`, not `custom (3 of 3 allowed)`.
        let mut config = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = empty_ws();
        ws.allowed_agents = vec!["alpha".into(), "beta".into()];
        // Cursor on row 2 (agent `gamma`, the missing one).
        let mut state = editor_on_agents_tab(ws, 2);

        press(&mut state, &mut config, KeyCode::Char(' ')).unwrap();

        assert_eq!(
            pending_allowed(&state),
            Vec::<String>::new(),
            "filling the custom list must collapse it to empty (= all allowed)",
        );
    }

    // ── Mounts tab: R toggles readonly (rw ↔ ro) ──────────────────────

    /// Build an editor sitting on the Mounts tab over `ws` with the cursor
    /// pointed at `row`. Mirrors `editor_on_agents_tab` for Agents-tab tests.
    fn editor_on_mounts_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
        let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(row);
        state.stage = ManagerStage::Editor(editor);
        state
    }

    fn ws_with_one_mount(readonly: bool) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![MountConfig {
                src: "/host/a".into(),
                dst: "/host/a".into(),
                readonly,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

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

    #[test]
    fn toggle_rw_to_ro_reflects_in_render() {
        // After pressing R, render the Mounts tab and check the visible
        // `mode` column displays `ro`. Guards against a future regression
        // where the flip only updates state but the render helper ignores
        // the new value.
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut config = AppConfig::default();
        let mut state = editor_on_mounts_tab(ws_with_one_mount(false), 0);

        press(&mut state, &mut config, KeyCode::Char('R')).unwrap();

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("editor stage expected");
        };
        let backend = TestBackend::new(80, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            crate::launch::manager::render::render_editor(f, editor, &config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut found = false;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if row.contains(" ro ") || row.trim_end().ends_with(" ro") || row.contains(" ro  ") {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "post-toggle render must show `ro` in the mode column"
        );
    }
}
