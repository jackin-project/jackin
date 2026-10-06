// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` workdir and mount-destination inputs.

use super::super::super::{EditorMode, EditorState};
use std::collections::{BTreeMap, BTreeSet};

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    pub fn commit_workdir_input(&mut self, workdir: impl Into<String>) {
        self.pending.workdir = workdir.into();
        self.clear_modal_chain();
    }

    pub fn commit_last_mount_dst_input(&mut self, dst: impl Into<String>) {
        if let Some(last) = self.pending.mounts.last_mut() {
            last.dst = dst.into();
        }
        self.clear_modal_chain();
    }

    pub fn apply_confirmed_mounts(
        &mut self,
        final_mounts: Option<Vec<jackin_config::MountConfig>>,
    ) {
        if let Some(final_mounts) = final_mounts {
            self.pending.mounts = final_mounts;
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        if self.pending != self.original {
            return true;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|n| n != name)
        {
            return true;
        }
        false
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        let mut n = 0;
        if self.pending.workdir != self.original.workdir {
            n += 1;
        }
        if self.pending.default_role != self.original.default_role {
            n += 1;
        }
        if self.pending.allowed_roles != self.original.allowed_roles {
            n += 1;
        }
        if self.pending.keep_awake != self.original.keep_awake {
            n += 1;
        }
        if self.pending.git_pull_on_entry != self.original.git_pull_on_entry {
            n += 1;
        }
        if self.pending.github != self.original.github {
            n += 1;
        }
        if self.pending.accounts != self.original.accounts {
            n += 1;
        }
        if self.pending.account_bindings != self.original.account_bindings {
            n += 1;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|pn| pn != name)
        {
            n += 1;
        }
        n += crate::mount_diff::classify_mount_diffs(&self.original.mounts, &self.pending.mounts)
            .iter()
            .filter(|d| !matches!(d, crate::mount_diff::MountDiff::Unchanged(_)))
            .count();
        n += crate::tui::screens::settings::update::settings_map_change_count(
            &self.original.env,
            &self.pending.env,
        );

        let role_keys: BTreeSet<&String> = self
            .original
            .roles
            .keys()
            .chain(self.pending.roles.keys())
            .collect();
        for role in role_keys {
            let orig = self.original.roles.get(role);
            let pend = self.pending.roles.get(role);
            let empty = BTreeMap::<String, jackin_config::EnvValue>::new();
            let orig_env = orig.map_or(&empty, |o| &o.env);
            let pend_env = pend.map_or(&empty, |p| &p.env);
            n += crate::tui::screens::settings::update::settings_map_change_count(
                orig_env, pend_env,
            );
            if orig.and_then(|o| o.github.as_ref()) != pend.and_then(|p| p.github.as_ref()) {
                n += 1;
            }
            let empty_bindings = BTreeMap::new();
            if orig.map_or(&empty_bindings, |o| &o.account_bindings)
                != pend.map_or(&empty_bindings, |p| &p.account_bindings)
            {
                n += 1;
            }
        }
        n
    }
}
