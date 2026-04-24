//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

use crossterm::event::{KeyCode, KeyEvent};

use super::super::widgets::{
    ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
    workdir_pick::WorkdirPickState,
};
use super::state::{
    ConfirmTarget, EditorMode, EditorState, EditorTab, ExitIntent, FieldFocus, FileBrowserTarget,
    ManagerStage, ManagerState, Modal, Toast, ToastKind,
};
use crate::config::AppConfig;
use crate::paths::JackinPaths;

pub enum InputOutcome {
    /// Stay in the manager.
    Continue,
    /// Exit jackin entirely (Esc/q from the manager list).
    ExitJackin,
    /// Launch the named workspace — resolved by name in `run_launch`.
    LaunchNamed(String),
}

#[allow(clippy::too_many_lines)]
pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Modal precedence: if a modal is open, it gets the event.
    // Use a discriminant check so we can take &mut without keeping an
    // immutable borrow alive across the call.
    if let ManagerStage::Editor(editor) = &mut state.stage
        && editor.modal.is_some()
    {
        handle_editor_modal(editor, key);
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
                    save_editor(state, config, paths)?;
                    // If save succeeded (no error_banner), exit to list.
                    if let ManagerStage::Editor(e) = &state.stage {
                        if e.error_banner.is_none() {
                            *state = ManagerState::from_config(config);
                        } else {
                            // Save failed — clear exit intent so user can retry.
                            if let ManagerStage::Editor(e) = &mut state.stage {
                                e.exit_after_save = None;
                            }
                        }
                    }
                }
                ExitIntent::RetrySave => {
                    // Collapse-confirm flow: operator approved, re-enter the
                    // save path with `collapse_approved` set so the plan
                    // commits. Stay in the editor on success — this is a
                    // regular save, not an exit.
                    if let ManagerStage::Editor(e) = &mut state.stage {
                        e.exit_after_save = None;
                    }
                    save_editor(state, config, paths)?;
                }
                ExitIntent::Discard => {
                    *state = ManagerState::from_config(config);
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
                handle_prelude_modal(p, key);
            }
            // After the modal handler runs, the prelude is in one of three states:
            // - still in a modal (user pressed a non-commit/cancel key): continue
            // - modal cleared + pending_name set: wizard complete → transition to Editor
            // - modal cleared + pending_name unset: wizard cancelled → back to List
            #[allow(clippy::items_after_statements)]
            enum PreludeStatus {
                InProgress,
                Complete,
                Cancelled,
            }
            let status = if let ManagerStage::CreatePrelude(p) = &state.stage {
                if p.modal.is_some() {
                    PreludeStatus::InProgress
                } else if p.pending_name.is_some() {
                    PreludeStatus::Complete
                } else {
                    PreludeStatus::Cancelled
                }
            } else {
                PreludeStatus::InProgress
            };
            match status {
                PreludeStatus::Complete => {
                    if let ManagerStage::CreatePrelude(p) = &state.stage {
                        let ws = p.build_workspace().expect("prelude complete");
                        let name = p.pending_name.clone().unwrap();
                        let mut editor = EditorState::new_create();
                        editor.pending = ws;
                        editor.pending_name = Some(name);
                        state.stage = ManagerStage::Editor(editor);
                    }
                }
                PreludeStatus::Cancelled => {
                    *state = ManagerState::from_config(config);
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
        StageDis::List => handle_list_key(state, config, paths, key),
        StageDis::Editor => handle_editor_key(state, config, paths, key),
        StageDis::CreatePrelude => Ok(handle_prelude_key(state, config, paths, key)),
        StageDis::ConfirmDelete => handle_confirm_delete_key(state, config, paths, key),
    }
}

fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Ok(InputOutcome::ExitJackin),
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // +1 because the "+ New workspace" row is also selectable.
            let max = state.workspaces.len();
            state.selected = (state.selected + 1).min(max);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => {
            if state.selected == state.workspaces.len() {
                // [+ New workspace] sentinel: start the create prelude
                // with a FileBrowser modal open.
                let mut prelude = super::state::CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                state.stage = ManagerStage::CreatePrelude(prelude);
                Ok(InputOutcome::Continue)
            } else if let Some(summary) = state.workspaces.get(state.selected) {
                // Launch the selected workspace — hand control back to run_launch.
                Ok(InputOutcome::LaunchNamed(summary.name.clone()))
            } else {
                Ok(InputOutcome::Continue)
            }
        }
        KeyCode::Char('e') => {
            if let Some(summary) = state.workspaces.get(state.selected) {
                // Open the editor for the selected workspace.
                let name = summary.name.clone();
                if let Some(ws) = config.workspaces.get(&name) {
                    state.stage = ManagerStage::Editor(EditorState::new_edit(name, ws.clone()));
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n') => {
            let mut prelude = super::state::CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
            state.stage = ManagerStage::CreatePrelude(prelude);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d') => {
            if let Some(ws) = state.workspaces.get(state.selected) {
                let name = ws.name.clone();
                state.stage = ManagerStage::ConfirmDelete {
                    name: name.clone(),
                    state: ConfirmState::new(format!("Delete \"{name}\"?")),
                };
            }
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Handle s and Esc outside the editor borrow to avoid re-borrow
    // conflicts (both need to call back into state or config).
    match key.code {
        KeyCode::Char('s') => {
            if matches!(&state.stage, ManagerStage::Editor(_)) {
                save_editor(state, config, paths)?;
            }
            return Ok(InputOutcome::Continue);
        }
        KeyCode::Esc => {
            // Re-borrow pattern — immutable read first, then mutate.
            if let ManagerStage::Editor(editor) = &state.stage {
                let dirty = editor.is_dirty();
                if dirty {
                    if let ManagerStage::Editor(editor) = &mut state.stage {
                        editor.modal = Some(Modal::SaveDiscardCancel {
                            state: crate::launch::widgets::save_discard::SaveDiscardState::new(
                                "Save changes before leaving?",
                            ),
                        });
                    }
                } else {
                    *state = ManagerState::from_config(config);
                }
            }
            return Ok(InputOutcome::Continue);
        }
        _ => {}
    }

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(InputOutcome::Continue);
    };

    match key.code {
        KeyCode::Tab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Agents,
                EditorTab::Agents => EditorTab::Secrets,
                EditorTab::Secrets => EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::BackTab => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Agents => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Agents,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let FieldFocus::Row(n) = editor.active_field;
            editor.active_field = FieldFocus::Row(n.saturating_sub(1));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let FieldFocus::Row(n) = editor.active_field;
            let max = max_row_for_tab(editor, config);
            editor.active_field = FieldFocus::Row((n + 1).min(max));
        }
        KeyCode::Enter => {
            match editor.active_tab {
                EditorTab::General => open_editor_field_modal(editor),
                EditorTab::Mounts => {
                    // Enter on the "+ Add mount" sentinel row triggers add flow.
                    let FieldFocus::Row(n) = editor.active_field;
                    if n == editor.pending.mounts.len() {
                        editor.modal = Some(Modal::FileBrowser {
                            target: FileBrowserTarget::EditAddMountSrc,
                            state: FileBrowserState::new_from_home()?,
                        });
                    }
                    // Enter on an existing mount row: no-op for now.
                }
                _ => {}
            }
        }
        KeyCode::Char(' ') if editor.active_tab == EditorTab::Agents => {
            toggle_agent_allowed_at_cursor(editor, config);
        }
        KeyCode::Char('*') if editor.active_tab == EditorTab::Agents => {
            set_default_agent_at_cursor(editor, config);
        }
        KeyCode::Char('a') if editor.active_tab == EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d') if editor.active_tab == EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

/// Returns the highest valid `FieldFocus::Row` index for the current tab.
fn max_row_for_tab(editor: &EditorState<'_>, config: &AppConfig) -> usize {
    match editor.active_tab {
        EditorTab::General => match editor.mode {
            // Edit: name (0), workdir (1), default_agent (2), last_used (3)
            EditorMode::Edit { .. } => 3,
            // Create: name read-only (0), workdir (1)
            EditorMode::Create => 1,
        },
        EditorTab::Mounts => editor.pending.mounts.len(), // mounts fill 0..N-1, sentinel at N
        EditorTab::Agents => config.agents.len().saturating_sub(1), // 0-based into agents
        EditorTab::Secrets => 0,
    }
}

fn open_editor_field_modal(editor: &mut EditorState<'_>) {
    use super::super::widgets::text_input::TextInputState;
    if editor.active_tab == EditorTab::General {
        let FieldFocus::Row(n) = editor.active_field;
        match n {
            0 => {
                // Name — Edit mode only (Create mode name comes from prelude).
                if let EditorMode::Edit { name } = &editor.mode {
                    let current = editor.pending_name.clone().unwrap_or_else(|| name.clone());
                    editor.modal = Some(Modal::TextInput {
                        target: super::state::TextInputTarget::Name,
                        state: TextInputState::new("Rename workspace", current),
                    });
                }
            }
            1 if !editor.pending.mounts.is_empty() => {
                // workdir — use WorkdirPick if mounts exist
                editor.modal = Some(Modal::WorkdirPick {
                    state: WorkdirPickState::from_mounts(&editor.pending.mounts),
                });
            }
            _ => {}
        }
    }
}

fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    // n is 0-based into config.agents (no header offset).
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    if let Some(agent) = agent_names.get(n) {
        if let Some(pos) = editor
            .pending
            .allowed_agents
            .iter()
            .position(|a| a == agent)
        {
            editor.pending.allowed_agents.remove(pos);
            if editor.pending.default_agent.as_deref() == Some(agent) {
                editor.pending.default_agent = None;
            }
        } else {
            editor.pending.allowed_agents.push(agent.clone());
        }
    }
}

fn set_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let agent_names: Vec<String> = config.agents.keys().cloned().collect();
    if let Some(agent) = agent_names.get(n) {
        // Setting a default also implies allowing that agent.
        if !editor.pending.allowed_agents.contains(agent) {
            editor.pending.allowed_agents.push(agent.clone());
        }
        editor.pending.default_agent = Some(agent.clone());
    }
}

