//! Long-lived event sources that drive the host console TUI loop.

use jackin_tui::runtime::BlockingSubscription;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subscription {
    /// Keyboard, mouse, resize, focus, and paste events from crossterm.
    TerminalEvents,
    /// Periodic redraw tick for spinners and animated picker surfaces.
    AnimationTick,
}

pub const CONSOLE_SUBSCRIPTIONS: &[Subscription] =
    &[Subscription::TerminalEvents, Subscription::AnimationTick];

pub fn drift_check_worker_disconnected_message() -> &'static str {
    "drift check worker disconnected"
}

pub fn isolation_cleanup_worker_disconnected_message() -> &'static str {
    "isolation cleanup worker disconnected"
}

pub fn role_loader_worker_disconnected_message() -> &'static str {
    "role loader worker disconnected"
}

pub fn op_read_worker_disconnected_message() -> &'static str {
    "op read worker disconnected"
}

pub fn instance_refresh_worker_disconnected_message() -> &'static str {
    "instance refresh worker disconnected"
}

/// In-flight 1Password read triggered by an op picker commit from an auth form.
pub struct PendingOpCommit<OpRef> {
    /// The op reference preserved so completion can commit it into the form.
    pub op_ref: OpRef,
    /// Oneshot receiver for the `spawn_blocking` result.
    pub rx: BlockingSubscription<anyhow::Result<()>>,
}

impl<OpRef> PendingOpCommit<OpRef> {
    #[must_use]
    pub fn new(op_ref: OpRef, rx: BlockingSubscription<anyhow::Result<()>>) -> Self {
        Self { op_ref, rx }
    }
}

impl<OpRef: std::fmt::Debug> std::fmt::Debug for PendingOpCommit<OpRef> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingOpCommit")
            .field("op_ref", &self.op_ref)
            .finish_non_exhaustive()
    }
}

/// In-flight isolation-drift check for a save operation.
pub struct PendingDriftCheck<DriftDetection, SavePlan> {
    pub rx: BlockingSubscription<anyhow::Result<DriftDetection>>,
    pub plan: SavePlan,
    pub exit_on_success: bool,
    pub original_name: String,
}

impl<DriftDetection, SavePlan> PendingDriftCheck<DriftDetection, SavePlan> {
    #[must_use]
    pub fn new(
        rx: BlockingSubscription<anyhow::Result<DriftDetection>>,
        original_name: String,
        plan: SavePlan,
        exit_on_success: bool,
    ) -> Self {
        Self {
            rx,
            plan,
            exit_on_success,
            original_name,
        }
    }
}

impl<DriftDetection, SavePlan> std::fmt::Debug
    for PendingDriftCheck<DriftDetection, SavePlan>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingDriftCheck")
            .field("original_name", &self.original_name)
            .field("exit_on_success", &self.exit_on_success)
            .finish_non_exhaustive()
    }
}

/// In-flight isolated-state cleanup for a save operation.
pub struct PendingIsolationCleanup<SavePlan> {
    pub rx: BlockingSubscription<anyhow::Result<()>>,
    pub plan: SavePlan,
    pub exit_on_success: bool,
}

impl<SavePlan> PendingIsolationCleanup<SavePlan> {
    #[must_use]
    pub fn new(
        rx: BlockingSubscription<anyhow::Result<()>>,
        plan: SavePlan,
        exit_on_success: bool,
    ) -> Self {
        Self {
            rx,
            plan,
            exit_on_success,
        }
    }
}

impl<SavePlan> std::fmt::Debug for PendingIsolationCleanup<SavePlan> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingIsolationCleanup")
            .field("exit_on_success", &self.exit_on_success)
            .finish_non_exhaustive()
    }
}

/// In-flight role repository registration.
pub struct PendingRoleLoad<RoleSource> {
    pub raw: String,
    pub key: String,
    pub source: RoleSource,
    pub rx: BlockingSubscription<anyhow::Result<()>>,
}

impl<RoleSource: std::fmt::Debug> std::fmt::Debug for PendingRoleLoad<RoleSource> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingRoleLoad")
            .field("raw", &self.raw)
            .field("key", &self.key)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_disconnect_messages_are_subscription_owned() {
        assert_eq!(
            drift_check_worker_disconnected_message(),
            "drift check worker disconnected"
        );
        assert_eq!(
            isolation_cleanup_worker_disconnected_message(),
            "isolation cleanup worker disconnected"
        );
        assert_eq!(
            role_loader_worker_disconnected_message(),
            "role loader worker disconnected"
        );
        assert_eq!(
            op_read_worker_disconnected_message(),
            "op read worker disconnected"
        );
        assert_eq!(
            instance_refresh_worker_disconnected_message(),
            "instance refresh worker disconnected"
        );
    }
}
