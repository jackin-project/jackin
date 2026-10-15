// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod effect;
pub mod message;
pub mod model;
pub mod screen;
pub mod update;
pub mod view;

pub use effect::WorkflowsEffect;
pub use message::WorkflowsMessage;
pub use model::{ObserveTarget, WorkflowAction, WorkflowScreen};
pub use update::apply_workflows_message;
pub use view::{
    blocking_reason, confirm_prompt, render_workflow_detail, render_workflow_list,
    workflow_detail_lines, workflow_list_line,
};