fn remove_mount_at_cursor(editor: &mut EditorState<'_>) {
    let FieldFocus::Row(n) = editor.active_field;
    if n < editor.pending.mounts.len() {
        editor.pending.mounts.remove(n);
    }
}

#[allow(clippy::too_many_lines)]
fn save_editor(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };

    // Capture and clear the operator's prior approval for this save cycle.
    // Cleared up front so any return path leaves the editor in a known state;
    // the save will re-request approval on the next `s` press.
    let collapse_approved = editor.collapse_approved;
    editor.collapse_approved = false;

    // Discriminate first so we can mutate editor fields inside each arm
    // without a live borrow on editor.mode.
    #[allow(clippy::items_after_statements)]
    enum SaveMode {
        Edit { original_name: String },
        Create,
    }
    let save_mode = match &editor.mode {
        EditorMode::Edit { name } => SaveMode::Edit {
            original_name: name.clone(),
        },
        EditorMode::Create => SaveMode::Create,
    };

    // --- Collapse planning (must precede any write) ---
    //
    // Both `plan_edit` and `plan_create` are pure; they tell us whether this
    // save will silently subsume any mounts. We classify the result into:
    // - Ok, no collapse: proceed.
    // - CollapseError: show banner, abort.
    // - Pre-existing collapses only: show banner mentioning `workspace prune`.
    // - Edit-driven collapses: open Confirm modal; rerun save on approval.
    match &save_mode {
        SaveMode::Edit { original_name } => {
            let Some(current_ws) = config.workspaces.get(original_name).cloned() else {
                editor.error_banner = Some(format!(
                    "workspace {original_name:?} no longer exists in config"
                ));
                return Ok(());
            };
            let edit_delta = build_workspace_edit(&editor.original, &editor.pending);
            match crate::workspace::planner::plan_edit(
                &current_ws,
                &edit_delta.upsert_mounts,
                &edit_delta.remove_destinations,
                false,
            ) {
                Err(e) => {
                    editor.error_banner = Some(e.to_string());
                    return Ok(());
                }
                Ok(plan) => {
                    if plan.edit_driven_collapses.is_empty()
                        && !plan.pre_existing_collapses.is_empty()
                    {
                        let details: Vec<String> = plan
                            .pre_existing_collapses
                            .iter()
                            .map(|r| {
                                format!(
                                    "{} covered by {}",
                                    crate::tui::shorten_home(&r.child.src),
                                    crate::tui::shorten_home(&r.covered_by.src),
                                )
                            })
                            .collect();
                        editor.error_banner = Some(format!(
                            "pre-existing redundant mount(s) in this workspace: {}; \
                             run `jackin workspace prune {original_name}` to clean up",
                            details.join(", "),
                        ));
                        return Ok(());
                    }
                    if !plan.edit_driven_collapses.is_empty() && !collapse_approved {
                        editor.modal = Some(Modal::Confirm {
                            target: ConfirmTarget::SaveCollapse,
                            state: ConfirmState::new(collapse_confirm_prompt(
                                &plan.edit_driven_collapses,
                            )),
                        });
                        return Ok(());
                    }
                }
            }
        }
        SaveMode::Create => {
            let Some(name) = editor.pending_name.clone() else {
                editor.error_banner = Some("missing workspace name".into());
                return Ok(());
            };
            match crate::workspace::planner::plan_create(
                &editor.pending.workdir,
                editor.pending.mounts.clone(),
                false,
            ) {
                Err(e) => {
                    editor.error_banner = Some(e.to_string());
                    return Ok(());
                }
                Ok(plan) => {
                    if !plan.collapsed.is_empty() && !collapse_approved {
                        editor.modal = Some(Modal::Confirm {
                            target: ConfirmTarget::SaveCollapse,
                            state: ConfirmState::new(collapse_confirm_prompt(&plan.collapsed)),
                        });
                        return Ok(());
                    }
                    // Stash the collapsed mount set on pending so the actual
                    // write below persists the collapsed form.
                    editor.pending.mounts = plan.final_mounts;
                    // Keep pending_name consistent for the later save.
                    let _ = name;
                }
            }
        }
    }

    let mut ce = crate::config::ConfigEditor::open(paths)?;

    match save_mode {
        SaveMode::Edit { original_name } => {
            let mut current_name = original_name.clone();

            // If the user renamed, perform the rename before the field edit.
            let pending_name = editor.pending_name.clone();
            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                if let Err(e) = ce.rename_workspace(&original_name, &new_name) {
                    editor.error_banner = Some(e.to_string());
                    return Ok(());
                }
                current_name.clone_from(&new_name);
                // Reflect the rename in the editor's mode so subsequent saves
                // target the new name.
                editor.mode = EditorMode::Edit { name: new_name };
            }

            // Recompute plan_edit against the (possibly-renamed) current
            // config to pick up effective_removals — this folds collapsed
            // children into the remove list so AppConfig::edit_workspace's
            // internal rule-C check passes.
            let current_ws_for_removals = config
                .workspaces
                .get(&original_name)
                .cloned()
                .expect("current_ws existed above; planner pass can't have deleted it");
            let mut edit = build_workspace_edit(&editor.original, &editor.pending);
            match crate::workspace::planner::plan_edit(
                &current_ws_for_removals,
                &edit.upsert_mounts,
                &edit.remove_destinations,
                false,
            ) {
                Ok(plan) => {
                    edit.remove_destinations = plan.effective_removals;
                }
                Err(e) => {
                    editor.error_banner = Some(e.to_string());
                    return Ok(());
                }
            }

            if let Err(e) = ce.edit_workspace(&current_name, edit) {
                editor.error_banner = Some(e.to_string());
                return Ok(());
            }
        }
        SaveMode::Create => {
            let Some(name) = editor.pending_name.clone() else {
                editor.error_banner = Some("missing workspace name".into());
                return Ok(());
            };
            if let Err(e) = ce.create_workspace(&name, editor.pending.clone()) {
                editor.error_banner = Some(e.to_string());
                return Ok(());
            }
        }
    }
    match ce.save() {
        Ok(fresh) => {
            *config = fresh;
            // Refresh editor original/pending from the new config.
            if let ManagerStage::Editor(editor) = &mut state.stage {
                let change_count = editor.change_count();
                match &editor.mode {
                    EditorMode::Edit { name } => {
                        if let Some(ws) = config.workspaces.get(name) {
                            editor.original = ws.clone();
                            editor.pending = ws.clone();
                        }
                    }
                    EditorMode::Create => {
                        // After create, jump back to manager list with toast.
                    }
                }
                editor.error_banner = None;
                state.toast = Some(Toast {
                    message: format!("saved · {change_count} changes written"),
                    kind: ToastKind::Success,
                    shown_at: std::time::Instant::now(),
                });
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                editor.error_banner = Some(e.to_string());
            }
        }
    }
    Ok(())
}

