// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` and modal traits.

use super::{
    EditorFocusTarget, EditorHorizontalScrollKeyPlan, EditorHoverTarget, EditorMode,
    EditorMountGithubOpenPlan, EditorTab, ExitIntent, FieldFocus, SecretsScopeTag,
};
use crate::tui::focus::TabFocus;
use jackin_config::WorkspaceConfig;
use std::collections::BTreeSet;
use std::marker::PhantomData;

pub trait EditorStatusPopupModal {
    fn is_status_popup(&self) -> bool;
}

pub trait EditorRoleOverridePickerModal {
    fn is_role_override_picker(&self) -> bool;
}

pub trait EditorSaveDiscardModal<SaveDiscardState> {
    fn save_discard_cancel_modal(state: SaveDiscardState) -> Self;
}

pub trait EditorErrorPopupModal<ErrorPopupState> {
    fn error_popup_modal(state: ErrorPopupState) -> Self;
}

#[derive(Debug)]
pub struct EditorState<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    /// W3C ARIA Tabs: focus is either on the tab list or exactly one content block.
    pub focus_owner: TabFocus<EditorFocusTarget>,
    pub hover_target: Option<EditorHoverTarget>,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub mount_info_cache: MountInfoCache,
    pub modal: Option<Modal>,
    pub modal_parents: Vec<Modal>,
    /// Create-mode only; Edit mode reads name from `EditorMode::Edit`.
    pub pending_name: Option<String>,
    /// Signals the outer input handler to save and/or pop to List.
    pub exit_after_save: Option<ExitIntent>,
    pub save_flow: SaveFlow,
    /// Secrets tab keys whose value is currently unmasked.
    pub unmasked_rows: BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: BTreeSet<String>,
    pub _env_value: PhantomData<fn() -> EnvValue>,
    pub workspace_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub tab_scroll: termrock::widgets::ScrollAreaState,
    pub tab_content_width: usize,
    pub tab_content_height: usize,

    pub pending_role_load: Option<PendingRoleLoad>,
    pub pending_drift_check: Option<PendingDriftCheck>,
    pub pending_isolation_cleanup: Option<PendingIsolationCleanup>,
    pub pending_op_commit: Option<PendingOpCommit>,
    pub cached_footer_h: u16,
}

impl<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    #[must_use]
    pub fn workspace_mounts_content_width(&self) -> usize {
        crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
            &self.pending.mounts,
            &self.mount_info_cache,
        )
    }

    #[must_use]
    pub fn horizontal_scroll_key_plan(&self, delta: i16) -> EditorHorizontalScrollKeyPlan {
        if self.active_tab == EditorTab::Mounts {
            return EditorHorizontalScrollKeyPlan::WorkspaceMounts {
                delta,
                content_width: self.workspace_mounts_content_width(),
            };
        }
        EditorHorizontalScrollKeyPlan::TabContent {
            delta,
            content_width: self.tab_content_width,
        }
    }

    #[must_use]
    pub fn focused_mount_github_open_plan(&self) -> EditorMountGithubOpenPlan {
        let FieldFocus::Row(n) = self.active_field;
        let Some(mount) = self.pending.mounts.get(n) else {
            return EditorMountGithubOpenPlan::NoSelection;
        };
        match self.mount_info_cache.github_web_url(&mount.src) {
            Some(web_url) => EditorMountGithubOpenPlan::Open(web_url),
            None => EditorMountGithubOpenPlan::NoGithubUrl,
        }
    }
}
