// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace save preview line rendering.

use super::{
    WorkspaceAuthChange, WorkspaceMountDiff, WorkspaceSaveMode, WorkspaceSavePreview,
    enabled_label, settings_env_diff_lines,
};

use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "Workspace-save preview renderer: per-section (header / mount / auth / \
              env / role / status) line builder. Inline shape preserves the \
              per-section readability."
)]
pub fn workspace_save_lines(preview: &WorkspaceSavePreview) -> Vec<Line<'static>> {
    let heading = termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong);
    let value = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::Accent)
        .fg
        .unwrap_or_default());
    let dim = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::TextMuted)
        .fg
        .unwrap_or_default());

    let mut out: Vec<Line<'static>> = Vec::new();

    match &preview.mode {
        WorkspaceSaveMode::Create { name } => {
            out.push(Line::from(vec![
                Span::styled("Create workspace: ", heading),
                Span::styled(name.clone(), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Working directory: ", heading),
                Span::styled(preview.pending_workdir.clone(), value),
            ]));

            let mounts: Vec<_> = preview
                .mount_diffs
                .iter()
                .filter_map(|diff| match diff {
                    WorkspaceMountDiff::Added(row) => Some(row.summary()),
                    WorkspaceMountDiff::Removed(_)
                    | WorkspaceMountDiff::Modified { .. }
                    | WorkspaceMountDiff::Unchanged => None,
                })
                .collect();
            if !mounts.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled(
                    format!("Mounts ({}):", mounts.len()),
                    heading,
                )));
                for mount in mounts {
                    out.push(Line::from(Span::styled(
                        format!("  \u{2022} {mount}"),
                        value,
                    )));
                }
            }

            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Allowed roles: ", heading),
                Span::styled(allowed_roles_summary(preview), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Default role: ", heading),
                Span::styled(
                    preview
                        .pending_default_role
                        .clone()
                        .unwrap_or_else(|| "(none)".into()),
                    value,
                ),
            ]));
            if preview.pending_toggles.keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Keep awake: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            if preview.pending_toggles.git_pull {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Git pull: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            let env_lines =
                settings_env_diff_lines(&preview.env_original, &preview.env_pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
            append_workspace_auth_lines(&mut out, &preview.auth_changes, heading, value, dim);
        }
        WorkspaceSaveMode::Edit {
            original_name,
            display_name,
            pending_name,
        } => {
            out.push(Line::from(vec![
                Span::styled("Edit workspace: ", heading),
                Span::styled(display_name.clone(), value),
            ]));

            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Rename:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {original_name}"),
                    dim,
                )));
                out.push(Line::from(Span::styled(format!("  + {new_name}"), value)));
            }

            if let Some(original_workdir) = &preview.original_workdir
                && original_workdir != &preview.pending_workdir
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Working directory:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {original_workdir}"),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", preview.pending_workdir),
                    value,
                )));
            }

            if preview
                .mount_diffs
                .iter()
                .any(|diff| !matches!(diff, WorkspaceMountDiff::Unchanged))
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Mounts:", heading)));
                for diff in &preview.mount_diffs {
                    match diff {
                        WorkspaceMountDiff::Added(row) => {
                            let summary = row.summary();
                            out.push(Line::from(Span::styled(format!("  + {summary}"), value)));
                        }
                        WorkspaceMountDiff::Removed(row) => {
                            let summary = row.summary();
                            out.push(Line::from(Span::styled(format!("  - {summary}"), dim)));
                        }
                        WorkspaceMountDiff::Modified { original, pending } => {
                            let original = original.summary();
                            let pending = pending.summary();
                            out.push(Line::from(Span::styled(format!("  ~ {pending}"), value)));
                            out.push(Line::from(Span::styled(
                                format!("      was: {original}"),
                                dim,
                            )));
                        }
                        WorkspaceMountDiff::Unchanged => {}
                    }
                }
            }

            let added_roles: Vec<_> = preview
                .pending_allowed_roles
                .iter()
                .filter(|role| !preview.original_allowed_roles.contains(*role))
                .collect();
            let removed_roles: Vec<_> = preview
                .original_allowed_roles
                .iter()
                .filter(|role| !preview.pending_allowed_roles.contains(*role))
                .collect();
            if !added_roles.is_empty() || !removed_roles.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Allowed roles:", heading)));
                for role in added_roles {
                    out.push(Line::from(Span::styled(format!("  + {role}"), value)));
                }
                for role in removed_roles {
                    out.push(Line::from(Span::styled(format!("  - {role}"), dim)));
                }
            }

            if preview.pending_default_role != preview.original_default_role {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Default role:", heading)));
                if let Some(old) = &preview.original_default_role {
                    out.push(Line::from(Span::styled(format!("  - {old}"), dim)));
                }
                if let Some(new) = &preview.pending_default_role {
                    out.push(Line::from(Span::styled(format!("  + {new}"), value)));
                } else {
                    out.push(Line::from(Span::styled("  + (none)", value)));
                }
            }

            if preview.pending_toggles.keep_awake != preview.original_toggles.keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Keep awake:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_toggles.keep_awake)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_toggles.keep_awake)),
                    value,
                )));
            }

            if preview.pending_toggles.git_pull != preview.original_toggles.git_pull {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Git pull:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_toggles.git_pull)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_toggles.git_pull)),
                    value,
                )));
            }

            let env_lines =
                settings_env_diff_lines(&preview.env_original, &preview.env_pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
            append_workspace_auth_lines(&mut out, &preview.auth_changes, heading, value, dim);
        }
    }

    if !preview.collapse_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Mount collapse required:",
            heading,
        )));
        out.extend(preview.collapse_lines.iter().cloned());
    }

    out
}

pub(crate) fn append_workspace_auth_lines(
    out: &mut Vec<Line<'static>>,
    changes: &[WorkspaceAuthChange],
    heading: Style,
    value: Style,
    dim: Style,
) {
    if changes.is_empty() {
        return;
    }
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("Accounts:", heading)));
    for change in changes {
        out.push(Line::from(Span::styled(
            format!("  {}", change.label),
            heading,
        )));
        out.push(Line::from(Span::styled(
            format!("    - {}", change.original),
            dim,
        )));
        out.push(Line::from(Span::styled(
            format!("    + {}", change.pending),
            value,
        )));
    }
}

pub(crate) fn allowed_roles_summary(preview: &WorkspaceSavePreview) -> String {
    if preview.pending_allowed_roles.is_empty() {
        return format!("any ({} roles)", preview.role_count);
    }
    preview.pending_allowed_roles.join(", ")
}
