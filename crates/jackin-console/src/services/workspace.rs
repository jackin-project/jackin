// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-owned workspace and mount helpers.

use jackin_config::{
    AppConfig, GlobalMountRow, MountConfig, MountEntry, MountIsolation, find_sensitive_mounts,
};
use jackin_core::RoleSelector;

#[must_use]
pub fn current_dir_mount_config(cwd_str: &str) -> MountConfig {
    shared_mount_config(cwd_str, cwd_str, false)
}

#[must_use]
pub fn shared_mount_config(
    src: impl Into<String>,
    dst: impl Into<String>,
    readonly: bool,
) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly,
        isolation: MountIsolation::Shared,
    }
}

/// Mirror the merge order `AppConfig::edit_workspace` uses to build the
/// post-edit mount list, so source-drift checks evaluate the same shape that
/// will land on disk.
#[must_use]
pub fn prospective_workspace_mounts(
    current: &[MountConfig],
    pending: &[MountConfig],
    effective_removals: &[String],
) -> Vec<MountConfig> {
    let mut out: Vec<MountConfig> = current
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

pub fn global_rows_have_sensitive_mount(rows: &[GlobalMountRow]) -> bool {
    let mounts = rows
        .iter()
        .map(|row| row.mount.clone())
        .collect::<Vec<MountConfig>>();
    !find_sensitive_mounts(&mounts).is_empty()
}

#[must_use]
pub fn split_global_mount_rows(
    rows: &[GlobalMountRow],
) -> (Vec<&GlobalMountRow>, Vec<&GlobalMountRow>) {
    rows.iter().partition(|row| row.scope.is_none())
}

#[must_use]
pub fn global_rows_for_picker(
    config: &AppConfig,
    picker_role: Option<&RoleSelector>,
) -> Vec<GlobalMountRow> {
    picker_role.map_or_else(
        || {
            config
                .list_mount_rows()
                .into_iter()
                .filter(|row| row.scope.is_none())
                .collect()
        },
        |role| config.resolve_mount_rows(role),
    )
}

/// Extract unscoped global Docker mounts for launch-time console choices.
pub fn unscoped_global_mounts(config: &AppConfig) -> anyhow::Result<Vec<MountConfig>> {
    let mounts = config
        .docker
        .mounts
        .iter()
        .filter_map(|(name, entry)| match entry {
            MountEntry::Mount(mount) => Some((name.clone(), MountConfig::from(mount.clone()))),
            MountEntry::Scoped(_) => None,
        })
        .collect::<Vec<_>>();

    AppConfig::expand_and_validate_named_mounts(&mounts)
}

#[must_use]
pub fn global_mount_scope_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[must_use]
pub fn unique_global_mount_name(rows: &[GlobalMountRow], scope: Option<&str>, dst: &str) -> String {
    let basename = std::path::Path::new(dst)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("mount");
    let base = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    let base = if base.is_empty() {
        "mount".to_owned()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while rows
        .iter()
        .any(|row| row.scope.as_deref() == scope && row.name == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

#[cfg(test)]
mod tests;
