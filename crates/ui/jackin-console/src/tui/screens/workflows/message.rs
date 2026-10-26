// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::WorkflowAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowsMessage {
    SelectNext,
    SelectPrevious,
    OpenDetail,
    CloseDetail,
    Observe(String),
    RequestAction(WorkflowAction),
    ConfirmAction,
    DismissConfirm,
    StopObserving { return_control: bool },
    RequestExport(String),
}
