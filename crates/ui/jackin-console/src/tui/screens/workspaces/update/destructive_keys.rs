// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Destructive confirm key plans.

use super::{
    DestructiveConfirmPlan, InstancePurgeKeyPlan, SelectedInstanceActionPlan,
    SelectedInstancePurgeConfirmPlan, WorkspaceDeleteKeyPlan, WorkspaceInstanceAction,
    WorkspaceInstanceStatus,
};

use jackin_oppicker::ModalOutcome;

#[must_use]
pub const fn destructive_confirm_plan(outcome: ModalOutcome<bool>) -> DestructiveConfirmPlan {
    match outcome {
        ModalOutcome::Commit(true) => DestructiveConfirmPlan::Commit,
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => DestructiveConfirmPlan::ReturnToList,
        ModalOutcome::Continue => DestructiveConfirmPlan::Continue,
    }
}

#[must_use]
pub fn workspace_delete_key_plan(
    outcome: ModalOutcome<bool>,
    name: String,
) -> WorkspaceDeleteKeyPlan {
    match destructive_confirm_plan(outcome) {
        DestructiveConfirmPlan::Commit => WorkspaceDeleteKeyPlan::RemoveWorkspace { name },
        DestructiveConfirmPlan::ReturnToList => WorkspaceDeleteKeyPlan::ReturnToList,
        DestructiveConfirmPlan::Continue => WorkspaceDeleteKeyPlan::Continue,
    }
}

#[must_use]
pub fn instance_purge_key_plan(
    outcome: ModalOutcome<bool>,
    container: String,
) -> InstancePurgeKeyPlan {
    match destructive_confirm_plan(outcome) {
        DestructiveConfirmPlan::Commit => InstancePurgeKeyPlan::Purge { container },
        DestructiveConfirmPlan::ReturnToList => InstancePurgeKeyPlan::ReturnToList,
        DestructiveConfirmPlan::Continue => InstancePurgeKeyPlan::Continue,
    }
}

#[must_use]
pub fn selected_instance_action_plan(container: Option<String>) -> SelectedInstanceActionPlan {
    match container {
        Some(container) => SelectedInstanceActionPlan::Start { container },
        None => SelectedInstanceActionPlan::OpenError,
    }
}

#[must_use]
pub fn selected_instance_purge_confirm_plan(
    container: Option<String>,
    label_for_container: impl FnOnce(&str) -> String,
) -> SelectedInstancePurgeConfirmPlan {
    let Some(container) = container else {
        return SelectedInstancePurgeConfirmPlan::OpenError;
    };
    let label = label_for_container(&container);
    SelectedInstancePurgeConfirmPlan::OpenConfirm { container, label }
}

/// Action x status acceptance grid. Each arm enumerates the exact set
/// of statuses the action runs against. Positive matching keeps future
/// status variants from becoming accepted by accident.
#[must_use]
pub const fn instance_action_accepts_status(
    action: WorkspaceInstanceAction,
    status: WorkspaceInstanceStatus,
) -> bool {
    use WorkspaceInstanceAction as A;
    use WorkspaceInstanceStatus as S;
    match (action, status) {
        // Reconnect / Inspect: anything that still has on-disk state to read.
        (A::Reconnect | A::Inspect, status) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
        // NewSession / Shell / Stop: live container required.
        (A::NewSession | A::Shell | A::Stop, status) => match status {
            S::Active | S::Running => true,
            S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::Purged
            | S::FailedSetup => false,
        },
        // Purge: anything that has not already been purged. Crashed /
        // CleanExited / Preserved* rows have local state worth deleting
        // even though their containers are gone.
        (A::Purge, status) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
    }
}