/// Build the Confirm-modal prompt for a mount-collapse plan. Mirrors the
/// CLI wording in `src/app/mod.rs` so operators who move between the CLI
/// and the TUI see familiar text.
fn collapse_confirm_prompt(collapses: &[crate::workspace::Removal]) -> String {
    use std::fmt::Write as _;
    let mut prompt = format!(
        "Adding mount(s) will subsume {} existing mount(s):",
        collapses.len()
    );
    for r in collapses {
        let child = crate::tui::shorten_home(&r.child.src);
        let parent = crate::tui::shorten_home(&r.covered_by.src);
        // write! into a String cannot fail.
        write!(prompt, "\n  {child} covered by {parent}").ok();
    }
    prompt.push_str("\nProceed?");
    prompt
}

fn build_workspace_edit(
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
) -> crate::workspace::WorkspaceEdit {
    let mut edit = crate::workspace::WorkspaceEdit::default();
    if pending.workdir != original.workdir {
        edit.workdir = Some(pending.workdir.clone());
    }
    for m in &pending.mounts {
        if !original.mounts.iter().any(|o| o == m) {
            edit.upsert_mounts.push(m.clone());
        }
    }
    for o in &original.mounts {
        if !pending.mounts.iter().any(|p| p.dst == o.dst) {
            edit.remove_destinations.push(o.dst.clone());
        }
    }
    for a in &pending.allowed_agents {
        if !original.allowed_agents.contains(a) {
            edit.allowed_agents_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_agents {
        if !pending.allowed_agents.contains(a) {
            edit.allowed_agents_to_remove.push(a.clone());
        }
    }
    if pending.default_agent != original.default_agent {
        edit.default_agent = Some(pending.default_agent.clone());
    }
    edit
}

fn handle_prelude_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    key: KeyEvent,
) -> InputOutcome {
    if key.code == KeyCode::Esc {
        *state = ManagerState::from_config(config);
    }
    InputOutcome::Continue
}

fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
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
            *state = ManagerState::from_config(config);
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

fn handle_editor_modal(editor: &mut EditorState<'_>, key: KeyEvent) {
    let Some(modal) = editor.modal.as_mut() else {
        return;
    };
    match modal {
        Modal::TextInput { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => {
                let target = *target;
                editor.modal = None;
                apply_text_input_to_pending(target, editor, &value);
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::FileBrowser { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(path) => {
                let target = *target;
                editor.modal = None;
                apply_file_browser_to_editor(target, editor, path);
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::WorkdirPick { state } => match state.handle_key(key) {
            ModalOutcome::Commit(workdir) => {
                editor.pending.workdir = workdir;
                editor.modal = None;
            }
            ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::Confirm { target, state } => {
            let target = *target;
            match state.handle_key(key) {
                ModalOutcome::Commit(confirmed) => {
                    editor.modal = None;
                    if target == ConfirmTarget::SaveCollapse && confirmed {
                        // Operator approved the collapse plan — re-enter the
                        // save path with `collapse_approved` set so the
                        // planner's pre-check passes and the write proceeds.
                        editor.collapse_approved = true;
                        editor.exit_after_save = Some(ExitIntent::RetrySave);
                    }
                    // confirmed==false OR non-SaveCollapse target: just close
                    // the modal and return to the editor unchanged.
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::SaveDiscardCancel { state: modal_state } => {
            use crate::launch::widgets::save_discard::SaveDiscardChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveDiscardChoice::Save) => {
                    editor.modal = None;
                    editor.exit_after_save = Some(ExitIntent::Save);
                }
                ModalOutcome::Commit(SaveDiscardChoice::Discard) => {
                    editor.modal = None;
                    editor.exit_after_save = Some(ExitIntent::Discard);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
    }
}

fn apply_text_input_to_pending(
    target: super::state::TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
) {
    use super::state::TextInputTarget;
    match target {
        TextInputTarget::Name => {
            // Both Edit and Create modes stash the pending name on the editor.
            // Save-time plumbing distinguishes: Edit calls rename_workspace,
            // Create passes it to create_workspace.
            editor.pending_name = Some(value.to_string());
        }
        TextInputTarget::Workdir => editor.pending.workdir = value.to_string(),
        TextInputTarget::MountDst => {
            // Completing the add-mount flow: we stashed the src when the
            // FileBrowser committed; see apply_file_browser_to_editor.
            // At this point editor.pending.mounts already has a provisional
            // entry with src == dst == default_path. Update its dst.
            if let Some(last) = editor.pending.mounts.last_mut() {
                last.dst = value.to_string();
            }
        }
    }
}

fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    use super::super::widgets::text_input::TextInputState;
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Provisional mount with dst defaulting to same as src.
            editor.pending.mounts.push(crate::workspace::MountConfig {
                src: path.display().to_string(),
                dst: path.display().to_string(),
                readonly: false,
            });
            // Open a TextInput for dst refinement (pre-filled with path).
            editor.modal = Some(Modal::TextInput {
                target: super::state::TextInputTarget::MountDst,
                state: TextInputState::new(
                    "destination (default: same as host path)",
                    path.display().to_string(),
                ),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — Task 17 handles this.
            let _ = (editor, path);
        }
    }
}

#[allow(clippy::too_many_lines)]
fn handle_prelude_modal(prelude: &mut super::state::CreatePreludeState<'_>, key: KeyEvent) {
    use super::super::widgets::text_input::TextInputState;
    use super::state::{FileBrowserTarget, TextInputTarget};

    // Determine which step we're on by inspecting the modal discriminant,
    // then dispatch. We do this with a discriminant enum so we can end the
    // immutable/mutable borrow on `prelude.modal` before mutating other
    // fields on `prelude` (Rust borrow rules).
    enum PreludeModalDis {
        FileBrowserSrc,
        TextInputDst,
        WorkdirPick,
        TextInputName,
        Other,
    }
    let dis = match &prelude.modal {
        Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            ..
        }) => PreludeModalDis::FileBrowserSrc,
        Some(Modal::TextInput {
            target: TextInputTarget::MountDst,
            ..
        }) => PreludeModalDis::TextInputDst,
        Some(Modal::WorkdirPick { .. }) => PreludeModalDis::WorkdirPick,
        Some(Modal::TextInput {
            target: TextInputTarget::Name,
            ..
        }) => PreludeModalDis::TextInputName,
        _ => PreludeModalDis::Other,
    };

    match dis {
        PreludeModalDis::FileBrowserSrc => {
            let outcome = if let Some(Modal::FileBrowser { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(path) => {
                    prelude.modal = None;
                    prelude.accept_mount_src(path);
                    // Push dst TextInput pre-filled with host path (host-path-mirror rule).
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: TextInputState::new(
                            "destination (default: same as host path)",
                            default_dst,
                        ),
                    });
                }
                ModalOutcome::Cancel => {
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::TextInputDst => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(dst) => {
                    prelude.modal = None;
                    // readonly defaults to false (toggle for readonly is
                    // future work — spec allows this simplification).
                    prelude.accept_mount_dst(dst, false);
                    // Push WorkdirPick with the staged mount.
                    let mount = crate::workspace::MountConfig {
                        src: prelude
                            .pending_mount_src
                            .as_ref()
                            .unwrap()
                            .display()
                            .to_string(),
                        dst: prelude.pending_mount_dst.clone().unwrap(),
                        readonly: prelude.pending_readonly,
                    };
                    prelude.modal = Some(Modal::WorkdirPick {
                        state: WorkdirPickState::from_mounts(&[mount]),
                    });
                }
                ModalOutcome::Cancel => {
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::WorkdirPick => {
            let outcome = if let Some(Modal::WorkdirPick { state }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(workdir) => {
                    prelude.modal = None;
                    prelude.accept_workdir(workdir);
                    let default_name = prelude.default_name().unwrap_or_default();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::Name,
                        state: TextInputState::new("Name this workspace", default_name),
                    });
                }
                ModalOutcome::Cancel => {
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::TextInputName => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(name) => {
                    prelude.modal = None;
                    prelude.accept_name(name);
                    // Prelude complete — the outer handle_key dispatcher
                    // checks for this and transitions to Editor(Create).
                }
                ModalOutcome::Cancel => {
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::Other => {}
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
    use super::*;
    use crate::config::AppConfig;
    use crate::launch::manager::state::ManagerState;
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

    #[test]
    fn save_editor_opens_confirm_on_edit_driven_collapse() {
        // Existing workspace with /work/sub; operator adds /work which
        // subsumes the child. Expected: Confirm modal opens, no write yet.
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

        let mut state = ManagerState::from_config(&config);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        // Add the /work parent to pending mounts — this is the edit-driven
        // case.
        editor.pending.mounts.insert(0, mount("/work", "/work"));
        state.stage = ManagerStage::Editor(editor);

        save_editor(&mut state, &mut config, &paths).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            matches!(
                e.modal,
                Some(Modal::Confirm {
                    target: ConfirmTarget::SaveCollapse,
                    ..
                })
            ),
            "expected SaveCollapse confirm modal; got {:?}",
            e.modal
        );
        assert!(e.error_banner.is_none(), "no error banner expected");
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
        // Same setup, then simulate Y press on the confirm modal — this
        // should approve and re-run save_editor, committing the collapsed
        // mount set.
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

        let mut state = ManagerState::from_config(&config);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        editor.pending.mounts.insert(0, mount("/work", "/work"));
        state.stage = ManagerStage::Editor(editor);

        // Step 1: first save opens the confirm modal.
        save_editor(&mut state, &mut config, &paths).unwrap();

        // Step 2: deliver Y through the modal path. We call handle_key which
        // routes through handle_editor_modal → SaveCollapse approval →
        // RetrySave intent → save_editor re-entry.
        handle_key(&mut state, &mut config, &paths, key(KeyCode::Char('y'))).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(
            e.modal.is_none(),
            "modal should be closed after approval; got {:?}",
            e.modal
        );
        assert!(
            e.error_banner.is_none(),
            "save should have succeeded: {:?}",
            e.error_banner
        );

        // On-disk config now contains only the collapsed parent.
        let reloaded = AppConfig::load_or_init(&paths).unwrap();
        let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
        assert_eq!(ws_on_disk.mounts.len(), 1);
        assert_eq!(ws_on_disk.mounts[0].dst, "/work");
    }

    #[test]
    fn cancelling_collapse_keeps_pending_mounts_intact() {
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

        let mut state = ManagerState::from_config(&config);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        editor.pending.mounts.insert(0, mount("/work", "/work"));
        state.stage = ManagerStage::Editor(editor);

        save_editor(&mut state, &mut config, &paths).unwrap();

        // Press N — cancel the collapse.
        handle_key(&mut state, &mut config, &paths, key(KeyCode::Char('n'))).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(e.modal.is_none(), "modal should close on cancel");
        assert_eq!(
            e.pending.mounts.len(),
            2,
            "pending mounts stay so operator can fix by hand"
        );
        assert!(!e.collapse_approved, "approval flag must be clear");

        // On-disk config unchanged.
        let reloaded = AppConfig::load_or_init(&paths).unwrap();
        let ws_on_disk = reloaded.workspaces.get("big-monorepo").unwrap();
        assert_eq!(ws_on_disk.mounts.len(), 1);
    }

    #[test]
    fn readonly_mismatch_produces_error_banner_no_write() {
        // Add a rw /work that would subsume an existing ro /work/sub —
        // plan_edit must reject with ReadonlyMismatch.
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

        let mut state = ManagerState::from_config(&config);
        let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
        editor.pending.mounts.insert(0, mount("/work", "/work")); // rw
        state.stage = ManagerStage::Editor(editor);

        save_editor(&mut state, &mut config, &paths).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(e.modal.is_none(), "no modal for hard errors");
        let banner = e
            .error_banner
            .as_deref()
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
        // Workspace already has overlapping mounts. Operator opens editor
        // and saves without mount changes — plan_edit reports
        // pre_existing_collapses; no write, error banner references prune.
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

        let mut state = ManagerState::from_config(&config);
        let editor = EditorState::new_edit("legacy-workspace".into(), ws);
        state.stage = ManagerStage::Editor(editor);

        save_editor(&mut state, &mut config, &paths).unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        assert!(e.modal.is_none(), "no confirm for pre-existing-only case");
        let banner = e
            .error_banner
            .as_deref()
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
}
