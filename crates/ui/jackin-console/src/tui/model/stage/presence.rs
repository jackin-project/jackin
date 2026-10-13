// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Modal-presence and pending-commit traits.

use super::ConsoleStageModalFacts;

pub trait ConsoleEditorModalPresence {
    fn editor_modal_open(&self) -> bool;
}

pub trait ConsoleEditorFooterHeight {
    fn editor_cached_footer_height(&self) -> u16;
}

pub trait ConsoleSettingsModalPresence {
    fn settings_modal_facts(&self) -> ConsoleStageModalFacts;
}

pub trait ConsoleSettingsFooterHeight {
    fn settings_cached_footer_height(&self) -> u16;
}

pub trait ConsolePendingRoleLoad {
    type PendingRoleLoad;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)>;
}

pub trait ConsolePendingDriftCheck {
    type PendingDriftCheck;
    type DriftDetection;

    fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Self::PendingDriftCheck,
        anyhow::Result<Self::DriftDetection>,
    )>;
}

pub trait ConsolePendingIsolationCleanup {
    type PendingIsolationCleanup;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)>;
}

pub trait ConsolePendingOpCommit {
    type OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePendingOpCommitOrigin {
    Editor,
    Settings,
}

#[derive(Debug)]
pub struct ConsolePendingOpCommitResolution<OpRef> {
    pub op_ref: OpRef,
    pub result: anyhow::Result<()>,
    pub origin: ConsolePendingOpCommitOrigin,
}

pub trait ConsoleAnimationTick {
    fn tick_active_animation(&mut self) -> bool;
}

impl<T> ConsoleAnimationTick for Box<T>
where
    T: ConsoleAnimationTick + ?Sized,
{
    fn tick_active_animation(&mut self) -> bool {
        self.as_mut().tick_active_animation()
    }
}

pub trait ConsoleCreatePreludeModalPresence {
    fn create_prelude_modal_open(&self) -> bool;
}

pub trait ConsoleManagerModalBlockPresence {
    fn list_modal_open(&self) -> bool;
    fn editor_modal_open(&self) -> bool;
}
