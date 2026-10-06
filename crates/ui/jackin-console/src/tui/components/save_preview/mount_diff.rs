// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Mount diff, auth changes, and collapse lines.

use super::SettingsEnvPreview;
use crate::tui::auth_config::env_display_map_without_auth_credentials;
use crate::tui::screens::editor::model::{EditorMode, EditorState};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn workspace_mount_diffs_preview<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> Vec<WorkspaceMountDiff> {
    match editor.mode {
        EditorMode::Create => editor
            .pending
            .mounts
            .iter()
            .map(|mount| {
                WorkspaceMountDiff::Added(workspace_mount_preview_row(
                    mount,
                    &editor.mount_info_cache,
                ))
            })
            .collect(),
        EditorMode::Edit { .. } => {
            crate::mount_diff::classify_mount_diffs(&editor.original.mounts, &editor.pending.mounts)
                .into_iter()
                .map(|diff| match diff {
                    crate::mount_diff::MountDiff::Added(mount) => WorkspaceMountDiff::Added(
                        workspace_mount_preview_row(mount, &editor.mount_info_cache),
                    ),
                    crate::mount_diff::MountDiff::Removed(mount) => WorkspaceMountDiff::Removed(
                        workspace_mount_preview_row(mount, &editor.mount_info_cache),
                    ),
                    crate::mount_diff::MountDiff::Modified { original, pending } => {
                        WorkspaceMountDiff::Modified {
                            original: workspace_mount_preview_row(
                                original,
                                &editor.mount_info_cache,
                            ),
                            pending: workspace_mount_preview_row(pending, &editor.mount_info_cache),
                        }
                    }
                    crate::mount_diff::MountDiff::Unchanged(_) => WorkspaceMountDiff::Unchanged,
                })
                .collect()
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAuthChange {
    pub label: String,
    pub original: String,
    pub pending: String,
}

#[must_use]
pub fn workspace_auth_change(
    label_prefix: &str,
    field: &str,
    original: &str,
    pending: &str,
) -> WorkspaceAuthChange {
    WorkspaceAuthChange {
        label: format!("{label_prefix} {field}"),
        original: original.to_owned(),
        pending: pending.to_owned(),
    }
}

#[must_use]
pub fn workspace_env_preview(workspace: &jackin_config::WorkspaceConfig) -> SettingsEnvPreview {
    SettingsEnvPreview {
        env: env_display_map_without_auth_credentials(&workspace.env),
        roles: workspace
            .roles
            .iter()
            .map(|(role, config)| {
                (
                    role.clone(),
                    env_display_map_without_auth_credentials(&config.env),
                )
            })
            .collect(),
    }
}

#[must_use]
pub fn workspace_auth_changes(
    _config: &jackin_config::AppConfig,
    _workspace_name: &str,
    original: &jackin_config::WorkspaceConfig,
    pending: &jackin_config::WorkspaceConfig,
) -> Vec<WorkspaceAuthChange> {
    let mut changes = Vec::new();
    if original.accounts != pending.accounts {
        changes.push(workspace_auth_change(
            "Accounts",
            "allowed",
            &original.accounts.join(", "),
            &pending.accounts.join(", "),
        ));
    }
    push_binding_changes(
        &mut changes,
        "Default",
        &original.account_bindings,
        &pending.account_bindings,
    );
    let roles: BTreeSet<_> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::new();
    for role in roles {
        push_binding_changes(
            &mut changes,
            &format!("Role {role}"),
            original
                .roles
                .get(role)
                .map_or(&empty, |r| &r.account_bindings),
            pending
                .roles
                .get(role)
                .map_or(&empty, |r| &r.account_bindings),
        );
    }
    changes
}

pub(crate) fn push_binding_changes(
    changes: &mut Vec<WorkspaceAuthChange>,
    scope: &str,
    original: &BTreeMap<jackin_core::Agent, String>,
    pending: &BTreeMap<jackin_core::Agent, String>,
) {
    let agents: BTreeSet<_> = original.keys().chain(pending.keys()).collect();
    for agent in agents {
        if original.get(agent) != pending.get(agent) {
            changes.push(workspace_auth_change(
                scope,
                agent.slug(),
                original.get(agent).map_or("(none)", String::as_str),
                pending.get(agent).map_or("(none)", String::as_str),
            ));
        }
    }
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
pub fn workspace_mount_preview_row(
    mount: &jackin_config::MountConfig,
    cache: &crate::mount_info_cache::MountInfoCache,
) -> WorkspaceMountPreviewRow {
    WorkspaceMountPreviewRow {
        src: jackin_core::shorten_home(&mount.src),
        dst: jackin_core::shorten_home(&mount.dst),
        readonly: mount.readonly,
        isolation: mount.isolation.as_str().to_owned(),
        kind: cache.label(&mount.src),
    }
}

#[must_use]
pub fn collapse_section_lines(collapses: &[(String, String)]) -> Vec<Line<'static>> {
    let style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::TextMuted)
        .fg
        .unwrap_or_default());
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
                jackin_core::shorten_home(&removal.child.src),
                jackin_core::shorten_home(&removal.covered_by.src),
            )
        })
        .collect();
    collapse_section_lines(&display_pairs)
}
