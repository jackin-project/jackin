//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::super::widgets::{
    ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
    workdir_pick::WorkdirPickState,
};
use super::state::{
    DragState, EditorMode, EditorState, EditorTab, ExitIntent, FieldFocus, FileBrowserTarget,
    ManagerStage, ManagerState, Modal, Toast, ToastKind, clamp_split,
};
use crate::config::AppConfig;
use crate::paths::JackinPaths;

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
        handle_list_modal(state, key);
        return Ok(InputOutcome::Continue);
    }
    // Modal precedence: if a modal is open, it gets the event.
    // Use a discriminant check so we can take &mut without keeping an
    // immutable borrow alive across the call.
    if let ManagerStage::Editor(editor) = &mut state.stage
        && editor.modal.is_some()
    {
        handle_editor_modal(editor, key);

        // Drain the ConfirmSave → commit signal FIRST. The modal handler
        // only closes the modal and stashes the plan; this outer layer
        // has `paths`/`cwd` and actually performs the write.
        let pending = if let ManagerStage::Editor(editor) = &mut state.stage {
            editor.pending_save_commit.take()
        } else {
            None
        };
        if let Some(plan) = pending {
            commit_editor_save(state, config, paths, cwd, plan)?;
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
                    // Mark the editor so that, if the operator picks Save
                    // in the confirm dialog and the write succeeds, we
                    // bounce out to the workspace list.
                    if let ManagerStage::Editor(e) = &mut state.stage {
                        e.exit_after_save = None;
                        e.exit_on_save_success = true;
                    }
                    begin_editor_save(state, config)?;
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
        StageDis::List => handle_list_key(state, config, paths, cwd, key),
        StageDis::Editor => handle_editor_key(state, config, paths, cwd, key),
        StageDis::CreatePrelude => Ok(handle_prelude_key(state, config, paths, cwd, key)),
        StageDis::ConfirmDelete => handle_confirm_delete_key(state, config, paths, cwd, key),
    }
}

fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    _cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Row layout (mirrors ManagerState::from_config):
    //   0                 → synthetic "Current directory"
    //   1..=saved_count   → saved workspaces (saved_index = selected - 1)
    //   saved_count + 1   → "+ New workspace" sentinel
    let saved_count = state.workspaces.len();
    let sentinel_idx = saved_count + 1;
    let total_rows = sentinel_idx + 1; // 0..=sentinel_idx are valid
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Ok(InputOutcome::ExitJackin),
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            state.selected = (state.selected + 1).min(total_rows - 1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => {
            if state.selected == 0 {
                // Row 0 — launch against cwd. Run-loop routes through the
                // same agent-picker stage as LaunchNamed.
                Ok(InputOutcome::LaunchCurrentDir)
            } else if state.selected == sentinel_idx {
                // [+ New workspace] sentinel: start the create prelude
                // with a FileBrowser modal open.
                let mut prelude = super::state::CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                state.stage = ManagerStage::CreatePrelude(prelude);
                Ok(InputOutcome::Continue)
            } else if let Some(summary) = state.workspaces.get(state.selected - 1) {
                // Launch the selected saved workspace.
                Ok(InputOutcome::LaunchNamed(summary.name.clone()))
            } else {
                Ok(InputOutcome::Continue)
            }
        }
        KeyCode::Char('e' | 'E') => {
            if state.selected == 0 {
                state.toast = Some(Toast {
                    message: "Current directory cannot be edited".into(),
                    kind: ToastKind::Error,
                    shown_at: std::time::Instant::now(),
                });
                return Ok(InputOutcome::Continue);
            }
            if state.selected == sentinel_idx {
                return Ok(InputOutcome::Continue);
            }
            if let Some(summary) = state.workspaces.get(state.selected - 1) {
                // Open the editor for the selected workspace.
                let name = summary.name.clone();
                if let Some(ws) = config.workspaces.get(&name) {
                    state.stage = ManagerStage::Editor(EditorState::new_edit(name, ws.clone()));
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n' | 'N') => {
            let mut prelude = super::state::CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
            state.stage = ManagerStage::CreatePrelude(prelude);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d' | 'D') => {
            if state.selected == 0 {
                state.toast = Some(Toast {
                    message: "Current directory cannot be deleted".into(),
                    kind: ToastKind::Error,
                    shown_at: std::time::Instant::now(),
                });
                return Ok(InputOutcome::Continue);
            }
            if state.selected == sentinel_idx {
                return Ok(InputOutcome::Continue);
            }
            if let Some(ws) = state.workspaces.get(state.selected - 1) {
                let name = ws.name.clone();
                state.stage = ManagerStage::ConfirmDelete {
                    name: name.clone(),
                    state: ConfirmState::new(format!("Delete \"{name}\"?")),
                };
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('o' | 'O') => {
            handle_list_open_in_github(state, config, sentinel_idx);
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

/// Dispatch the `o` key on the workspace list view. Keeps `handle_list_key`
/// below clippy's `too_many_lines` threshold and isolates the
/// toast/open/picker decision tree.
fn handle_list_open_in_github(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    sentinel_idx: usize,
) {
    if state.selected == 0 || state.selected == sentinel_idx {
        state.toast = Some(Toast {
            message: "no workspace selected".into(),
            kind: ToastKind::Error,
            shown_at: std::time::Instant::now(),
        });
        return;
    }
    let Some(summary) = state.workspaces.get(state.selected - 1) else {
        return;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return;
    };
    let choices = resolve_github_mounts_for_workspace(ws);
    match choices.len() {
        0 => {
            state.toast = Some(Toast {
                message: "no GitHub URLs for this workspace".into(),
                kind: ToastKind::Error,
                shown_at: std::time::Instant::now(),
            });
        }
        1 => {
            if let Err(e) = open::that_detached(&choices[0].url) {
                state.toast = Some(Toast {
                    message: format!("failed to open URL: {e}"),
                    kind: ToastKind::Error,
                    shown_at: std::time::Instant::now(),
                });
            }
        }
        _ => {
            state.list_modal = Some(Modal::GithubPicker {
                state: crate::launch::widgets::github_picker::GithubPickerState::new(choices),
            });
        }
    }
}

/// Inspect each mount of `ws`, keep only those whose src resolves to a
/// GitHub-hosted git working copy with a web URL, and return a picker-
/// friendly tuple `(src, branch, url)` per surviving mount. Used by the
/// list-view `o` key to decide whether to toast / open / show a picker.
pub(super) fn resolve_github_mounts_for_workspace(
    ws: &crate::workspace::WorkspaceConfig,
) -> Vec<crate::launch::widgets::github_picker::GithubChoice> {
    use super::mount_info::{GitBranch, GitHost, MountKind, inspect};
    use crate::launch::widgets::github_picker::GithubChoice;
    ws.mounts
        .iter()
        .filter_map(|m| {
            let MountKind::Git {
                branch,
                host: GitHost::Github,
                web_url: Some(url),
            } = inspect(&m.src)
            else {
                return None;
            };
            let branch_label = match branch {
                GitBranch::Named(b) => b,
                GitBranch::Detached { short_sha } => format!("detached {short_sha}"),
                GitBranch::Unknown => "unknown".to_string(),
            };
            Some(GithubChoice {
                src: m.src.clone(),
                branch: branch_label,
                url,
            })
        })
        .collect()
}

fn handle_editor_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // Handle s and Esc outside the editor borrow to avoid re-borrow
    // conflicts (both need to call back into state or config).
    match key.code {
        KeyCode::Char('s' | 'S') => {
            if let ManagerStage::Editor(editor) = &state.stage {
                // No-op when there's nothing to save — avoid putting up
                // an empty ConfirmSave dialog. `exit_on_save_success` is
                // NOT reset here: the `ExitIntent::Save` path explicitly
                // sets it to `true` before calling begin_editor_save, and
                // both paths want the flag preserved through the save cycle.
                if editor.change_count() == 0 {
                    return Ok(InputOutcome::Continue);
                }
            }
            if matches!(&state.stage, ManagerStage::Editor(_)) {
                begin_editor_save(state, config)?;
            }
            // `paths` is not needed until the operator actually commits
            // in the ConfirmSave dialog; silence the unused binding until
            // the reborrow for commit happens in handle_editor_modal.
            let _ = paths;
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
                    *state = ManagerState::from_config(config, cwd);
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
        KeyCode::Tab | KeyCode::Right => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Mounts,
                EditorTab::Mounts => EditorTab::Agents,
                EditorTab::Agents => EditorTab::Secrets,
                EditorTab::Secrets => EditorTab::General,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::BackTab | KeyCode::Left => {
            editor.active_tab = match editor.active_tab {
                EditorTab::General => EditorTab::Secrets,
                EditorTab::Mounts => EditorTab::General,
                EditorTab::Agents => EditorTab::Mounts,
                EditorTab::Secrets => EditorTab::Agents,
            };
            editor.active_field = FieldFocus::Row(0);
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            let FieldFocus::Row(n) = editor.active_field;
            editor.active_field = FieldFocus::Row(n.saturating_sub(1));
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
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
        KeyCode::Char('D' | 'd') if editor.active_tab == EditorTab::Agents => {
            set_default_agent_at_cursor(editor, config);
        }
        KeyCode::Char('a' | 'A') if editor.active_tab == EditorTab::Mounts => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
        }
        KeyCode::Char('d' | 'D') if editor.active_tab == EditorTab::Mounts => {
            remove_mount_at_cursor(editor);
        }
        KeyCode::Char('o' | 'O') if editor.active_tab == EditorTab::Mounts => {
            // Open the highlighted mount's GitHub URL in the system browser.
            // Silent no-op when the cursor is on the `+ Add mount` sentinel,
            // or when the row's MountKind doesn't expose a resolvable URL
            // (non-GitHub remotes, repos without `origin`, plain folders).
            // On non-GitHub mounts we emit a toast so the hint is discoverable.
            let FieldFocus::Row(n) = editor.active_field;
            if let Some(m) = editor.pending.mounts.get(n) {
                let kind = super::mount_info::inspect(&m.src);
                match kind {
                    super::mount_info::MountKind::Git {
                        host: super::mount_info::GitHost::Github,
                        web_url: Some(url),
                        ..
                    } => {
                        // End the editor borrow before we set `state.toast`.
                        if let Err(e) = open::that_detached(&url) {
                            state.toast = Some(Toast {
                                message: format!("failed to open URL: {e}"),
                                kind: ToastKind::Error,
                                shown_at: std::time::Instant::now(),
                            });
                        }
                    }
                    super::mount_info::MountKind::Git { .. }
                    | super::mount_info::MountKind::Folder
                    | super::mount_info::MountKind::Missing => {
                        state.toast = Some(Toast {
                            message: "no GitHub URL for this mount".into(),
                            kind: ToastKind::Error,
                            shown_at: std::time::Instant::now(),
                        });
                    }
                }
            }
            // Sentinel row (n == mounts.len()): silent no-op.
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

/// Phase 1 of the save flow: run pre-save validation, compute the
/// plan, and open a `Modal::ConfirmSave` summarising the change set.
///
/// Validation failures (missing name, planner reject, pre-existing-only
/// collapse) surface as inline `editor.error_banner` messages — NOT as
/// an `ErrorPopup`. The popup is reserved for commit-time errors (phase 2).
///
/// On success, the function stashes the planner's `effective_removals`
/// / `final_mounts` on the modal state so the commit path doesn't need
/// to re-run `plan_edit`/`plan_create`.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
fn begin_editor_save(state: &mut ManagerState<'_>, config: &mut AppConfig) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    // A stale banner from a previous cycle should clear now that the
    // operator has kicked off a fresh save attempt.
    editor.error_banner = None;

    // Classify once so mutating arms below don't keep editor.mode borrowed.
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

    let (effective_removals, final_mounts, has_collapses, collapse_lines) = match &save_mode {
        SaveMode::Edit { original_name } => {
            let Some(current_ws) = config.workspaces.get(original_name).cloned() else {
                editor.error_banner = Some(format!(
                    "workspace {original_name:?} no longer exists in config"
                ));
                editor.exit_on_save_success = false;
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
                    editor.exit_on_save_success = false;
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
                             run `jackin' workspace prune {original_name}` to clean up",
                            details.join(", "),
                        ));
                        editor.exit_on_save_success = false;
                        return Ok(());
                    }
                    let has = !plan.edit_driven_collapses.is_empty();
                    let lines = collapse_section_lines(&plan.edit_driven_collapses);
                    (plan.effective_removals, None, has, lines)
                }
            }
        }
        SaveMode::Create => {
            if editor.pending_name.is_none() {
                editor.error_banner = Some("missing workspace name".into());
                editor.exit_on_save_success = false;
                return Ok(());
            }
            match crate::workspace::planner::plan_create(
                &editor.pending.workdir,
                editor.pending.mounts.clone(),
                false,
            ) {
                Err(e) => {
                    editor.error_banner = Some(e.to_string());
                    editor.exit_on_save_success = false;
                    return Ok(());
                }
                Ok(plan) => {
                    let has = !plan.collapsed.is_empty();
                    let lines = collapse_section_lines(&plan.collapsed);
                    (Vec::new(), Some(plan.final_mounts), has, lines)
                }
            }
        }
    };

    // Build the display lines describing the plan. These pre-computed
    // lines are what the ConfirmSave widget renders; the widget itself
    // stays dumb.
    let lines = build_confirm_save_lines(editor, config, &collapse_lines);
    let mut confirm_state = crate::launch::widgets::confirm_save::ConfirmSaveState::new(lines);
    confirm_state.effective_removals = effective_removals;
    confirm_state.final_mounts = final_mounts;
    confirm_state.has_collapses = has_collapses;
    editor.modal = Some(Modal::ConfirmSave {
        state: confirm_state,
    });
    Ok(())
}

/// Phase 2 of the save flow: the operator clicked Save in the `ConfirmSave`
/// dialog. Actually write to the on-disk config via the internal
/// `ConfigEditor` API (NO CLI subprocess).
///
/// On Err, opens an `ErrorPopup` describing the failure. On Ok, refreshes
/// the editor's origin-of-truth snapshot and — if `exit_on_save_success`
/// is set — transitions the whole manager back to the list view.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
fn commit_editor_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    plan: super::state::PendingSaveCommit,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };

    // Reuse the classify-first pattern from begin_editor_save so the
    // mutating write arms don't keep editor.mode borrowed.
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

    // If plan_create stashed a collapsed mount set, honour it now — the
    // operator already saw + approved it in the confirm dialog.
    if let Some(final_mounts) = plan.final_mounts {
        editor.pending.mounts = final_mounts;
    }

    let ce_res = crate::config::ConfigEditor::open(paths);
    let mut ce = match ce_res {
        Ok(ce) => ce,
        Err(e) => {
            open_save_error_popup(editor, &e.to_string());
            editor.exit_on_save_success = false;
            return Ok(());
        }
    };

    match save_mode {
        SaveMode::Edit { original_name } => {
            let mut current_name = original_name.clone();
            let pending_name = editor.pending_name.clone();
            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                if let Err(e) = ce.rename_workspace(&original_name, &new_name) {
                    open_save_error_popup(editor, &e.to_string());
                    editor.exit_on_save_success = false;
                    return Ok(());
                }
                current_name.clone_from(&new_name);
                editor.mode = EditorMode::Edit { name: new_name };
            }

            let mut edit = build_workspace_edit(&editor.original, &editor.pending);
            edit.remove_destinations = plan.effective_removals;

            if let Err(e) = ce.edit_workspace(&current_name, edit) {
                open_save_error_popup(editor, &e.to_string());
                editor.exit_on_save_success = false;
                return Ok(());
            }
        }
        SaveMode::Create => {
            let Some(name) = editor.pending_name.clone() else {
                open_save_error_popup(editor, "missing workspace name");
                editor.exit_on_save_success = false;
                return Ok(());
            };
            if let Err(e) = ce.create_workspace(&name, editor.pending.clone()) {
                open_save_error_popup(editor, &e.to_string());
                editor.exit_on_save_success = false;
                return Ok(());
            }
        }
    }

    match ce.save() {
        Ok(fresh) => {
            *config = fresh;
            // Refresh editor origin-of-truth; keep the operator on the
            // editor (direct `s` press) OR bounce to list (Esc→Save path).
            let should_exit = if let ManagerStage::Editor(editor) = &mut state.stage {
                let change_count = editor.change_count();
                if let EditorMode::Edit { name } = &editor.mode
                    && let Some(ws) = config.workspaces.get(name)
                {
                    editor.original = ws.clone();
                    editor.pending = ws.clone();
                }
                editor.error_banner = None;
                let exit = editor.exit_on_save_success;
                editor.exit_on_save_success = false;
                state.toast = Some(Toast {
                    message: format!("saved · {change_count} changes written"),
                    kind: ToastKind::Success,
                    shown_at: std::time::Instant::now(),
                });
                exit
            } else {
                false
            };
            if should_exit
                || matches!(
                    state.stage,
                    ManagerStage::Editor(EditorState {
                        mode: EditorMode::Create,
                        ..
                    })
                )
            {
                // Create mode always exits to the list after a successful
                // write; there's no persistent "edit" view for a freshly-
                // created workspace until the operator picks it.
                *state = ManagerState::from_config(config, cwd);
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                open_save_error_popup(editor, &e.to_string());
                editor.exit_on_save_success = false;
            }
        }
    }
    Ok(())
}

fn open_save_error_popup(editor: &mut EditorState<'_>, message: &str) {
    editor.modal = Some(Modal::ErrorPopup {
        state: crate::launch::widgets::error_popup::ErrorPopupState::new(
            "Save failed",
            message.to_string(),
        ),
    });
}

/// Build the list of display lines shown inside the `ConfirmSave` modal.
/// In Create mode we show a summary; in Edit mode a structured diff
/// between `editor.original` and `editor.pending`. If the planner
/// reports mount collapses, a final "Mount collapse required:" section
/// is appended.
#[allow(clippy::too_many_lines)]
fn build_confirm_save_lines(
    editor: &EditorState<'_>,
    config: &AppConfig,
    collapse_lines: &[ratatui::text::Line<'static>],
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    let phosphor_green = Color::Rgb(0, 255, 65);
    let phosphor_dim = Color::Rgb(0, 140, 30);
    let white = Color::Rgb(255, 255, 255);
    let heading = Style::default().fg(white).add_modifier(Modifier::BOLD);
    let value = Style::default().fg(phosphor_green);
    let dim = Style::default().fg(phosphor_dim);

    let mut out: Vec<Line<'static>> = Vec::new();

    match &editor.mode {
        EditorMode::Create => {
            let name = editor
                .pending_name
                .clone()
                .unwrap_or_else(|| "(unnamed)".into());
            out.push(Line::from(vec![
                Span::styled("Create workspace: ", heading),
                Span::styled(name, value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Working directory: ", heading),
                Span::styled(crate::tui::shorten_home(&editor.pending.workdir), value),
            ]));
            if !editor.pending.mounts.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled(
                    format!("Mounts ({}):", editor.pending.mounts.len()),
                    heading,
                )));
                for m in &editor.pending.mounts {
                    out.push(Line::from(Span::styled(
                        format!("  \u{2022} {}", mount_summary(m)),
                        value,
                    )));
                }
            }
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Allowed agents: ", heading),
                Span::styled(allowed_agents_summary(editor, config), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Default agent: ", heading),
                Span::styled(
                    editor
                        .pending
                        .default_agent
                        .clone()
                        .unwrap_or_else(|| "(none)".into()),
                    value,
                ),
            ]));
        }
        EditorMode::Edit { name } => {
            let display_name = editor.pending_name.clone().unwrap_or_else(|| name.clone());
            out.push(Line::from(vec![
                Span::styled("Edit workspace: ", heading),
                Span::styled(display_name, value),
            ]));

            // Rename diff (a rename counts even though it's not a
            // workspace-field change per se).
            if let Some(new_name) = &editor.pending_name
                && new_name != name
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Rename:", heading)));
                out.push(Line::from(Span::styled(format!("  - {name}"), dim)));
                out.push(Line::from(Span::styled(format!("  + {new_name}"), value)));
            }

            if editor.pending.workdir != editor.original.workdir {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Working directory:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", crate::tui::shorten_home(&editor.original.workdir)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", crate::tui::shorten_home(&editor.pending.workdir)),
                    value,
                )));
            }

            let added_mounts: Vec<_> = editor
                .pending
                .mounts
                .iter()
                .filter(|m| !editor.original.mounts.contains(m))
                .collect();
            let removed_mounts: Vec<_> = editor
                .original
                .mounts
                .iter()
                .filter(|m| !editor.pending.mounts.contains(m))
                .collect();
            if !added_mounts.is_empty() || !removed_mounts.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Mounts:", heading)));
                for m in &added_mounts {
                    out.push(Line::from(Span::styled(
                        format!("  + {}", mount_summary(m)),
                        value,
                    )));
                }
                for m in &removed_mounts {
                    out.push(Line::from(Span::styled(
                        format!("  - {}", mount_summary(m)),
                        dim,
                    )));
                }
            }

            let added_agents: Vec<_> = editor
                .pending
                .allowed_agents
                .iter()
                .filter(|a| !editor.original.allowed_agents.contains(a))
                .collect();
            let removed_agents: Vec<_> = editor
                .original
                .allowed_agents
                .iter()
                .filter(|a| !editor.pending.allowed_agents.contains(a))
                .collect();
            if !added_agents.is_empty() || !removed_agents.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Allowed agents:", heading)));
                for a in &added_agents {
                    out.push(Line::from(Span::styled(format!("  + {a}"), value)));
                }
                for a in &removed_agents {
                    out.push(Line::from(Span::styled(format!("  - {a}"), dim)));
                }
            }

            if editor.pending.default_agent != editor.original.default_agent {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Default agent:", heading)));
                if let Some(old) = &editor.original.default_agent {
                    out.push(Line::from(Span::styled(format!("  - {old}"), dim)));
                }
                if let Some(new) = &editor.pending.default_agent {
                    out.push(Line::from(Span::styled(format!("  + {new}"), value)));
                } else {
                    out.push(Line::from(Span::styled("  + (none)", value)));
                }
            }
        }
    }

    if !collapse_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Mount collapse required:",
            heading,
        )));
        out.extend(collapse_lines.iter().cloned());
    }

    out
}

