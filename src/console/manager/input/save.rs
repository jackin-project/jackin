//! Editor save flow: two-phase commit with planner validation, a
//! `ConfirmSave` preview modal, and `ConfigEditor`-driven writes.

use super::super::state::{
    EditorMode, EditorSaveFlow, EditorState, ManagerListRow, ManagerStage, ManagerState, Modal,
    Toast, ToastKind,
};
use crate::config::AppConfig;
use crate::config::editor::EnvScope;
use crate::paths::JackinPaths;

/// Phase 1: validate, plan, open `ConfirmSave`. Validation failures
/// route to `EditorSaveFlow::Error` as an inline banner (popup is
/// reserved for phase-2 commit errors). The plan is stashed on the
/// modal so commit doesn't re-run `plan_edit`/`plan_create`.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub(super) fn begin_editor_save(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    exit_on_success: bool,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    // Clear any stale banner from a prior attempt.
    editor.save_flow = EditorSaveFlow::Idle;

    // Classify first so mutating arms don't keep editor.mode borrowed.
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

    let lines = build_confirm_save_lines(editor, config, &collapse_lines);
    let mut confirm_state = crate::console::widgets::confirm_save::ConfirmSaveState::new(lines);
    confirm_state.effective_removals = effective_removals;
    confirm_state.final_mounts = final_mounts;
    confirm_state.has_collapses = has_collapses;
    editor.modal = Some(Modal::ConfirmSave {
        state: confirm_state,
    });
    editor.save_flow = EditorSaveFlow::Confirming { exit_on_success };
    Ok(())
}

/// Phase 2: write to disk via `ConfigEditor` (no CLI subprocess). On
/// Err → `EditorSaveFlow::Error` + `ErrorPopup`. On Ok → refresh
/// editor snapshot, optionally bounce to list.
///
/// **Source-drift safeguard (Task 10.3):** before any disk write, runs
/// the same `detect_workspace_edit_drift` check the CLI uses. Running
/// containers with preserved isolated state for an affected mount → open
/// `ErrorPopup` ("eject first") and abort. Stopped containers with
/// preserved state → open a `ConfirmTarget::DeleteIsolatedAndSave`
/// confirm modal that, on Yes, re-stashes the plan with
/// `delete_isolated_acknowledged = true` so the second commit pass skips
/// the check and runs `force_cleanup_isolated` for each affected record.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub(super) fn commit_editor_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    plan: super::super::state::PendingSaveCommit,
    exit_on_success: bool,
) -> anyhow::Result<()> {
    commit_editor_save_with_runner(
        state,
        config,
        paths,
        cwd,
        plan,
        exit_on_success,
        &mut crate::docker::ShellRunner::default(),
    )
}

