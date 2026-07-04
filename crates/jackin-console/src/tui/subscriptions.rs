// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Long-lived event sources that drive the host console TUI loop.

use jackin_tui::runtime::BlockingSubscription;

pub const INSTANCE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

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

pub fn config_save_worker_disconnected_message() -> &'static str {
    "config save worker disconnected"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceRefreshThrottleState {
    pub in_flight: bool,
    pub last_refresh: Option<std::time::Instant>,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceRefreshThrottlePlan {
    pub last_refresh: Option<std::time::Instant>,
    pub generation: u64,
    pub start_generation: Option<u64>,
}

#[must_use]
pub fn instance_refresh_throttle_plan(
    state: InstanceRefreshThrottleState,
    now: std::time::Instant,
) -> InstanceRefreshThrottlePlan {
    if state.in_flight {
        return InstanceRefreshThrottlePlan {
            last_refresh: state.last_refresh,
            generation: state.generation,
            start_generation: None,
        };
    }
    if let Some(last) = state.last_refresh
        && now.duration_since(last) < INSTANCE_REFRESH_INTERVAL
    {
        return InstanceRefreshThrottlePlan {
            last_refresh: state.last_refresh,
            generation: state.generation,
            start_generation: None,
        };
    }
    let generation = state.generation.wrapping_add(1);
    InstanceRefreshThrottlePlan {
        last_refresh: Some(now),
        generation,
        start_generation: Some(generation),
    }
}

#[must_use]
pub const fn forced_instance_refresh_generation(generation: u64) -> u64 {
    generation.wrapping_add(1)
}

#[derive(Debug)]
pub struct InstanceRefreshSnapshot<Instance, Session, Snapshot> {
    pub instances: Vec<Instance>,
    pub sessions: std::collections::HashMap<String, Vec<Session>>,
    pub session_errors: std::collections::HashSet<String>,
    pub snapshots: std::collections::HashMap<String, Snapshot>,
}

#[derive(Debug)]
pub struct WorkspaceSaveResult<AppConfig> {
    pub config: AppConfig,
    pub current_name: String,
    pub pending_rename: Option<String>,
}

#[derive(Debug)]
pub enum RoleSourcePersistOrigin<RoleSource> {
    RoleLoad {
        raw: String,
        key: String,
        source: RoleSource,
    },
    TrustConfirm {
        key: String,
        source: RoleSource,
    },
}

#[derive(Debug)]
pub enum ConfigSaveResult<AppConfig, RoleSource> {
    Workspace {
        result: anyhow::Result<WorkspaceSaveResult<AppConfig>>,
        exit_on_success: bool,
    },
    Settings(anyhow::Result<AppConfig>),
    RemoveWorkspace {
        result: anyhow::Result<AppConfig>,
        cwd: std::path::PathBuf,
    },
    RoleSourcePersist {
        result: anyhow::Result<AppConfig>,
        origin: RoleSourcePersistOrigin<RoleSource>,
    },
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

impl<DriftDetection, SavePlan> std::fmt::Debug for PendingDriftCheck<DriftDetection, SavePlan> {
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

/// Request to mint/write an auth token while the TUI is suspended.
#[derive(Debug, Clone)]
pub struct PendingTokenGenerate<Scope, Args> {
    pub scope: Scope,
    pub args: Args,
}

#[cfg(test)]
mod tests;