/// Summarise a mount as `<src>  (rw|ro, <label>)` where label is
/// github/git/folder/missing from `mount_info::inspect`.
fn mount_summary(m: &crate::workspace::MountConfig) -> String {
    let src = crate::tui::shorten_home(&m.src);
    let kind = super::mount_info::inspect(&m.src);
    let rw = if m.readonly { "ro" } else { "rw" };
    format!("{src}  ({rw}, {})", kind.label())
}

/// Summarise the allowed-agent selection — `any (N agents)` when the
/// workspace lets every configured agent run, otherwise a comma-separated
/// list.
fn allowed_agents_summary(editor: &EditorState<'_>, config: &AppConfig) -> String {
    if editor.pending.allowed_agents.is_empty() {
        return format!("any ({} agents)", config.agents.len());
    }
    editor.pending.allowed_agents.join(", ")
}

/// Render each mount-collapse entry as `  <child> → <parent>`, to be
/// appended to the `ConfirmSave` lines under a "Mount collapse required:"
/// heading.
fn collapse_section_lines(
    collapses: &[crate::workspace::Removal],
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    let phosphor_dim = Color::Rgb(0, 140, 30);
    let style = Style::default().fg(phosphor_dim);
    collapses
        .iter()
        .map(|r| {
            let child = crate::tui::shorten_home(&r.child.src);
            let parent = crate::tui::shorten_home(&r.covered_by.src);
            Line::from(Span::styled(
                format!("  {child} will be subsumed under {parent}"),
                style,
            ))
        })
        .collect()
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
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    if key.code == KeyCode::Esc {
        *state = ManagerState::from_config(config, cwd);
    }
    InputOutcome::Continue
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

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
/// Only `Modal::GithubPicker` is expected here today; any other variant that
/// sneaks in is treated as cancel so the operator isn't stuck.
fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(modal) = state.list_modal.as_mut() else {
        return;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                state.list_modal = None;
                if let Err(e) = open::that_detached(&url) {
                    state.toast = Some(Toast {
                        message: format!("failed to open URL: {e}"),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
            }
            ModalOutcome::Cancel => {
                state.list_modal = None;
            }
            ModalOutcome::Continue => {}
        },
        // Defensive catch-all — no other Modal variants are placed on the
        // list_modal slot today.
        _ => {
            state.list_modal = None;
        }
    }
}

/// Minimum terminal width (in columns) at which the list/details seam is
/// draggable. Below this, the 20/80 clamp bounds leave the right pane
/// implausibly narrow for meaningful interaction — silently ignore mouse
/// events rather than produce an unusable layout.
const MIN_DRAGGABLE_WIDTH: u16 = 40;
/// Half-width of the seam hit-region. A Down event lands within ±1 column
/// of the computed seam to initiate drag. Narrow enough that operators
/// don't accidentally start a drag while clicking in either pane.
const SEAM_HIT_SLACK: u16 = 1;

/// Height of the header chunk in the list-view chrome. Mirrors
/// `Constraint::Length(3)` in `render::render`. Used by mouse hit-testing
/// to convert a terminal row into a list item index.
const LIST_HEADER_HEIGHT: u16 = 3;
/// Height of the footer chunk in the list-view chrome. Mirrors
/// `Constraint::Length(2)` in `render::render`.
const LIST_FOOTER_HEIGHT: u16 = 2;

/// Dispatch a mouse event into the workspace manager's list view. Drives
/// the mouse-draggable seam between the list pane and the details pane.
///
/// Behaviour:
/// - On `ManagerStage::List` with no list-level modal open: drives the
///   list/details seam drag (anchor + drag + release) and click-to-select.
/// - On `ManagerStage::Editor` / `CreatePrelude` with a `FileBrowser` modal
///   whose git-prompt overlay is active AND has a resolved URL: a
///   `Down(Left)` on the URL row fires `open::that_detached` best-effort.
/// - Ignores everything when the terminal is narrower than
///   [`MIN_DRAGGABLE_WIDTH`] — drag bounds would be absurd.
/// - All other events are ignored.
///
/// The caller (run-loop in `src/launch/mod.rs`) is responsible for
/// passing the current `terminal.size()?` as `term_size` so the handler
/// can compute the seam column as `term_size.width * list_split_pct / 100`.
pub fn handle_mouse(state: &mut ManagerState<'_>, mouse: MouseEvent, term_size: Rect) {
    if term_size.width < MIN_DRAGGABLE_WIDTH {
        return;
    }

    // Editor / CreatePrelude file-browser URL click: only on Down(Left),
    // only when the modal is a FileBrowser with a resolved git URL.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && try_open_file_browser_git_url(state, mouse, term_size)
    {
        return;
    }

    // Stage + modal gate for the list-view seam drag. Only the List view
    // participates in drag; the Editor, CreatePrelude and ConfirmDelete
    // stages only observe the URL-click path above.
    if !matches!(state.stage, ManagerStage::List) {
        return;
    }
    if state.list_modal.is_some() {
        return;
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let seam_x = seam_column(state.list_split_pct, term_size.width);
            // Seam hit always wins — a click on the seam column starts a
            // drag, never a row select. Even if the seam happens to overlap
            // a valid row position, the resize affordance takes precedence.
            if near_seam(mouse.column, seam_x) {
                state.drag_state = Some(DragState {
                    anchor_pct: state.list_split_pct,
                    anchor_x: mouse.column,
                });
                return;
            }
            // Otherwise, treat as click-to-select if the click lands inside
            // the list pane's content area (excluding borders).
            if let Some(row) = list_content_row_index(state, mouse, term_size, seam_x) {
                state.selected = row;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(anchor) = state.drag_state {
                let new_pct = pct_from_drag(anchor, mouse.column, term_size.width);
                state.list_split_pct = clamp_split(new_pct);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.drag_state = None;
        }
        _ => {}
    }
}

/// Compute the `FileBrowser` modal's outer rect, mirroring
/// `render::render_modal`'s `centered_rect_fixed(area, 70, 22)` for
/// `Modal::FileBrowser`. Inlined here to avoid adding a cross-module pub
/// helper for one call-site; if another modal ever needs mouse hit-testing
/// we can lift this into a shared module.
fn file_browser_modal_rect(term_size: Rect) -> Rect {
    let pct_w: u16 = 70;
    let rows: u16 = 22;
    let w = term_size.width * pct_w / 100;
    let h = rows.min(term_size.height);
    Rect {
        x: term_size.x + term_size.width.saturating_sub(w) / 2,
        y: term_size.y + term_size.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// If the `Editor` or `CreatePrelude` stage has an open `FileBrowser`
/// whose git-prompt is active with a resolved URL, and the click lands
/// on the URL row, fire `open::that_detached` best-effort. Returns
/// `true` iff the click was consumed (URL opened). Non-matching stages,
/// non-click events, and clicks outside the URL row all return `false`
/// and the caller falls through to the list-view handler.
fn try_open_file_browser_git_url(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let fb_state: &FileBrowserState = match &state.stage {
        ManagerStage::Editor(editor) => match editor.modal.as_ref() {
            Some(Modal::FileBrowser { state, .. }) => state,
            _ => return false,
        },
        ManagerStage::CreatePrelude(prelude) => match prelude.modal.as_ref() {
            Some(Modal::FileBrowser { state, .. }) => state,
            _ => return false,
        },
        _ => return false,
    };
    let modal_area = file_browser_modal_rect(term_size);
    fb_state.maybe_open_url_on_click(modal_area, mouse.column, mouse.row)
}

/// Return the list-row index the mouse is over, or `None` if the click
/// falls outside the list pane's content area.
///
/// Mirrors the layout from `render::render` + `render::render_list_body`:
///   - Chrome: `[header (3 rows)][body][footer (2 rows)]`
///   - Body is horizontally split; left column hosts the workspace list.
///   - The list itself sits inside a bordered block — row 0 of list
///     items is at y = header + 1 (the +1 skips the top border).
///
/// Returns `Some(idx)` only when:
///   - `mouse.column` is inside `[1, seam_x - 1]` (left pane interior,
///     i.e. excluding both the left border and the seam column itself)
///   - `mouse.row` is inside `[header + 1, body_end - 1]` (body interior,
///     excluding the top and bottom border rows)
///   - The computed index is within `[0, sentinel_idx]` (the valid range
///     of rows the operator can select)
fn list_content_row_index(
    state: &ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    seam_x: u16,
) -> Option<usize> {
    // Column check — strictly inside the left pane (exclude left border
    // and seam column, which is also the left pane's right border).
    if mouse.column == 0 || mouse.column >= seam_x {
        return None;
    }
    // Row check — strictly inside the bordered list block.
    let content_top = LIST_HEADER_HEIGHT + 1; // +1 skips the top border
    let body_end = term_size.height.saturating_sub(LIST_FOOTER_HEIGHT);
    // Content bottom is body_end - 1 (skip bottom border). Guard against
    // a terminal so short that the list has no interior.
    let content_bottom = body_end.saturating_sub(1);
    if mouse.row < content_top || mouse.row >= content_bottom {
        return None;
    }
    // Row index into the list: items start at y = content_top (the first
    // row below the top border).
    let idx = usize::from(mouse.row - content_top);
    // The selectable range is [0, sentinel_idx] where:
    //   0                 → "Current directory"
    //   1..=saved_count   → saved workspaces
    //   saved_count + 1   → "+ New workspace" sentinel
    let sentinel_idx = state.workspaces.len() + 1;
    if idx > sentinel_idx {
        return None;
    }
    Some(idx)
}

/// Compute the seam column (0-based) for a given split percentage and
/// total terminal width. Mirrors ratatui's own `Layout::split` arithmetic
/// closely enough for hit-testing purposes.
const fn seam_column(pct: u16, width: u16) -> u16 {
    // (width * pct) / 100 — saturating so a pathological width of 0 doesn't
    // panic. Under MIN_DRAGGABLE_WIDTH this arithmetic is already gated off
    // by the caller, but keep the helper safe for direct unit-testing.
    width.saturating_mul(pct) / 100
}

/// `true` when `column` is within ±`SEAM_HIT_SLACK` of `seam_x`.
const fn near_seam(column: u16, seam_x: u16) -> bool {
    let lo = seam_x.saturating_sub(SEAM_HIT_SLACK);
    let hi = seam_x.saturating_add(SEAM_HIT_SLACK);
    column >= lo && column <= hi
}

/// Derive the new split percentage from an active drag anchor and the
/// current mouse column. Handles the signed delta safely (mouse can move
/// either way along x) without underflow on u16.
fn pct_from_drag(anchor: DragState, mouse_col: u16, width: u16) -> u16 {
    // Signed delta in columns, scaled to a percentage of terminal width.
    let delta_cols = i32::from(mouse_col) - i32::from(anchor.anchor_x);
    let delta_pct = delta_cols * 100 / i32::from(width.max(1));
    let candidate = i32::from(anchor.anchor_pct) + delta_pct;
    // Clamp into [0, 100] before the narrower [MIN..=MAX] clamp so we can
    // safely cast back to u16.
    let bounded = candidate.clamp(0, 100);
    // `as u16` is safe: bounded is in [0,100].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let narrowed = bounded as u16;
    narrowed
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
        Modal::Confirm { target: _, state } => match state.handle_key(key) {
            // Editor-side Confirm only reaches here for non-destructive
            // variants now that SaveCollapse folds into ConfirmSave.
            // Treat Commit/Cancel identically — close the modal.
            ModalOutcome::Commit(_) | ModalOutcome::Cancel => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
        Modal::MountDstChoice {
            target,
            state: modal_state,
        } => {
            let target = *target;
            let src = modal_state.src.clone();
            let outcome = modal_state.handle_key(key);
            dispatch_editor_mount_dst_choice(editor, target, &src, &outcome);
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
        // GithubPicker is a list-view modal — the editor never opens it.
        // If one somehow ends up here, treat any key as cancel so the
        // operator isn't stuck.
        Modal::GithubPicker { .. } => {
            editor.modal = None;
        }
        Modal::ConfirmSave { state: modal_state } => {
            use crate::launch::widgets::confirm_save::SaveChoice;
            match modal_state.handle_key(key) {
                ModalOutcome::Commit(SaveChoice::Save) => {
                    // Stash the plan on the editor so the outer handler
                    // (which has `paths`/`cwd`) can drive the write.
                    let pending = super::state::PendingSaveCommit {
                        effective_removals: modal_state.effective_removals.clone(),
                        final_mounts: modal_state.final_mounts.clone(),
                    };
                    editor.modal = None;
                    editor.pending_save_commit = Some(pending);
                }
                ModalOutcome::Cancel => {
                    editor.modal = None;
                    editor.exit_on_save_success = false;
                }
                ModalOutcome::Continue => {}
            }
        }
        Modal::ErrorPopup { state: popup_state } => match popup_state.handle_key(key) {
            ModalOutcome::Cancel | ModalOutcome::Commit(()) => {
                editor.modal = None;
            }
            ModalOutcome::Continue => {}
        },
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

/// Dispatch the three outcomes of `MountDstChoiceState::handle_key` into
/// concrete editor mutations. Only the `EditAddMountSrc` target is
/// meaningful here — the prelude's `CreateFirstMountSrc` target is routed
/// through `handle_prelude_modal` instead.
fn dispatch_editor_mount_dst_choice(
    editor: &mut EditorState<'_>,
    target: FileBrowserTarget,
    src: &str,
    outcome: &ModalOutcome<crate::launch::widgets::mount_dst_choice::MountDstChoice>,
) {
    use crate::launch::widgets::mount_dst_choice::MountDstChoice;
    match outcome {
        ModalOutcome::Commit(MountDstChoice::Ok) => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(crate::workspace::MountConfig {
                    src: src.to_string(),
                    dst: src.to_string(),
                    readonly: false,
                });
            }
            editor.modal = None;
        }
        ModalOutcome::Commit(MountDstChoice::Edit) => {
            if target == FileBrowserTarget::EditAddMountSrc {
                editor.pending.mounts.push(crate::workspace::MountConfig {
                    src: src.to_string(),
                    dst: src.to_string(),
                    readonly: false,
                });
                editor.modal = Some(Modal::TextInput {
                    target: super::state::TextInputTarget::MountDst,
                    state: crate::launch::widgets::text_input::TextInputState::new(
                        "Destination",
                        src,
                    ),
                });
            } else {
                editor.modal = None;
            }
        }
        ModalOutcome::Cancel => {
            editor.modal = None;
        }
        ModalOutcome::Continue => {}
    }
}

fn apply_file_browser_to_editor(
    target: FileBrowserTarget,
    editor: &mut EditorState<'_>,
    path: std::path::PathBuf,
) {
    use crate::launch::widgets::mount_dst_choice::MountDstChoiceState;
    match target {
        FileBrowserTarget::EditAddMountSrc => {
            // Defer the mount push to the choice modal: in the common case
            // the operator will take "OK" (dst = src) and we skip the
            // TextInput entirely. Only the `Edit destination` branch pushes
            // a provisional mount and opens the TextInput.
            editor.modal = Some(Modal::MountDstChoice {
                target,
                state: MountDstChoiceState::new(path.display().to_string()),
            });
        }
        FileBrowserTarget::CreateFirstMountSrc => {
            // Only meaningful in prelude path — handled by
            // `handle_prelude_modal`.
            let _ = (editor, path);
        }
    }
}

/// Prelude-side transition: mount-src and mount-dst are both known, now
/// advance to the `PickWorkdir` step by opening a `WorkdirPick` modal.
///
/// Factored out so both the `MountDstChoice::Ok` path (no `TextInput`) and
/// the `TextInputDst` commit path (operator edited dst) end the same way.
/// Callers are responsible for having already pushed the mount dst onto
/// the prelude (via `accept_mount_dst`).
fn prelude_advance_to_workdir_pick(prelude: &mut super::state::CreatePreludeState<'_>) {
    let mount = crate::workspace::MountConfig {
        src: prelude
            .pending_mount_src
            .as_ref()
            .expect("mount src must be set before advancing to workdir pick")
            .display()
            .to_string(),
        dst: prelude
            .pending_mount_dst
            .clone()
            .expect("mount dst must be set before advancing to workdir pick"),
        readonly: prelude.pending_readonly,
    };
    prelude.modal = Some(Modal::WorkdirPick {
        state: WorkdirPickState::from_mounts(&[mount]),
    });
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
        MountDstChoice,
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
        Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            ..
        }) => PreludeModalDis::MountDstChoice,
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
            // Capture the current browser cwd on Commit so step-back from
            // MountDstChoice can restore it. Read before moving the
            // outcome out of `prelude.modal`.
            let (outcome, browser_cwd) =
                if let Some(Modal::FileBrowser { state, .. }) = &mut prelude.modal {
                    let cwd = state.cwd().to_path_buf();
                    (state.handle_key(key), Some(cwd))
                } else {
                    return;
                };
            match outcome {
                ModalOutcome::Commit(path) => {
                    prelude.modal = None;
                    prelude.last_browser_cwd = browser_cwd;
                    prelude.accept_mount_src(path);
                    // Offer the 3-button choice: OK (dst=src, skip TextInput),
                    // Edit destination (open TextInput), or Cancel.
                    let src = prelude
                        .pending_mount_src
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    prelude.modal = Some(Modal::MountDstChoice {
                        target: FileBrowserTarget::CreateFirstMountSrc,
                        state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(
                            src,
                        ),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step 1 of the wizard — no prior state to rewind to.
                    // Close the modal; the outer dispatcher treats
                    // `modal = None + pending_name = None` as "cancelled"
                    // and drops back to the workspace list.
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::MountDstChoice => {
            use crate::launch::widgets::mount_dst_choice::MountDstChoice;
            let outcome = if let Some(Modal::MountDstChoice { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(MountDstChoice::Ok) => {
                    // Fast path: dst = src, skip TextInput, chain straight
                    // to WorkdirPick (mirrors the post-TextInputDst tail).
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.modal = None;
                    prelude.used_edit_dst = false;
                    prelude.accept_mount_dst(default_dst, false);
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Commit(MountDstChoice::Edit) => {
                    // Re-enter today's flow: open TextInput pre-filled with
                    // the host path. The TextInputDst branch below handles
                    // the advance to WorkdirPick once the operator commits.
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.used_edit_dst = true;
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: TextInputState::new("Destination", default_dst),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen FileBrowserSrc at the last-seen
                    // browser cwd (captured when src was committed). The
                    // mount src field is left stashed so `default_mount_dst`
                    // keeps working if the operator re-commits the same path.
                    reopen_file_browser_at_last_cwd(prelude);
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
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen MountDstChoice with the stashed src.
                    reopen_mount_dst_choice(prelude);
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
                    // Step-back: rewind to whichever dst-step the operator
                    // took — TextInputDst if they edited the destination,
                    // otherwise MountDstChoice (fast-path OK).
                    if prelude.used_edit_dst {
                        let current_dst = prelude.pending_mount_dst.clone().unwrap_or_default();
                        prelude.modal = Some(Modal::TextInput {
                            target: TextInputTarget::MountDst,
                            state: TextInputState::new("Destination", current_dst),
                        });
                    } else {
                        reopen_mount_dst_choice(prelude);
                    }
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
                    // Step-back: reopen WorkdirPick from the stashed
                    // mount src/dst — mirrors the post-TextInputDst tail.
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::Other => {}
    }
}

/// Reopen the FileBrowserSrc modal positioned at the last-seen cwd.
/// Used by step-back navigation from MountDstChoice. Silently starts at
/// `$HOME` when the browser fails to build or no cwd was recorded.
fn reopen_file_browser_at_last_cwd(prelude: &mut super::state::CreatePreludeState<'_>) {
    use super::state::FileBrowserTarget;
    let Ok(mut fb) = crate::launch::widgets::file_browser::FileBrowserState::new_from_home() else {
        prelude.modal = None;
        return;
    };
    if let Some(cwd) = prelude.last_browser_cwd.as_ref() {
        fb.set_cwd(cwd);
    }
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: fb,
    });
}

/// Reopen the MountDstChoice modal seeded from the stashed mount src.
/// Used by step-back navigation from TextInputDst / WorkdirPick.
fn reopen_mount_dst_choice(prelude: &mut super::state::CreatePreludeState<'_>) {
    use super::state::FileBrowserTarget;
    let src = prelude
        .pending_mount_src
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    prelude.modal = Some(Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(src),
    });
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
        // Same setup, then simulate the operator pressing Enter on the
        // ConfirmSave modal — this should drain pending_save_commit,
        // call commit_editor_save, and write the collapsed mount set.
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
        assert!(e.pending_save_commit.is_none(), "plan must be cleared");

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
        assert!(e.error_banner.is_none());
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
        // Set exit_on_save_success = true (as the SaveDiscardCancel Save
        // path would). After Enter on ConfirmSave, we should land back
        // on ManagerStage::List.
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
        editor.exit_on_save_success = true;
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

        assert!(
            matches!(state.stage, ManagerStage::List),
            "save with exit_on_save_success should return to the list stage"
        );
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

        let choices = resolve_github_mounts_for_workspace(&ws);
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
        let choices = resolve_github_mounts_for_workspace(&ws);
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
        // Cursor on row 1 (agent "beta"), no default set yet.
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
        // Setting a default also implies allowing the agent.
        assert!(
            e.pending.allowed_agents.iter().any(|a| a == "beta"),
            "default agent must be represented in allowed_agents"
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
}

#[cfg(test)]
mod mouse_drag_tests {
    //! Unit tests for `handle_mouse`: the list/details seam is a
    //! mouse-draggable resize affordance driven entirely from `ManagerState`.
    //! These build `MouseEvent` values directly and bypass the ratatui
    //! event loop — enough to pin the seam hit-test + drag math without a
    //! real terminal.
    use super::{handle_mouse, resolve_github_mounts_for_workspace};
    use crate::launch::manager::state::{
        DEFAULT_SPLIT_PCT, EditorState, MAX_SPLIT_PCT, MIN_SPLIT_PCT, ManagerStage, ManagerState,
        Modal,
    };
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    /// Build a `ManagerState` in the List stage at the default split,
    /// with no workspaces and no modal.
    fn list_state() -> ManagerState<'static> {
        let config = crate::config::AppConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
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
        assert!(state.drag_state.is_none(), "right-pane click must not drag",);
    }

    #[test]
    fn drag_ignored_when_list_modal_open() {
        // GithubPicker is the only list-level modal today. Any mouse event
        // while it's up must be a silent no-op — the picker owns the
        // keyboard + (implicitly) the mouse focus.
        let mut state = list_state();
        // Use resolve_github_mounts_for_workspace indirectly — easier to
        // just synthesize a GithubPicker state with an arbitrary choice.
        // The picker's exact contents don't matter; only `list_modal.is_some()`.
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![MountConfig {
                src: "/w".into(),
                dst: "/w".into(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        // Ensure the helper signature compiles (guards against future refactors).
        let _ = resolve_github_mounts_for_workspace(&ws);
        state.list_modal = Some(Modal::GithubPicker {
            state: crate::launch::widgets::github_picker::GithubPickerState::new(vec![
                crate::launch::widgets::github_picker::GithubChoice {
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
    fn drag_ignored_on_non_list_stage() {
        // While in the Editor (or any non-List stage), mouse events are
        // ignored outright — no seam to drag.
        let mut state = list_state();
        let ws = WorkspaceConfig {
            workdir: "/w".into(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
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
    // row must be consumed by the open-URL path (best-effort; silent on
    // failure) — observable side-effect: the drag-anchor never latches.

    /// Term of 120x40 ⇒ FileBrowser modal at (18, 9, 84, 22); URL row at
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
    fn mouse_down_on_url_row_in_prelude_with_url_does_not_drag() {
        use crate::launch::manager::state::CreatePreludeState;
        use crate::launch::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        // Build a FileBrowser at `parent`, select the repo, open git prompt,
        // and inject a URL so the URL row renders.
        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::launch::manager::state::FileBrowserTarget::CreateFirstMountSrc,
                state: fb,
            }),
            ..CreatePreludeState::default()
        };
        state.stage = ManagerStage::CreatePrelude(prelude);

        // URL row at y = 17 for this term size; centre column ≈ 60.
        handle_mouse(&mut state, mouse_down_at(60, 17), term_120x40());
        // No drag latched — URL click is consumed before the seam path.
        assert!(
            state.drag_state.is_none(),
            "URL click must not start a seam drag",
        );
    }

    #[test]
    fn mouse_down_outside_url_row_in_prelude_is_silent_noop() {
        use crate::launch::manager::state::CreatePreludeState;
        use crate::launch::widgets::file_browser::FileBrowserState;
        let mut state = list_state();
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut fb = FileBrowserState::new_at(tmp.path().to_path_buf(), parent);
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.handle_key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        });
        fb.pending_git_url = Some("file:///tmp/unreachable".to_string());

        let prelude = CreatePreludeState {
            modal: Some(Modal::FileBrowser {
                target: crate::launch::manager::state::FileBrowserTarget::CreateFirstMountSrc,
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
    // Layout (100x30 terminal, header=3 footer=2 body=25):
    //   y = 0..=2   → header (chunks[0])
    //   y = 3       → body top border (list block)
    //   y = 4       → list item 0 ("Current directory")
    //   y = 5       → list item 1 (first saved workspace)
    //   ...
    //   y = 28      → body bottom border
    //   y = 29      → footer (chunks[2])
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

    /// Build a list state with `n` saved workspaces (row 0 + n + sentinel).
    fn list_state_with_saved(n: usize) -> ManagerState<'static> {
        let mut config = crate::config::AppConfig::default();
        for i in 0..n {
            config.workspaces.insert(
                format!("ws-{i:02}"),
                WorkspaceConfig {
                    workdir: format!("/w/{i}"),
                    mounts: vec![],
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                    env: std::collections::BTreeMap::new(),
                    agents: std::collections::BTreeMap::new(),
                },
            );
        }
        let tmp = tempfile::tempdir().unwrap();
        ManagerState::from_config(&config, tmp.path())
    }

    #[test]
    fn click_on_first_row_sets_selected_to_zero() {
        // y=4 = first list item (index 0, "Current directory").
        let mut state = list_state_with_saved(3);
        state.selected = 2;
        handle_mouse(&mut state, mouse_at(10, 4), term(100));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn click_on_fifth_row_sets_selected_to_four() {
        // y=8 = fifth list row (index 4). Needs enough saved workspaces
        // to make index 4 a valid selection target.
        let mut state = list_state_with_saved(5);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 4);
    }

    #[test]
    fn click_on_sentinel_row_sets_selected_to_sentinel_idx() {
        // 3 saved workspaces ⇒ rows are:
        //   y=4  → index 0 ("Current directory")
        //   y=5,6,7 → indices 1, 2, 3 (saved)
        //   y=8  → index 4 (sentinel "+ New workspace")
        let mut state = list_state_with_saved(3);
        state.selected = 0;
        handle_mouse(&mut state, mouse_at(10, 8), term(100));
        assert_eq!(state.selected, 4, "sentinel_idx = saved_count + 1 = 4");
    }

    #[test]
    fn click_outside_list_rows_does_not_change_selected() {
        // Several "outside" positions must all leave selected untouched:
        //   - Click above the list (y < 4, e.g. in the header)
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
        handle_mouse(&mut state, mouse_at(10, 3), term(100));
        assert_eq!(state.selected, initial, "click on top border");

        // On the left border column.
        handle_mouse(&mut state, mouse_at(0, 4), term(100));
        assert_eq!(state.selected, initial, "click on left border");

        // Past the sentinel row (y=9+ when we have 3 saved workspaces).
        handle_mouse(&mut state, mouse_at(10, 10), term(100));
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
        // `DEFAULT_SPLIT_PCT`. y=5 maps to list index 1 in our layout —
        // if seam didn't win, selection would flip to 1.
        handle_mouse(&mut state, mouse_at(DEFAULT_SPLIT_PCT, 5), term(100));
        assert!(state.drag_state.is_some(), "click on seam must start drag");
        assert_eq!(
            state.selected, 0,
            "seam-click must not change selection even when y lands on a list row"
        );
    }
}