/// Test seam: the same flow as [`commit_editor_save`] but with an
/// injectable `CommandRunner`. Production code paths thread through the
/// public wrapper above with `ShellRunner::default()`. Tests pass a
/// `FakeRunner` so the drift detection branch is exercised without a
/// real Docker daemon.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub(super) fn commit_editor_save_with_runner(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    plan: super::super::state::PendingSaveCommit,
    exit_on_success: bool,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };

    // Same classify-first pattern as begin_editor_save.
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

    // Operator already approved the collapsed mount set in
    // ConfirmSave; honour it now. Clone so subsequent source-drift logic
    // can still inspect the full `plan`.
    if let Some(final_mounts) = plan.final_mounts.clone() {
        editor.pending.mounts = final_mounts;
    }

    // ── Source-drift safeguard ────────────────────────────────────────
    // Only meaningful in Edit mode — Create has no preserved state. Skip
    // entirely if the operator already acknowledged the modal on a
    // previous commit pass.
    if let SaveMode::Edit { original_name } = &save_mode
        && !plan.delete_isolated_acknowledged
    {
        // Build prospective mounts mirroring `edit_workspace`'s merge
        // order: drop `effective_removals`, then upsert each pending
        // mount over the existing on-disk set.
        let current_ws = config.workspaces.get(original_name).cloned();
        if let Some(current_ws) = current_ws {
            let prospective_mounts = build_prospective_mounts(
                &current_ws.mounts,
                &editor.pending.mounts,
                &plan.effective_removals,
            );
            match crate::config::detect_workspace_edit_drift(
                paths,
                original_name,
                &prospective_mounts,
                runner,
            ) {
                Err(e) => {
                    open_save_error_popup(editor, &e.to_string());
                    return Ok(());
                }
                Ok(detection) => {
                    if !detection.running_containers.is_empty() {
                        let msg = format!(
                            "Cannot save: {} container(s) are running with isolated state for an affected mount: {}; eject them first.",
                            detection.running_containers.len(),
                            detection.running_containers.join(", "),
                        );
                        open_save_error_popup(editor, &msg);
                        return Ok(());
                    }
                    if !detection.stopped_records.is_empty() {
                        let affected_containers: Vec<String> = detection
                            .stopped_records
                            .iter()
                            .map(|r| r.container_name.clone())
                            .collect();
                        let prompt = format!(
                            "Edit affects preserved isolated state for {} stopped container(s):\n  {}\n\n\
                             Delete the preserved state and save?",
                            affected_containers.len(),
                            affected_containers.join("\n  "),
                        );
                        editor.modal = Some(Modal::Confirm {
                            target: super::super::state::ConfirmTarget::DeleteIsolatedAndSave {
                                plan,
                                exit_on_success,
                                affected_containers,
                            },
                            state: crate::console::widgets::confirm::ConfirmState::new(prompt),
                        });
                        // Park the save flow until the operator answers the
                        // modal. The modal handler re-stashes the plan
                        // with `delete_isolated_acknowledged = true` on Yes.
                        editor.save_flow =
                            super::super::state::EditorSaveFlow::Confirming { exit_on_success };
                        return Ok(());
                    }
                }
            }
        }
    }

    // Acknowledged — clean up preserved state for each affected record
    // before the on-disk write so a partial failure leaves the system in
    // a recoverable state. Mirrors the CLI's `--delete-isolated-state`
    // branch in `app/mod.rs`.
    if let SaveMode::Edit { original_name } = &save_mode
        && plan.delete_isolated_acknowledged
    {
        let current_ws = config.workspaces.get(original_name).cloned();
        if let Some(current_ws) = current_ws {
            let prospective_mounts = build_prospective_mounts(
                &current_ws.mounts,
                &editor.pending.mounts,
                &plan.effective_removals,
            );
            // Re-detect to avoid a TOCTOU window where state changed
            // between the confirm modal opening and the operator's Yes.
            // `force_cleanup_isolated` is idempotent so re-running is safe.
            match crate::config::detect_workspace_edit_drift(
                paths,
                original_name,
                &prospective_mounts,
                runner,
            ) {
                Err(e) => {
                    open_save_error_popup(editor, &e.to_string());
                    return Ok(());
                }
                Ok(detection) => {
                    for rec in &detection.stopped_records {
                        let container_dir = paths.data_dir.join(&rec.container_name);
                        if let Err(e) = crate::isolation::cleanup::force_cleanup_isolated(
                            rec,
                            &container_dir,
                            runner,
                        ) {
                            open_save_error_popup(editor, &e.to_string());
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    let ce_res = crate::config::ConfigEditor::open(paths);
    let mut ce = match ce_res {
        Ok(ce) => ce,
        Err(e) => {
            open_save_error_popup(editor, &e.to_string());
            return Ok(());
        }
    };

    // Defer `editor.mode` rename until ce.save() succeeds — a later
    // failure would otherwise leave the UI advertising a name that
    // never reached disk. `current_name` carries the post-rename name
    // for the env-diff step.
    let (pending_rename, current_name): (Option<String>, String) = match save_mode {
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

            (rename_to, current_name)
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
            (None, name)
        }
    };

    // `create_workspace`/`edit_workspace` don't touch env — TUI
    // manages env exclusively through this diff loop.
    apply_env_diff(&mut ce, &current_name, &editor.original, &editor.pending)?;

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
                // Carry the toast across `from_config_with_cache_and_op`
                // (which would otherwise discard it) so create-save and
                // Esc→Save keep positive feedback parity with direct `s`.
                let carry_toast = state.toast.take();
                let cache = state.op_cache.clone();
                let op_available = state.op_available;
                *state =
                    ManagerState::from_config_with_cache_and_op(config, cwd, cache, op_available);
                state.toast = carry_toast;
                // Land on the workspace that was just saved.
                let saved_count = state.workspaces.len();
                if let Some(idx) = state.workspaces.iter().position(|w| w.name == current_name) {
                    state.selected =
                        ManagerListRow::SavedWorkspace(idx).to_screen_index(saved_count);
                }
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
        state: crate::console::widgets::error_popup::ErrorPopupState::new(
            "Save failed",
            message.to_string(),
        ),
    });
    editor.save_flow = EditorSaveFlow::Error {
        message: message.to_string(),
    };
}

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
                Span::styled("Allowed roles: ", heading),
                Span::styled(allowed_agents_summary(editor, config), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Default role: ", heading),
                Span::styled(
                    editor
                        .pending
                        .default_role
                        .clone()
                        .unwrap_or_else(|| "(none)".into()),
                    value,
                ),
            ]));
            if editor.pending.keep_awake.enabled {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Keep awake: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            let env_lines = env_diff_lines(&editor.original, &editor.pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
        }
        EditorMode::Edit { name } => {
            let display_name = editor.pending_name.clone().unwrap_or_else(|| name.clone());
            out.push(Line::from(vec![
                Span::styled("Edit workspace: ", heading),
                Span::styled(display_name, value),
            ]));

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

            let mount_diffs = super::super::state::classify_mount_diffs(
                &editor.original.mounts,
                &editor.pending.mounts,
            );
            let any_diff = mount_diffs
                .iter()
                .any(|d| !matches!(d, super::super::state::MountDiff::Unchanged(_)));
            if any_diff {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Mounts:", heading)));
                for diff in &mount_diffs {
                    match diff {
                        super::super::state::MountDiff::Added(m) => {
                            out.push(Line::from(Span::styled(
                                format!("  + {}", mount_summary(m)),
                                value,
                            )));
                        }
                        super::super::state::MountDiff::Removed(m) => {
                            out.push(Line::from(Span::styled(
                                format!("  - {}", mount_summary(m)),
                                dim,
                            )));
                        }
                        super::super::state::MountDiff::Modified { original, pending } => {
                            // Modified row: show the new state (`~`) with a
                            // dimmed `was:` follow-up so the operator can
                            // see exactly what changed without reading a
                            // remove + add pair.
                            out.push(Line::from(Span::styled(
                                format!("  ~ {}", mount_summary(pending)),
                                value,
                            )));
                            out.push(Line::from(Span::styled(
                                format!("      was: {}", mount_summary(original)),
                                dim,
                            )));
                        }
                        super::super::state::MountDiff::Unchanged(_) => {}
                    }
                }
            }

            let added_agents: Vec<_> = editor
                .pending
                .allowed_roles
                .iter()
                .filter(|a| !editor.original.allowed_roles.contains(a))
                .collect();
            let removed_agents: Vec<_> = editor
                .original
                .allowed_roles
                .iter()
                .filter(|a| !editor.pending.allowed_roles.contains(a))
                .collect();
            if !added_agents.is_empty() || !removed_agents.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Allowed roles:", heading)));
                for a in &added_agents {
                    out.push(Line::from(Span::styled(format!("  + {a}"), value)));
                }
                for a in &removed_agents {
                    out.push(Line::from(Span::styled(format!("  - {a}"), dim)));
                }
            }

            if editor.pending.default_role != editor.original.default_role {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Default role:", heading)));
                if let Some(old) = &editor.original.default_role {
                    out.push(Line::from(Span::styled(format!("  - {old}"), dim)));
                }
                if let Some(new) = &editor.pending.default_role {
                    out.push(Line::from(Span::styled(format!("  + {new}"), value)));
                } else {
                    out.push(Line::from(Span::styled("  + (none)", value)));
                }
            }

            if editor.pending.keep_awake.enabled != editor.original.keep_awake.enabled {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Keep awake:", heading)));
                let old_label = if editor.original.keep_awake.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let new_label = if editor.pending.keep_awake.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                out.push(Line::from(Span::styled(format!("  - {old_label}"), dim)));
                out.push(Line::from(Span::styled(format!("  + {new_label}"), value)));
            }

            let env_lines = env_diff_lines(&editor.original, &editor.pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
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

fn mount_summary(m: &crate::workspace::MountConfig) -> String {
    let src = crate::tui::shorten_home(&m.src);
    let kind = super::super::mount_info::inspect(&m.src);
    let rw = if m.readonly { "ro" } else { "rw" };
    let isolation = m.isolation.as_str();
    format!("{src}  ({rw}, {isolation}, {})", kind.label())
}

fn allowed_agents_summary(editor: &EditorState<'_>, config: &AppConfig) -> String {
    if super::super::agent_allow::allows_all_agents(&editor.pending) {
        return format!("any ({} roles)", config.roles.len());
    }
    editor.pending.allowed_roles.join(", ")
}

/// Per-role sections are prefixed with `  <role>:` so a single
/// "Env vars:" heading hosts both workspace and override deltas.
fn env_diff_lines(
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let mut out: Vec<Line<'static>> = Vec::new();

    append_env_map_diff_lines(&mut out, None, &original.env, &pending.env, value, dim);

    let agent_keys: std::collections::BTreeSet<&String> =
        original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = std::collections::BTreeMap::<String, crate::operator_env::EnvValue>::new();
    for role in agent_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        // Pre-check if there are any deltas for this role; only emit
        // the role header when there are.
        let mut probe: Vec<Line<'static>> = Vec::new();
        append_env_map_diff_lines(&mut probe, None, orig_env, pend_env, value, dim);
        if !probe.is_empty() {
            out.push(Line::from(Span::styled(format!("  role {role}:"), value)));
            append_env_map_diff_lines(&mut out, Some("  "), orig_env, pend_env, value, dim);
        }
    }
    out
}

/// Append `+ KEY = VALUE` / `- KEY` lines to `out` for the diff between
/// two env maps. `indent` (`None` or `Some("  ")`) controls per-role
/// sub-indent — workspace-level lines use two spaces to match existing
/// diff styling; per-role lines nest one extra level.
fn append_env_map_diff_lines(
    out: &mut Vec<ratatui::text::Line<'static>>,
    indent: Option<&str>,
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) {
    use ratatui::text::{Line, Span};
    let prefix = indent.unwrap_or("");
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {}", v.as_display_str()),
                value,
            ))),
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            out.push(Line::from(Span::styled(format!("{prefix}  - {k}"), dim)));
        }
    }
}

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

