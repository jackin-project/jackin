//! Editor save flow: two-phase commit with planner validation, a
//! `ConfirmSave` preview modal, and service-backed config writes.
#![allow(
    clippy::items_after_test_module,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

use crate::services::config_save::{
    EditorSavePreviewError, EditorSavePreviewInput, EditorSavePreviewPlan,
    plan_editor_save_preview, pre_existing_redundant_mounts_message,
};
#[cfg(test)]
use crate::tui::auth_config::env_display_map;
pub use crate::tui::components::save_preview::build_settings_save_lines;
use crate::tui::components::save_preview::{
    build_workspace_save_lines as build_confirm_save_lines, collapse_removal_lines,
};
use crate::tui::effect::WorkspaceSaveWriteMode;
use crate::tui::screens::editor::model::{EditorSaveModePlan, editor_save_mode_plan};
use crate::tui::screens::editor::view::{
    isolated_state_save_confirm_state, running_isolated_state_save_block_message,
};
use crate::tui::state::WorkspaceSaveEffect;
use crate::tui::state::{
    EditorSaveFlow, EditorState, ManagerStage, ManagerState, Modal, PendingDriftCheck,
    PendingIsolationCleanup,
};
use jackin_config::AppConfig;

#[cfg(test)]
pub fn append_env_map_diff_lines(
    out: &mut Vec<ratatui::text::Line<'static>>,
    indent: Option<&str>,
    original: &std::collections::BTreeMap<String, jackin_core::EnvValue>,
    pending: &std::collections::BTreeMap<String, jackin_core::EnvValue>,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) {
    let original = env_display_map(original);
    let pending = env_display_map(pending);
    crate::tui::components::save_preview::append_env_map_diff_lines(
        out, indent, &original, &pending, value, dim,
    );
}

/// Continue the editor save flow after an async drift check completes.
///
/// Called by the event loop when `poll_pending_drift_check` returns a result.
/// Handles the drift result identically to the synchronous path in
/// `commit_editor_save_with_runner`, then continues to the actual workspace
/// write (or shows an error / deletion-confirm modal) without blocking the
/// reactor.
pub fn continue_save_after_drift_check(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    drift_check: PendingDriftCheck,
    detection: anyhow::Result<jackin_core::DriftDetection>,
) -> anyhow::Result<Option<WorkspaceSaveEffect>> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(None);
    };

    // Clear the "Checking..." status popup; results or errors replace it.
    editor.dismiss_status_popup();

    match detection {
        Err(e) => {
            open_save_error_popup(editor, &e.to_string());
            return Ok(None);
        }
        Ok(detection) => {
            if !detection.running_containers.is_empty() {
                let msg = running_isolated_state_save_block_message(&detection.running_containers);
                open_save_error_popup(editor, &msg);
                return Ok(None);
            }
            if !detection.stopped_records.is_empty() {
                if drift_check.plan.delete_isolated_acknowledged {
                    return Ok(Some(WorkspaceSaveEffect::StartIsolationCleanup {
                        records: detection.stopped_records,
                        plan: drift_check.plan,
                        exit_on_success: drift_check.exit_on_success,
                    }));
                }
                let affected_containers: Vec<String> = detection
                    .stopped_records
                    .iter()
                    .map(|r| r.container_name.clone())
                    .collect();
                let state = isolated_state_save_confirm_state(&affected_containers);
                editor.modal = Some(Modal::Confirm {
                    target: crate::tui::state::ConfirmTarget::DeleteIsolatedAndSave {
                        plan: drift_check.plan.clone(),
                        exit_on_success: drift_check.exit_on_success,
                        affected_containers,
                    },
                    state,
                });
                editor.save_flow = EditorSaveFlow::Confirming {
                    exit_on_success: drift_check.exit_on_success,
                };
                return Ok(None);
            }
        }
    }

    // No drift detected — mark the drift gate complete before proceeding
    // so the write pass does not request the same check again.
    let mut plan = drift_check.plan;
    plan.isolated_cleanup_complete = true;
    commit_editor_save_with_runner(state, config, plan, drift_check.exit_on_success)
}

pub fn continue_save_after_isolation_cleanup(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    cleanup: PendingIsolationCleanup,
    result: anyhow::Result<()>,
) -> anyhow::Result<Option<WorkspaceSaveEffect>> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(None);
    };
    editor.dismiss_status_popup();
    if let Err(e) = result {
        open_save_error_popup(editor, &e.to_string());
        return Ok(None);
    }
    let mut plan = cleanup.plan;
    plan.isolated_cleanup_complete = true;
    commit_editor_save_with_runner(state, config, plan, cleanup.exit_on_success)
}

