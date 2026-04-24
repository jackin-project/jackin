//! Editor save flow: two-phase commit with planner validation, a
//! `ConfirmSave` preview modal, and `ConfigEditor`-driven writes.

use super::super::state::{
    EditorMode, EditorSaveFlow, EditorState, ManagerStage, ManagerState, Modal, Toast, ToastKind,
};
use crate::config::AppConfig;
use crate::paths::JackinPaths;

/// Phase 1 of the save flow: run pre-save validation, compute the
/// plan, and open a `Modal::ConfirmSave` summarising the change set.
///
/// Validation failures (missing name, planner reject, pre-existing-only
/// collapse) surface via `EditorSaveFlow::Error { message }` and render
/// as an inline banner — NOT as an `ErrorPopup`. The popup is reserved
/// for commit-time errors (phase 2). See `EditorSaveFlow` for the full
/// state machine.
///
/// `exit_on_success` is remembered on the `Confirming` variant so that
/// the commit phase can decide whether to bounce to the workspace list
/// after a successful write.
///
/// On success, the function stashes the planner's `effective_removals`
/// / `final_mounts` on the modal state so the commit path doesn't need
/// to re-run `plan_edit`/`plan_create`.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub(super) fn begin_editor_save(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    exit_on_success: bool,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    // A stale banner from a previous cycle should clear now that the
    // operator has kicked off a fresh save attempt.
    editor.save_flow = EditorSaveFlow::Idle;

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
                editor.save_flow = EditorSaveFlow::Error {
                    message: format!("workspace {original_name:?} no longer exists in config"),
                };
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
                    editor.save_flow = EditorSaveFlow::Error {
                        message: e.to_string(),
                    };
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
                        editor.save_flow = EditorSaveFlow::Error {
                            message: format!(
                                "pre-existing redundant mount(s) in this workspace: {}; \
                                 run `jackin' workspace prune {original_name}` to clean up",
                                details.join(", "),
                            ),
                        };
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
                editor.save_flow = EditorSaveFlow::Error {
                    message: "missing workspace name".into(),
                };
                return Ok(());
            }
            match crate::workspace::planner::plan_create(
                &editor.pending.workdir,
                editor.pending.mounts.clone(),
                false,
            ) {
                Err(e) => {
                    editor.save_flow = EditorSaveFlow::Error {
                        message: e.to_string(),
                    };
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
    editor.save_flow = EditorSaveFlow::Confirming { exit_on_success };
    Ok(())
}

/// Phase 2 of the save flow: the operator clicked Save in the `ConfirmSave`
/// dialog. Actually write to the on-disk config via the internal
/// `ConfigEditor` API (NO CLI subprocess).
///
/// On Err, transitions the editor's `save_flow` to `Error` and surfaces
/// the failure as an `ErrorPopup`. On Ok, refreshes the editor's
/// origin-of-truth snapshot and — if `exit_on_success` is set —
/// transitions the whole manager back to the list view.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub(super) fn commit_editor_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    plan: super::super::state::PendingSaveCommit,
    exit_on_success: bool,
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
            return Ok(());
        }
    };

    // Track a pending rename across the inner match. Finding #4: the
    // `editor.mode = EditorMode::Edit { name }` mutation is deferred
    // until after `ce.save()` succeeds — otherwise a later
    // `ce.edit_workspace` or `ce.save` failure would leave the editor UI
    // advertising the new name while nothing has reached disk.
    let pending_rename: Option<String> = match save_mode {
        SaveMode::Edit { original_name } => {
            let mut current_name = original_name.clone();
            let mut rename_to: Option<String> = None;
            let pending_name = editor.pending_name.clone();
            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                if let Err(e) = ce.rename_workspace(&original_name, &new_name) {
                    open_save_error_popup(editor, &e.to_string());
                    return Ok(());
                }
                current_name.clone_from(&new_name);
                rename_to = Some(new_name);
            }

            let mut edit = build_workspace_edit(&editor.original, &editor.pending);
            edit.remove_destinations = plan.effective_removals;

            if let Err(e) = ce.edit_workspace(&current_name, edit) {
                open_save_error_popup(editor, &e.to_string());
                return Ok(());
            }

            // Defer `editor.mode` mutation — only commit it in the
            // `ce.save()` success arm below.
            rename_to
        }
        SaveMode::Create => {
            let Some(name) = editor.pending_name.clone() else {
                open_save_error_popup(editor, "missing workspace name");
                return Ok(());
            };
            if let Err(e) = ce.create_workspace(&name, editor.pending.clone()) {
                open_save_error_popup(editor, &e.to_string());
                return Ok(());
            }
            None
        }
    };

    match ce.save() {
        Ok(fresh) => {
            *config = fresh;
            // Refresh editor origin-of-truth; keep the operator on the
            // editor (direct `s` press) OR bounce to list (Esc→Save path).
            if let ManagerStage::Editor(editor) = &mut state.stage {
                // Apply the deferred rename now that the whole write has
                // reached disk. Doing this BEFORE the `editor.original` /
                // `editor.pending` refresh below means that refresh can
                // look up the new name in `config.workspaces`.
                if let Some(new_name) = pending_rename {
                    editor.mode = EditorMode::Edit { name: new_name };
                }
                let change_count = editor.change_count();
                if let EditorMode::Edit { name } = &editor.mode
                    && let Some(ws) = config.workspaces.get(name)
                {
                    editor.original = ws.clone();
                    editor.pending = ws.clone();
                }
                editor.save_flow = EditorSaveFlow::Idle;
                state.toast = Some(Toast {
                    message: format!("saved · {change_count} changes written"),
                    kind: ToastKind::Success,
                    shown_at: std::time::Instant::now(),
                });
            }
            if exit_on_success
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
                //
                // `ManagerState::from_config` allocates a fresh state
                // with `toast: None`, which would discard the success
                // toast we just set above — leaving the two exit-to-list
                // flows (create-save, Esc→Save) with no positive
                // feedback while direct `s` saves (which stay on the
                // editor) keep theirs. Carry the toast across the reset.
                let carry_toast = state.toast.take();
                *state = ManagerState::from_config(config, cwd);
                state.toast = carry_toast;
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                open_save_error_popup(editor, &e.to_string());
            }
        }
    }
    Ok(())
}

pub(super) fn open_save_error_popup(editor: &mut EditorState<'_>, message: &str) {
    editor.modal = Some(Modal::ErrorPopup {
        state: crate::launch::widgets::error_popup::ErrorPopupState::new(
            "Save failed",
            message.to_string(),
        ),
    });
    editor.save_flow = EditorSaveFlow::Error {
        message: message.to_string(),
    };
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
    let kind = super::super::mount_info::inspect(&m.src);
    let rw = if m.readonly { "ro" } else { "rw" };
    format!("{src}  ({rw}, {})", kind.label())
}

/// Summarise the allowed-agent selection — `any (N agents)` when the
/// workspace lets every configured agent run, otherwise a comma-separated
/// list.
fn allowed_agents_summary(editor: &EditorState<'_>, config: &AppConfig) -> String {
    if super::super::agent_allow::allows_all_agents(&editor.pending) {
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

pub(super) fn build_workspace_edit(
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
