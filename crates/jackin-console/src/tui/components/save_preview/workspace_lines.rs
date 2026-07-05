use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::settings_lines::{enabled_label, settings_env_diff_lines};
use super::workspace::{
    WorkspaceAuthChange, WorkspaceMountDiff, WorkspaceSaveMode, WorkspaceSavePreview,
};

#[must_use]
pub fn workspace_save_lines(preview: &WorkspaceSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let value = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);

    match &preview.mode {
        WorkspaceSaveMode::Create { name } => workspace_create_save_lines(
            preview,
            name,
            WorkspaceStyles {
                heading,
                value,
                dim,
            },
        ),
        WorkspaceSaveMode::Edit {
            original_name,
            display_name,
            pending_name,
        } => workspace_edit_save_lines(
            preview,
            original_name,
            display_name,
            pending_name.as_deref(),
            WorkspaceStyles {
                heading,
                value,
                dim,
            },
        ),
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkspaceStyles {
    heading: Style,
    value: Style,
    dim: Style,
}

fn workspace_create_save_lines(
    preview: &WorkspaceSavePreview,
    name: &str,
    styles: WorkspaceStyles,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(Line::from(vec![
        Span::styled("Create workspace: ", styles.heading),
        Span::styled(name.to_owned(), styles.value),
    ]));
    out.push(Line::raw(""));
    out.push(Line::from(vec![
        Span::styled("Working directory: ", styles.heading),
        Span::styled(preview.pending_workdir.clone(), styles.value),
    ]));

    append_workspace_create_mounts(&mut out, preview, styles);
    append_workspace_create_options(&mut out, preview, styles);
    append_workspace_env_and_auth(&mut out, preview, styles);
    append_workspace_collapse_lines(&mut out, preview, styles.heading);
    out
}

fn workspace_edit_save_lines(
    preview: &WorkspaceSavePreview,
    original_name: &str,
    display_name: &str,
    pending_name: Option<&str>,
    styles: WorkspaceStyles,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(Line::from(vec![
        Span::styled("Edit workspace: ", styles.heading),
        Span::styled(display_name.to_owned(), styles.value),
    ]));

    append_workspace_name_change(&mut out, original_name, pending_name, styles);
    append_workspace_workdir_change(&mut out, preview, styles);
    append_workspace_mount_changes(&mut out, preview, styles);
    append_workspace_role_changes(&mut out, preview, styles);
    append_workspace_toggle_change(
        &mut out,
        "Keep awake",
        preview.original_toggles.keep_awake,
        preview.pending_toggles.keep_awake,
        styles,
    );
    append_workspace_toggle_change(
        &mut out,
        "Git pull",
        preview.original_toggles.git_pull,
        preview.pending_toggles.git_pull,
        styles,
    );
    append_workspace_env_and_auth(&mut out, preview, styles);
    append_workspace_collapse_lines(&mut out, preview, styles.heading);
    out
}

fn append_workspace_create_mounts(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
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
            styles.heading,
        )));
        for mount in mounts {
            out.push(Line::from(Span::styled(
                format!("  \u{2022} {mount}"),
                styles.value,
            )));
        }
    }
}

fn append_workspace_create_options(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
    out.push(Line::raw(""));
    out.push(Line::from(vec![
        Span::styled("Allowed roles: ", styles.heading),
        Span::styled(allowed_roles_summary(preview), styles.value),
    ]));
    out.push(Line::raw(""));
    out.push(Line::from(vec![
        Span::styled("Default role: ", styles.heading),
        Span::styled(
            preview
                .pending_default_role
                .clone()
                .unwrap_or_else(|| "(none)".into()),
            styles.value,
        ),
    ]));
    if preview.pending_toggles.keep_awake {
        out.push(Line::raw(""));
        out.push(Line::from(vec![
            Span::styled("Keep awake: ", styles.heading),
            Span::styled("enabled", styles.value),
        ]));
    }
    if preview.pending_toggles.git_pull {
        out.push(Line::raw(""));
        out.push(Line::from(vec![
            Span::styled("Git pull: ", styles.heading),
            Span::styled("enabled", styles.value),
        ]));
    }
}

fn append_workspace_name_change(
    out: &mut Vec<Line<'static>>,
    original_name: &str,
    pending_name: Option<&str>,
    styles: WorkspaceStyles,
) {
    if let Some(new_name) = pending_name
        && new_name != original_name
    {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled("Rename:", styles.heading)));
        out.push(Line::from(Span::styled(
            format!("  - {original_name}"),
            styles.dim,
        )));
        out.push(Line::from(Span::styled(
            format!("  + {new_name}"),
            styles.value,
        )));
    }
}