/// Phase 1: validate, plan, open `ConfirmSave`. Validation failures
/// route to `EditorSaveFlow::Error` and the shared `ErrorPopup`, same
/// as phase-2 commit errors. The plan is stashed on the modal so
/// commit doesn't re-run `plan_edit`/`plan_create`.
#[allow(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn begin_editor_save(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    exit_on_success: bool,
) -> anyhow::Result<()> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(());
    };
    // Clear any stale error from a prior attempt.
    editor.save_flow = EditorSaveFlow::Idle;

    let save_mode = editor_save_mode_plan(&editor.mode);

    let preview_input = match &save_mode {
        EditorSaveModePlan::Edit { original_name } => EditorSavePreviewInput::Edit {
            original_name,
            original: &editor.original,
            pending: &editor.pending,
        },
        EditorSaveModePlan::Create => EditorSavePreviewInput::Create {
            pending: &editor.pending,
            pending_name: editor.pending_name.as_deref(),
        },
    };
    let (effective_removals, final_mounts, has_collapses, collapse_lines) =
        match plan_editor_save_preview(config, preview_input) {
            Ok(EditorSavePreviewPlan::Edit {
                effective_removals,
                edit_driven_collapses,
            }) => {
                let has = !edit_driven_collapses.is_empty();
                let lines = collapse_removal_lines(&edit_driven_collapses);
                (effective_removals, None, has, lines)
            }
            Ok(EditorSavePreviewPlan::Create {
                final_mounts,
                collapsed,
            }) => {
                let has = !collapsed.is_empty();
                let lines = collapse_removal_lines(&collapsed);
                (Vec::new(), Some(final_mounts), has, lines)
            }
            Err(EditorSavePreviewError::Message(message)) => {
                open_save_error_popup(editor, &message);
                return Ok(());
            }
            Err(EditorSavePreviewError::PreExistingRedundantMounts {
                original_name,
                collapses,
            }) => {
                open_save_error_popup(
                    editor,
                    &pre_existing_redundant_mounts_message(&original_name, &collapses),
                );
                return Ok(());
            }
        };

    let lines = build_confirm_save_lines(editor, config, &collapse_lines);
    let mut confirm_state = crate::tui::components::confirm_save::ConfirmSaveState::new(lines);
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
/// `delete_isolated_acknowledged = true` so the second commit pass starts
/// the cleanup worker, then the final pass writes after cleanup completes.
#[allow(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn commit_editor_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    plan: crate::tui::state::PendingSaveCommit,
    exit_on_success: bool,
) -> anyhow::Result<Option<WorkspaceSaveEffect>> {
    commit_editor_save_with_runner(state, config, plan, exit_on_success)
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_ref_mut,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn commit_editor_save_with_runner(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    plan: crate::tui::state::PendingSaveCommit,
    exit_on_success: bool,
) -> anyhow::Result<Option<WorkspaceSaveEffect>> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return Ok(None);
    };

    let save_mode = editor_save_mode_plan(&editor.mode);

    // Operator already approved the collapsed mount set in ConfirmSave.
    // Clone so subsequent source-drift logic can still inspect the full plan.
    editor.apply_confirmed_mounts(plan.final_mounts.clone());

    // ── Source-drift safeguard ────────────────────────────────────────
    // Only meaningful in Edit mode — Create has no preserved state. Skip
    // entirely if the operator already acknowledged the modal on a
    // previous commit pass.
    if let EditorSaveModePlan::Edit { original_name } = &save_mode
        && !plan.delete_isolated_acknowledged
        && !plan.isolated_cleanup_complete
    {
        // Build prospective mounts mirroring `edit_workspace`'s merge
        // order: drop `effective_removals`, then upsert each pending
        // mount over the existing on-disk set.
        let current_ws = config.workspaces.get(original_name).cloned();
        if let Some(current_ws) = current_ws {
            let prospective_mounts = crate::services::workspace::prospective_workspace_mounts(
                &current_ws.mounts,
                &editor.pending.mounts,
                &plan.effective_removals,
            );
            return Ok(Some(WorkspaceSaveEffect::StartDriftCheck {
                original_name: original_name.clone(),
                prospective_mounts,
                plan,
                exit_on_success,
            }));
        }
    }

    // Acknowledged — clean up preserved state for each affected record
    // before the on-disk write so a partial failure leaves the system in
    // a recoverable state. Mirrors the CLI's `--delete-isolated-state`
    // branch in `app/mod.rs`.
    if let EditorSaveModePlan::Edit { original_name } = &save_mode
        && plan.delete_isolated_acknowledged
        && !plan.isolated_cleanup_complete
    {
        let current_ws = config.workspaces.get(original_name).cloned();
        if let Some(current_ws) = current_ws {
            let prospective_mounts = crate::services::workspace::prospective_workspace_mounts(
                &current_ws.mounts,
                &editor.pending.mounts,
                &plan.effective_removals,
            );
            // Re-detect outside the TUI boundary to avoid a TOCTOU window
            // where state changed between the confirm modal opening and the
            // operator's Yes.
            return Ok(Some(WorkspaceSaveEffect::StartDriftCheck {
                original_name: original_name.clone(),
                prospective_mounts,
                plan,
                exit_on_success,
            }));
        }
    }

    let service_mode = match save_mode {
        EditorSaveModePlan::Edit { original_name } => WorkspaceSaveWriteMode::Edit {
            original_name,
            pending_name: editor.pending_name.clone(),
            effective_removals: plan.effective_removals,
        },
        EditorSaveModePlan::Create => {
            let Some(name) = editor.pending_name.clone() else {
                open_save_error_popup(editor, "missing workspace name");
                return Ok(None);
            };
            WorkspaceSaveWriteMode::Create { name }
        }
    };

    Ok(Some(WorkspaceSaveEffect::WriteWorkspace {
        mode: service_mode,
        original: editor.original.clone(),
        pending: editor.pending.clone(),
        exit_on_success,
    }))
}

pub fn open_save_error_popup(editor: &mut EditorState<'_>, message: &str) {
    editor.open_error_popup(
        crate::tui::components::error_popup::save_failed_error_popup_state(message),
    );
    editor.save_flow = EditorSaveFlow::Error {
        message: message.to_owned(),
    };
}