/// Mirror the merge order `AppConfig::edit_workspace` uses to build the
/// post-edit mount list, so the source-drift check in
/// `commit_editor_save` evaluates the same shape that will land on disk.
/// Steps:
///   1. Drop every mount whose dst is in `effective_removals`.
///   2. For each pending mount, upsert (replace by dst, otherwise push).
fn build_prospective_mounts(
    current: &[crate::workspace::MountConfig],
    pending: &[crate::workspace::MountConfig],
    effective_removals: &[String],
) -> Vec<crate::workspace::MountConfig> {
    let mut out: Vec<crate::workspace::MountConfig> = current
        .iter()
        .filter(|m| !effective_removals.iter().any(|d| d == &m.dst))
        .cloned()
        .collect();
    for upsert in pending {
        if let Some(existing) = out.iter_mut().find(|existing| existing.dst == upsert.dst) {
            *existing = upsert.clone();
        } else {
            out.push(upsert.clone());
        }
    }
    out
}

/// Roles present only in `original` get all keys removed.
fn apply_env_diff(
    ce: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
) -> anyhow::Result<()> {
    let ws_scope = EnvScope::Workspace(workspace_name.to_string());
    apply_env_map_diff(ce, &ws_scope, &original.env, &pending.env)?;

    // Union so roles on only one side are caught.
    let agent_keys: std::collections::BTreeSet<&String> =
        original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = std::collections::BTreeMap::<String, crate::operator_env::EnvValue>::new();
    for role in agent_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        let scope = EnvScope::WorkspaceRole {
            workspace: workspace_name.to_string(),
            role: role.clone(),
        };
        apply_env_map_diff(ce, &scope, orig_env, pend_env)?;
    }
    Ok(())
}

