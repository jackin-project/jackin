// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` editor-debug facts trait impl.

use super::super::super::EditorState;

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::debug::ConsoleEditorDebugFacts
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
where
    Modal: crate::tui::debug::ConsoleModalDebugKind,
{
    fn editor_stage_debug(&self) -> crate::tui::debug::ConsoleStageDebug {
        crate::tui::debug::ConsoleStageDebug::Editor {
            mode: format!("{:?}", self.mode),
            tab: format!("{:?}", self.active_tab),
            field: format!("{:?}", self.active_field),
            modal: self
                .modal
                .as_ref()
                .map(crate::tui::debug::ConsoleModalDebugKind::modal_debug_kind),
        }
    }
}
