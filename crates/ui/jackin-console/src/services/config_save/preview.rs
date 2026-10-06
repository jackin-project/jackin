// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor save preview plans.

use super::build_workspace_edit;

use jackin_config::{AppConfig, MountConfig, Removal, WorkspaceConfig, plan_create, plan_edit};
use jackin_core::shorten_home;

#[derive(Debug)]
pub enum EditorSavePreviewInput<'a> {
    Edit {
        original_name: &'a str,
        original: &'a WorkspaceConfig,
        pending: &'a WorkspaceConfig,
    },
    Create {
        pending: &'a WorkspaceConfig,
        pending_name: Option<&'a str>,
    },
}

#[derive(Debug)]
pub enum EditorSavePreviewPlan {
    Edit {
        effective_removals: Vec<String>,
        edit_driven_collapses: Vec<Removal>,
    },
    Create {
        final_mounts: Vec<MountConfig>,
        collapsed: Vec<Removal>,
    },
}

#[derive(Debug)]
pub enum EditorSavePreviewError {
    Message(String),
    PreExistingRedundantMounts {
        original_name: String,
        collapses: Vec<Removal>,
    },
}

#[must_use]
pub fn pre_existing_redundant_mounts_message(original_name: &str, collapses: &[Removal]) -> String {
    let details: Vec<String> = collapses
        .iter()
        .map(|r| {
            format!(
                "{} covered by {}",
                shorten_home(&r.child.src),
                shorten_home(&r.covered_by.src),
            )
        })
        .collect();
    format!(
        "pre-existing redundant mount(s) in this workspace: {}; \
         run `jackin❯ workspace prune {original_name}` to clean up",
        details.join(", "),
    )
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn plan_editor_save_preview(
    config: &AppConfig,
    input: EditorSavePreviewInput<'_>,
) -> Result<EditorSavePreviewPlan, EditorSavePreviewError> {
    match input {
        EditorSavePreviewInput::Edit {
            original_name,
            original,
            pending,
        } => {
            let current_ws = config
                .workspaces
                .get(original_name)
                .cloned()
                .ok_or_else(|| {
                    EditorSavePreviewError::Message(format!(
                        "workspace {original_name:?} no longer exists in config"
                    ))
                })?;
            let edit_delta = build_workspace_edit(original, pending);
            let plan = plan_edit(
                &current_ws,
                &edit_delta.upsert_mounts,
                &edit_delta.remove_destinations,
                false,
            )
            .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            if plan.edit_driven_collapses.is_empty() && !plan.pre_existing_collapses.is_empty() {
                return Err(EditorSavePreviewError::PreExistingRedundantMounts {
                    original_name: original_name.to_owned(),
                    collapses: plan.pre_existing_collapses,
                });
            }
            Ok(EditorSavePreviewPlan::Edit {
                effective_removals: plan.effective_removals,
                edit_driven_collapses: plan.edit_driven_collapses,
            })
        }
        EditorSavePreviewInput::Create {
            pending,
            pending_name,
        } => {
            if pending_name.is_none() {
                return Err(EditorSavePreviewError::Message(
                    "missing workspace name".to_owned(),
                ));
            }
            let plan = plan_create(&pending.mounts)
                .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            Ok(EditorSavePreviewPlan::Create {
                final_mounts: plan.final_mounts,
                collapsed: plan.collapsed,
            })
        }
    }
}