fn apply_env_map_diff(
    ce: &mut crate::config::ConfigEditor,
    scope: &EnvScope,
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
) -> anyhow::Result<()> {
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => {
                ce.set_env_var(scope, k, v.clone())?;
            }
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            // `remove_env_var` returns false when the path is already
            // missing — treat as a no-op success.
            let _ = ce.remove_env_var(scope, k);
        }
    }
    Ok(())
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
    for a in &pending.allowed_roles {
        if !original.allowed_roles.contains(a) {
            edit.allowed_agents_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_roles {
        if !pending.allowed_roles.contains(a) {
            edit.allowed_agents_to_remove.push(a.clone());
        }
    }
    if pending.default_role != original.default_role {
        edit.default_role = Some(pending.default_role.clone());
    }
    if pending.keep_awake.enabled != original.keep_awake.enabled {
        edit.keep_awake_enabled = Some(pending.keep_awake.enabled);
    }
    edit
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    use super::super::super::state::{
        EditorMode, EditorSaveFlow, EditorState, ManagerStage, ManagerState, Modal, ToastKind,
    };
    use super::super::test_support::{key, mount};
    use super::{begin_editor_save, commit_editor_save};
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
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
        config.workspaces.insert(name.to_string(), ws);
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
            workdir: "/workspace/proj".into(),
            mounts: vec![mount("/work", "/workspace/proj")],
            keep_awake: KeepAwakeConfig { enabled: false },
            ..Default::default()
        };

        // No change → no field set.
        let pending_unchanged = original.clone();
        let edit = super::build_workspace_edit(&original, &pending_unchanged);
        assert_eq!(edit.keep_awake_enabled, None);

        // Flip on → Some(true).
        let pending_on = WorkspaceConfig {
            keep_awake: KeepAwakeConfig { enabled: true },
            ..original.clone()
        };
        let edit = super::build_workspace_edit(&original, &pending_on);
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
        let edit = super::build_workspace_edit(&original_on, &pending_off);
        assert_eq!(edit.keep_awake_enabled, Some(false));
    }

    #[test]
    fn save_editor_opens_confirm_save_on_edit_driven_collapse() {
        let ws = WorkspaceConfig {
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

        // Step 2: Enter on the ConfirmSave modal (default focus = Save)
        // commits the save.
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

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
    fn readonly_mismatch_produces_error_banner_no_write() {
        // Add a rw /work that would subsume an existing ro /work/sub —
        // plan_edit must reject with ReadonlyMismatch. Per spec, hard
        // planner errors surface as an inline banner, NOT as the new
        // ErrorPopup (which is reserved for commit-time failures).
        let ws = WorkspaceConfig {
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
            ..Default::default()
        };
        let (tmp, paths, mut config) =
            setup_with_workspace("legacy-workspace", ws.clone()).unwrap();

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

    #[test]
    fn s_with_zero_changes_is_noop() {
        let ws = WorkspaceConfig {
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
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

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
            workdir: "/w".into(),
            mounts: vec![mount("/w", "/w")],
            ..Default::default()
        };
        let (tmp, paths, mut config) = setup_with_workspace("z-second", ws.clone()).unwrap();
        config.workspaces.insert(
            "a-first".to_string(),
            WorkspaceConfig {
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
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

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
            ..Default::default()
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
        let bad_plan = crate::console::manager::state::PendingSaveCommit {
            effective_removals: vec!["/does/not/exist".to_string()],
            final_mounts: None,
            delete_isolated_acknowledged: false,
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
    fn confirm_save_s_exits_to_list_on_success() {
        // `s` + Enter on ConfirmSave returns the operator to the list,
        // consistent with the Esc→Save path.
        let ws = WorkspaceConfig {
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
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

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
            workdir: "/a".into(),
            mounts: vec![mount("/a", "/a")],
            ..Default::default()
        };
        let ws_b = WorkspaceConfig {
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
            ..Default::default()
        };
        let ws_b = WorkspaceConfig {
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
            &super::super::super::state::TextInputTarget::Name,
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
    fn edit_mode_confirm_save_shows_diff() {
        let ws = WorkspaceConfig {
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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
            workdir: dst.into(),
            mounts: vec![MountConfig {
                src: original_src.into(),
                dst: dst.into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
            }],
            allowed_roles: vec![],
            default_role: None,
            agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: KeepAwakeConfig::default(),
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
            selector_key: container.trim_start_matches("jackin-").into(),
            container_name: container.into(),
            cleanup_status: CleanupStatus::Active,
        };
        write_records(&cdir, std::slice::from_ref(&rec)).unwrap();

        (tmp, paths, config, ws)
    }

    /// `detect_workspace_edit_drift` issues two `capture` calls on the
    /// runner: `list_records_for_workspace` is filesystem-only (no
    /// runner traffic), but `list_role_names(running)` issues a `docker
    /// ps` capture that returns the newline-separated container names.
    /// Tests construct the runner with the appropriate output queued.
    fn fake_runner_with_running(names: &[&str]) -> crate::runtime::FakeRunner {
        let mut runner = crate::runtime::FakeRunner::default();
        let joined = if names.is_empty() {
            String::new()
        } else {
            format!("{}\n", names.join("\n"))
        };
        runner.capture_queue.push_back(joined);
        runner
    }

    #[test]
    fn save_blocks_with_error_popup_when_running_container_has_drifted_state() {
        let (tmp, paths, mut config, ws) =
            setup_with_isolated_record("driftws", "/old/src", "/workspace/x", "jackin-driftws");
        let cwd = tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("driftws".into(), ws);
        // Operator changes the src — this drifts the recorded original_src.
        editor.pending.mounts[0].src = "/new/src".into();
        state.stage = ManagerStage::Editor(editor);

        // Drive the save flow: `s` opens ConfirmSave; Enter on the modal
        // produces the PendingCommit signal we hand to commit_editor_save_with_runner.
        press_s(&mut state, &mut config, &paths, cwd);
        let plan = match &mut state.stage {
            ManagerStage::Editor(e) => match &e.modal {
                Some(Modal::ConfirmSave { state: m }) => {
                    crate::console::manager::state::PendingSaveCommit {
                        effective_removals: m.effective_removals.clone(),
                        final_mounts: m.final_mounts.clone(),
                        delete_isolated_acknowledged: false,
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

        let mut runner = fake_runner_with_running(&["jackin-driftws"]);
        super::commit_editor_save_with_runner(
            &mut state,
            &mut config,
            &paths,
            cwd,
            plan,
            false,
            &mut runner,
        )
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

    #[test]
    fn save_opens_confirm_modal_when_stopped_container_has_drifted_state() {
        let (tmp, paths, mut config, ws) =
            setup_with_isolated_record("driftws2", "/old/src", "/workspace/x", "jackin-driftws2");
        let cwd = tmp.path();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("driftws2".into(), ws);
        editor.pending.mounts[0].src = "/new/src".into();
        state.stage = ManagerStage::Editor(editor);

        press_s(&mut state, &mut config, &paths, cwd);
        let plan = match &mut state.stage {
            ManagerStage::Editor(e) => match &e.modal {
                Some(Modal::ConfirmSave { state: m }) => {
                    crate::console::manager::state::PendingSaveCommit {
                        effective_removals: m.effective_removals.clone(),
                        final_mounts: m.final_mounts.clone(),
                        delete_isolated_acknowledged: false,
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
        let mut runner = fake_runner_with_running(&[]);
        super::commit_editor_save_with_runner(
            &mut state,
            &mut config,
            &paths,
            cwd,
            plan,
            false,
            &mut runner,
        )
        .unwrap();

        let ManagerStage::Editor(e) = &state.stage else {
            panic!("editor stage expected");
        };
        match &e.modal {
            Some(Modal::Confirm {
                target:
                    crate::console::manager::state::ConfirmTarget::DeleteIsolatedAndSave {
                        affected_containers,
                        ..
                    },
                ..
            }) => {
                assert_eq!(
                    affected_containers,
                    &vec!["jackin-driftws2".to_string()],
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

    #[test]
    fn pre_save_diff_renders_op_ref_via_breadcrumb_not_uuid() {
        use crate::operator_env::{EnvValue, OpRef};
        use ratatui::style::Style;

        let original = std::collections::BTreeMap::new();
        let mut pending = std::collections::BTreeMap::new();
        pending.insert(
            "TOKEN".to_string(),
            EnvValue::OpRef(OpRef {
                op: "op://abc/def/fld".to_string(),
                path: "Private/Claude/auth".to_string(),
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
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
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
}
