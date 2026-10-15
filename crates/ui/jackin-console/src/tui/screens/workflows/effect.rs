// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-executed workflow intents. The screen emits these; the host
//! (which owns the daemon and Capsule clients) performs them and
//! feeds back fresh snapshots.

use super::model::{ObserveTarget, WorkflowAction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowsEffect {
    /// Open the exact recorded session read-only via `SessionEvents`
    /// (observe grants no input ownership).
    Observe(ObserveTarget),
    /// Take exclusive input ownership (`TakeControl`), then
    /// pause the run while the operator holds it.
    TakeControl(ObserveTarget),
    /// Explicit release of operator control (`ReleaseControl`);
    /// the run stays paused until `Resume`.
    ReleaseControl(ObserveTarget),
    /// Resume a paused or operator-blocked run.
    Resume { run_id: String },
    /// Cancel a run and preserve its source.
    Cancel { run_id: String },
    /// Write the Markdown report to a directory.
    Export { run_id: String, out_dir: String },
}

impl WorkflowsEffect {
    #[must_use]
    pub const fn action_label(&self) -> &'static str {
        match self {
            Self::Observe(_) => "Observe",
            Self::TakeControl(_) => "Take control",
            Self::ReleaseControl(_) => "Return control",
            Self::Resume { .. } => WorkflowAction::Resume.label(),
            Self::Cancel { .. } => WorkflowAction::Cancel.label(),
            Self::Export { .. } => "Export",
        }
    }
}