fn append_workspace_workdir_change(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
    if let Some(original_workdir) = &preview.original_workdir
        && original_workdir != &preview.pending_workdir
    {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Working directory:",
            styles.heading,
        )));
        out.push(Line::from(Span::styled(
            format!("  - {original_workdir}"),
            styles.dim,
        )));
        out.push(Line::from(Span::styled(
            format!("  + {}", preview.pending_workdir),
            styles.value,
        )));
    }
}

fn append_workspace_mount_changes(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
    if !preview
        .mount_diffs
        .iter()
        .any(|diff| !matches!(diff, WorkspaceMountDiff::Unchanged))
    {
        return;
    }
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("Mounts:", styles.heading)));
    for diff in &preview.mount_diffs {
        match diff {
            WorkspaceMountDiff::Added(row) => {
                out.push(Line::from(Span::styled(
                    format!("  + {}", row.summary()),
                    styles.value,
                )));
            }
            WorkspaceMountDiff::Removed(row) => {
                out.push(Line::from(Span::styled(
                    format!("  - {}", row.summary()),
                    styles.dim,
                )));
            }
            WorkspaceMountDiff::Modified { original, pending } => {
                out.push(Line::from(Span::styled(
                    format!("  ~ {}", pending.summary()),
                    styles.value,
                )));
                out.push(Line::from(Span::styled(
                    format!("      was: {}", original.summary()),
                    styles.dim,
                )));
            }
            WorkspaceMountDiff::Unchanged => {}
        }
    }
}

fn append_workspace_role_changes(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
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
        out.push(Line::from(Span::styled("Allowed roles:", styles.heading)));
        for role in added_roles {
            out.push(Line::from(Span::styled(
                format!("  + {role}"),
                styles.value,
            )));
        }
        for role in removed_roles {
            out.push(Line::from(Span::styled(format!("  - {role}"), styles.dim)));
        }
    }

    if preview.pending_default_role != preview.original_default_role {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled("Default role:", styles.heading)));
        if let Some(old) = &preview.original_default_role {
            out.push(Line::from(Span::styled(format!("  - {old}"), styles.dim)));
        }
        if let Some(new) = &preview.pending_default_role {
            out.push(Line::from(Span::styled(format!("  + {new}"), styles.value)));
        } else {
            out.push(Line::from(Span::styled("  + (none)", styles.value)));
        }
    }
}

fn append_workspace_toggle_change(
    out: &mut Vec<Line<'static>>,
    label: &str,
    original: bool,
    pending: bool,
    styles: WorkspaceStyles,
) {
    if pending == original {
        return;
    }
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled(
        format!("{label}:"),
        styles.heading,
    )));
    out.push(Line::from(Span::styled(
        format!("  - {}", enabled_label(original)),
        styles.dim,
    )));
    out.push(Line::from(Span::styled(
        format!("  + {}", enabled_label(pending)),
        styles.value,
    )));
}

fn append_workspace_env_and_auth(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    styles: WorkspaceStyles,
) {
    let env_lines = settings_env_diff_lines(
        &preview.env_original,
        &preview.env_pending,
        styles.value,
        styles.dim,
    );
    if !env_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled("Env vars:", styles.heading)));
        out.extend(env_lines);
    }
    append_workspace_auth_lines(
        out,
        &preview.auth_changes,
        styles.heading,
        styles.value,
        styles.dim,
    );
}

fn append_workspace_collapse_lines(
    out: &mut Vec<Line<'static>>,
    preview: &WorkspaceSavePreview,
    heading: Style,
) {
    if !preview.collapse_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Mount collapse required:",
            heading,
        )));
        out.extend(preview.collapse_lines.iter().cloned());
    }
}

fn append_workspace_auth_lines(
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
    out.push(Line::from(Span::styled("Auth:", heading)));
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

fn allowed_roles_summary(preview: &WorkspaceSavePreview) -> String {
    if preview.pending_allowed_roles.is_empty() {
        return format!("any ({} roles)", preview.role_count);
    }
    preview.pending_allowed_roles.join(", ")
}

#[must_use]
pub fn collapse_section_lines(collapses: &[(String, String)]) -> Vec<Line<'static>> {
    let style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    collapses
        .iter()
        .map(|(child, parent)| {
            Line::from(Span::styled(
                format!("  {child} will be subsumed under {parent}"),
                style,
            ))
        })
        .collect()
}

#[must_use]
pub fn collapse_removal_lines(collapses: &[jackin_config::Removal]) -> Vec<Line<'static>> {
    let display_pairs: Vec<_> = collapses
        .iter()
        .map(|removal| {
            (
                jackin_tui::shorten_home(&removal.child.src),
                jackin_tui::shorten_home(&removal.covered_by.src),
            )
        })
        .collect();
    collapse_section_lines(&display_pairs)
}
