// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::{ObserveTarget, WorkflowAction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowsEffect {
    Observe(ObserveTarget),
    TakeControl(ObserveTarget),
    ReleaseControl(ObserveTarget),
    Resume { run_id: String },
    Cancel { run_id: String },
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
