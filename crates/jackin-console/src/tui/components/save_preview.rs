//! Save-confirm preview line builders for console-local dialogs.

use std::collections::{BTreeMap, BTreeSet};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSavePreview {
    pub mode: WorkspaceSaveMode,
    pub original_workdir: Option<String>,
    pub pending_workdir: String,
    pub mount_diffs: Vec<WorkspaceMountDiff>,
    pub original_allowed_roles: Vec<String>,
    pub pending_allowed_roles: Vec<String>,
    pub role_count: usize,
    pub original_default_role: Option<String>,
    pub pending_default_role: Option<String>,
    pub original_keep_awake: bool,
    pub pending_keep_awake: bool,
    pub original_git_pull: bool,
    pub pending_git_pull: bool,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub collapse_lines: Vec<Line<'static>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSaveMode {
    Create {
        name: String,
    },
    Edit {
        original_name: String,
        display_name: String,
        pending_name: Option<String>,
    },
}

#[must_use]
pub fn workspace_create_display_name(pending_name: Option<&str>) -> String {
    pending_name.unwrap_or("(unnamed)").to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceMountDiff {
    Added(WorkspaceMountPreviewRow),
    Removed(WorkspaceMountPreviewRow),
    Modified {
        original: WorkspaceMountPreviewRow,
        pending: WorkspaceMountPreviewRow,
    },
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMountPreviewRow {
    pub src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: String,
    pub kind: String,
}

impl WorkspaceMountPreviewRow {
    #[must_use]
    pub fn summary(&self) -> String {
        let mode = if self.readonly { "ro" } else { "rw" };
        let host = if self.src == self.dst {
            String::new()
        } else {
            format!("  host: {}", self.src)
        };
        format!(
            "{}{host}  ({mode}, {}, {})",
            self.dst, self.isolation, self.kind
        )
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSavePreview {
    pub general: SettingsGeneralPreview,
    pub mounts_original: Vec<MountPreviewRow>,
    pub mounts_pending: Vec<MountPreviewRow>,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub auth_original: Vec<AuthPreviewRow>,
    pub auth_pending: Vec<AuthPreviewRow>,
    pub auth_github_env_original: BTreeMap<String, String>,
    pub auth_github_env_pending: BTreeMap<String, String>,
    pub trust_original: Vec<TrustPreviewRow>,
    pub trust_pending: Vec<TrustPreviewRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralPreview {
    pub original_coauthor_trailer: bool,
    pub pending_coauthor_trailer: bool,
    pub original_dco: bool,
    pub pending_dco: bool,
}

impl SettingsGeneralPreview {
    fn change_count(self) -> usize {
        usize::from(self.original_coauthor_trailer != self.pending_coauthor_trailer)
            + usize::from(self.original_dco != self.pending_dco)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPreviewRow {
    pub scope: Option<String>,
    pub name: String,
    pub src: String,
    pub dst: String,
    pub readonly: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsEnvPreview {
    pub env: BTreeMap<String, String>,
    pub roles: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreviewRow {
    pub label: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPreviewRow {
    pub role: String,
    pub trusted: bool,
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn workspace_save_lines(preview: &WorkspaceSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let value = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);

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
            if preview.pending_keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Keep awake: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            if preview.pending_git_pull {
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

            if preview.pending_keep_awake != preview.original_keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Keep awake:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_keep_awake)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_keep_awake)),
                    value,
                )));
            }

            if preview.pending_git_pull != preview.original_git_pull {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Git pull:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_git_pull)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_git_pull)),
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

fn allowed_roles_summary(preview: &WorkspaceSavePreview) -> String {
    if preview.pending_allowed_roles.is_empty() {
        return format!("any ({} roles)", preview.role_count);
    }
    preview.pending_allowed_roles.join(", ")
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn settings_save_lines(preview: &SettingsSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let add_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let remove_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let sep_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DARK);

    let mut out: Vec<Line<'static>> = Vec::new();

    out.push(Line::from(Span::styled("Save settings", heading)));
    out.push(Line::raw(""));

    let general_stats = settings_general_stats(preview.general);
    let mount_stats = settings_mount_stats(&preview.mounts_original, &preview.mounts_pending);
    let env_stats = settings_env_stats(&preview.env_original, &preview.env_pending);
    let auth_stats = settings_auth_stats(
        &preview.auth_original,
        &preview.auth_pending,
        &preview.auth_github_env_original,
        &preview.auth_github_env_pending,
    );
    let trust_stats = settings_trust_stats(&preview.trust_original, &preview.trust_pending);

    if let Some(s) = general_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  General:      ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = mount_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Mounts:       ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = env_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Environments: ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = auth_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Auth:         ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = trust_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Trust:        ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }

    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("  \u{2500}".repeat(30), sep_style)));
    out.push(Line::raw(""));

    if general_stats.is_some() {
        out.push(Line::from(Span::styled("General:", heading)));
        let arrow = "\u{2192}";

        if preview.general.pending_coauthor_trailer != preview.general.original_coauthor_trailer {
            let from = enabled_label(preview.general.original_coauthor_trailer);
            let to = enabled_label(preview.general.pending_coauthor_trailer);
            out.push(Line::from(vec![
                Span::styled("  co-author trailer: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        if preview.general.pending_dco != preview.general.original_dco {
            let from = enabled_label(preview.general.original_dco);
            let to = enabled_label(preview.general.pending_dco);
            out.push(Line::from(vec![
                Span::styled("  dco: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        out.push(Line::raw(""));
    }

    let mount_lines = settings_mount_diff_lines(
        &preview.mounts_original,
        &preview.mounts_pending,
        add_style,
        remove_style,
    );
    if !mount_lines.is_empty() {
        out.push(Line::from(Span::styled("Mounts:", heading)));
        out.extend(mount_lines);
        out.push(Line::raw(""));
    }

    let env_lines = settings_env_diff_lines(
        &preview.env_original,
        &preview.env_pending,
        add_style,
        remove_style,
    );
    if !env_lines.is_empty() {
        out.push(Line::from(Span::styled("Environments:", heading)));
        out.extend(env_lines);
        out.push(Line::raw(""));
    }

    let auth_lines = settings_auth_diff_lines(
        &preview.auth_original,
        &preview.auth_pending,
        &preview.auth_github_env_original,
        &preview.auth_github_env_pending,
        add_style,
        remove_style,
    );
    if !auth_lines.is_empty() {
        out.push(Line::from(Span::styled("Auth:", heading)));
        out.extend(auth_lines);
        out.push(Line::raw(""));
    }

    let trust_lines = settings_trust_diff_lines(
        &preview.trust_original,
        &preview.trust_pending,
        add_style,
        remove_style,
    );
    if !trust_lines.is_empty() {
        out.push(Line::from(Span::styled("Trust:", heading)));
        out.extend(trust_lines);
        out.push(Line::raw(""));
    }

    while out
        .last()
        .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        out.pop();
    }

    out
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn settings_general_stats(state: SettingsGeneralPreview) -> Option<String> {
    let count = state.change_count();
    if count == 0 {
        return None;
    }
    Some(if count == 1 {
        "1 change".to_string()
    } else {
        format!("{count} changes")
    })
}

fn settings_mount_stats(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> Option<String> {
    let (added, removed, modified) = mount_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

fn settings_env_stats(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> Option<String> {
    let (added, removed, modified) = env_config_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

fn summarize_diff_counts(added: usize, removed: usize, modified: usize) -> Option<String> {
    if added + removed + modified == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if added > 0 {
        parts.push(format!("{added} added"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    Some(parts.join(", "))
}

fn settings_auth_stats(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
) -> Option<String> {
    let row_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.mode != b.mode)
        .count();
    let (env_added, env_removed, env_modified) =
        env_map_diff_counts(orig_github_env, pend_github_env);
    let total = row_changes + env_added + env_removed + env_modified;
    if total == 0 {
        return None;
    }
    Some(format!("{total} changed"))
}

fn settings_trust_stats(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
) -> Option<String> {
    let changed = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.trusted != b.trusted)
        .count();
    if changed == 0 {
        return None;
    }
    Some(format!("{changed} changed"))
}

fn mount_diff_counts(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> (usize, usize, usize) {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);
    let added = pend_map
        .keys()
        .filter(|k| !orig_map.contains_key(*k))
        .count();
    let removed = orig_map
        .keys()
        .filter(|k| !pend_map.contains_key(*k))
        .count();
    let modified = pend_map
        .iter()
        .filter(|(k, prow)| orig_map.get(*k).is_some_and(|orow| orow != *prow))
        .count();
    (added, removed, modified)
}

fn env_config_diff_counts(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> (usize, usize, usize) {
    let (ga, gr, gm) = env_map_diff_counts(&original.env, &pending.env);
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    let (ra, rr, rm) = all_roles.into_iter().fold((0, 0, 0), |(a, r, m), role| {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let (da, dr, dm) = env_map_diff_counts(oe, pe);
        (a + da, r + dr, m + dm)
    });
    (ga + ra, gr + rr, gm + rm)
}

fn env_map_diff_counts(
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
) -> (usize, usize, usize) {
    let added = pending
        .keys()
        .filter(|k| !original.contains_key(*k))
        .count();
    let removed = original
        .keys()
        .filter(|k| !pending.contains_key(*k))
        .count();
    let modified = pending
        .iter()
        .filter(|(k, v)| original.get(*k).is_some_and(|ov| ov != *v))
        .count();
    (added, removed, modified)
}

fn settings_mount_diff_lines(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);

    let mut out: Vec<Line<'static>> = Vec::new();
    for (key, row) in &pend_map {
        if !orig_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  + {}", mount_row_summary(row)),
                add_style,
            )));
        }
    }
    for (key, row) in &orig_map {
        if !pend_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  - {}", mount_row_summary(row)),
                remove_style,
            )));
        }
    }
    for (key, prow) in &pend_map {
        if let Some(orow) = orig_map.get(key)
            && orow != prow
        {
            out.push(Line::from(Span::styled(
                format!("  ~ {}", mount_row_summary(prow)),
                add_style,
            )));
            out.push(Line::from(Span::styled(
                format!("      was: {}", mount_row_summary(orow)),
                remove_style,
            )));
        }
    }
    out
}

fn mount_map(rows: &[MountPreviewRow]) -> BTreeMap<(Option<String>, String), &MountPreviewRow> {
    rows.iter()
        .map(|row| ((row.scope.clone(), row.name.clone()), row))
        .collect()
}

fn mount_row_summary(row: &MountPreviewRow) -> String {
    let scope = row
        .scope
        .as_deref()
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    let ro = if row.readonly { " (ro)" } else { "" };
    format!("{scope}{} \u{2192} {}{ro}", row.src, row.dst)
}

fn settings_env_diff_lines(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    append_env_map_diff_lines(
        &mut out,
        None,
        &original.env,
        &pending.env,
        add_style,
        remove_style,
    );
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    for role in all_roles {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let mut probe: Vec<Line<'static>> = Vec::new();
        append_env_map_diff_lines(&mut probe, None, oe, pe, add_style, remove_style);
        if !probe.is_empty() {
            out.push(Line::from(Span::styled(
                format!("  role {role}:"),
                add_style,
            )));
            append_env_map_diff_lines(&mut out, Some("  "), oe, pe, add_style, remove_style);
        }
    }
    out
}

pub fn append_env_map_diff_lines(
    out: &mut Vec<Line<'static>>,
    indent: Option<&str>,
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
    value: Style,
    dim: Style,
) {
    let prefix = indent.unwrap_or("");
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {v}"),
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

fn settings_auth_diff_lines(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.mode != pend_row.mode {
            out.push(Line::from(Span::styled(
                format!(
                    "  ~ {}  {} \u{2192} {}",
                    pend_row.label, orig_row.mode, pend_row.mode
                ),
                add_style,
            )));
        }
    }
    append_env_map_diff_lines(
        &mut out,
        None,
        orig_github_env,
        pend_github_env,
        add_style,
        remove_style,
    );
    out
}

fn settings_trust_diff_lines(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.trusted != pend_row.trusted {
            let (label, style) = if pend_row.trusted {
                (format!("  + {}  trusted", pend_row.role), add_style)
            } else {
                (format!("  - {}  untrusted", pend_row.role), remove_style)
            };
            out.push(Line::from(Span::styled(label, style)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::workspace_create_display_name;

    #[test]
    fn workspace_create_display_name_uses_pending_or_visible_fallback() {
        assert_eq!(workspace_create_display_name(Some("demo")), "demo");
        assert_eq!(workspace_create_display_name(None), "(unnamed)");
    }
}
