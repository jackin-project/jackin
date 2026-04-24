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
    /// Launch the named workspace — resolved by name in `run_console`.
    LaunchNamed(String),
    /// Launch against the synthetic "Current directory" choice (row 0).
    /// `run_console` routes this through the same agent-picker path as
    /// `LaunchNamed`, using `ConsoleState::workspaces[0]` which is built
    /// in `ConsoleState::new` from the current cwd.
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

/// Cross-submodule helpers for the input/* test modules. Lifted out of
/// the per-submodule test blocks because `key()` and `mount()` show up in
/// virtually every test file; keeping a single canonical definition
/// avoids the previous problem where each submodule grew its own
/// near-identical copy.
#[cfg(test)]
pub(super) mod test_support {
    use crate::workspace::MountConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    pub(crate) fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    pub(crate) fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Cross-flow tests that genuinely span multiple stages. Stage-local
    //! tests live in the matching `input/<stage>.rs` test module:
    //! `input/list.rs`, `input/editor.rs`, `input/save.rs`,
    //! `input/prelude.rs`, `input/mouse.rs`.
    //!
    //! Anything kept here must drive a transition that crosses two stage
    //! handlers in a single test (e.g. open the in-editor rename modal,
    //! commit it via `handle_key`, then drive the save flow through the
    //! same `handle_key`).
    use super::super::state::{EditorState, FieldFocus, ManagerStage, ManagerState};
    use super::test_support::{key, mount};
    use super::*;
    use crate::config::AppConfig;
    use crate::paths::JackinPaths;
    use crossterm::event::KeyCode;

    /// End-to-end: start Create, rename via Enter-on-row-0, commit the
    /// save, and verify the workspace on disk has the updated name.
    /// Spans editor (rename modal) and save (commit) — a true cross-flow
    /// test that doesn't fit cleanly inside either submodule.
    #[test]
    fn create_mode_save_uses_updated_pending_name() {
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
        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('s')),
        )
        .unwrap();
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
}
