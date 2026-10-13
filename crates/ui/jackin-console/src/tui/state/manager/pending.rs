// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` pending subscription polls.

use crate::tui::runtime::SubscriptionPoll;

use super::super::{
    ManagerInstanceRefreshSnapshot, ManagerState, PendingDriftCheck, PendingIsolationCleanup,
    PendingRoleLoad,
};

impl ManagerState<'_> {
    /// Poll the in-flight drift check started by a save operation.
    ///
    /// Returns `Some(check)` when the check has a result ready, taking
    /// ownership of the `PendingDriftCheck` so the caller can continue the
    /// save flow. Returns `None` when the check is still running or there is
    /// no pending check.
    pub fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        PendingDriftCheck,
        anyhow::Result<jackin_core::DriftDetection>,
    )> {
        self.stage.poll_pending_drift_check()
    }

    pub fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(PendingIsolationCleanup, anyhow::Result<()>)> {
        self.stage.poll_pending_isolation_cleanup()
    }

    pub fn poll_pending_role_load(&mut self) -> Option<(PendingRoleLoad, anyhow::Result<()>)> {
        self.stage.poll_pending_role_load()
    }

    pub fn poll_pending_op_commit(
        &mut self,
    ) -> Option<(jackin_core::OpRef, anyhow::Result<()>, bool)> {
        self.stage.poll_pending_op_commit().map(|resolution| {
            (
                resolution.op_ref,
                resolution.result,
                matches!(
                    resolution.origin,
                    crate::tui::model::ConsolePendingOpCommitOrigin::Settings
                ),
            )
        })
    }

    pub(crate) fn drain_instance_refresh(
        &mut self,
    ) -> Option<Result<ManagerInstanceRefreshSnapshot, String>> {
        let rx = self.instances_refresh_rx.as_mut()?;
        match rx.poll_next() {
            SubscriptionPoll::Ready((generation, result)) => {
                self.instances_refresh_rx = None;
                if generation == self.instances_refresh_generation {
                    Some(result)
                } else {
                    None
                }
            }
            SubscriptionPoll::Pending => {
                // Worker still running — keep the receiver.
                None
            }
            SubscriptionPoll::Closed => {
                self.instances_refresh_rx = None;
                let message =
                    crate::tui::subscriptions::instance_refresh_worker_disconnected_message();
                Some(Err(message.into()))
            }
        }
    }

    pub fn apply_instance_refresh(
        &mut self,
        result: Result<ManagerInstanceRefreshSnapshot, String>,
    ) {
        match result {
            Ok(snapshot) => self.apply_instance_refresh_snapshot(snapshot),
            Err(error) => self.apply_instance_refresh_error(&error),
        }
    }
}
