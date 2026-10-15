// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::WorkflowAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowsMessage {
    SelectNext,
    SelectPrevious,
    OpenDetail,
    CloseDetail,
    /// Start observing a worker session (no confirmation: observe
    /// grants no input ownership).
    Observe(String),
    /// Request a confirmed action (resume/cancel/takeover).
    RequestAction(WorkflowAction),
    ConfirmAction,
    DismissConfirm,
    /// Stop observing / return control to the scheduler.
    StopObserving {
        return_control: bool,
    },
    RequestExport(String),
}
