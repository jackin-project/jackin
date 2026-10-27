// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_workflow::{RunDetail, RunSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowAction {
    Resume,
    Cancel,
    TakeControl,
}

impl WorkflowAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Cancel => "Cancel",
            Self::TakeControl => "Take control",
        }
    }

    #[must_use]
    pub const fn needs_confirm(self) -> bool {
        match self {
            Self::Resume | Self::Cancel | Self::TakeControl => true,
        }
    }
}

#[derive(Debug, Default)]
pub struct WorkflowScreen {
    runs: Vec<RunSummary>,
    selected: usize,
    detail: Option<RunDetail>,
    confirm: Option<WorkflowAction>,
    error: Option<String>,
    observing: Option<ObserveTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveTarget {
    pub run_id: String,
    pub worker_id: String,
    pub instance_id: Option<String>,
    pub session_id: Option<i64>,
    pub capsule_epoch: Option<i64>,
    pub process_identity: Option<String>,
}

impl WorkflowScreen {
    #[must_use]
    pub fn new(runs: Vec<RunSummary>) -> Self {
        Self {
            runs,
            selected: 0,
            detail: None,
            confirm: None,
            error: None,
            observing: None,
        }
    }

    #[must_use]
    pub const fn runs(&self) -> &Vec<RunSummary> {
        &self.runs
    }

    pub fn set_runs(&mut self, runs: Vec<RunSummary>) {
        self.runs = runs;
        self.selected = self.selected.min(self.runs.len().saturating_sub(1));
    }

    #[must_use]
    pub const fn selected_index(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_run(&self) -> Option<&RunSummary> {
        self.runs.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.runs.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.runs.len() - 1;
        if delta < 0 {
            self.selected = self.selected.saturating_sub(delta.unsigned_abs());
        } else {
            self.selected = self
                .selected
                .saturating_add(usize::try_from(delta).unwrap_or(usize::MAX))
                .min(max);
        }
    }

    #[must_use]
    pub const fn detail(&self) -> Option<&RunDetail> {
        self.detail.as_ref()
    }

    pub fn set_detail(&mut self, detail: RunDetail) {
        self.detail = Some(detail);
    }

    pub fn close_detail(&mut self) {
        self.detail = None;
        self.confirm = None;
    }

    #[must_use]
    pub const fn confirm(&self) -> Option<WorkflowAction> {
        self.confirm
    }

    pub fn open_confirm(&mut self, action: WorkflowAction) {
        self.confirm = Some(action);
    }

    pub fn dismiss_confirm(&mut self) {
        self.confirm = None;
    }

    #[must_use]
    pub const fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    pub fn set_error(&mut self, message: Option<String>) {
        self.error = message;
    }

    #[must_use]
    pub const fn observing(&self) -> Option<&ObserveTarget> {
        self.observing.as_ref()
    }

    #[must_use]
    pub fn observe_target(&self, worker_id: &str) -> Option<ObserveTarget> {
        let detail = self.detail.as_ref()?;
        let session = detail
            .sessions
            .iter()
            .find(|session| session.worker_id == worker_id)?;
        Some(ObserveTarget {
            run_id: detail.summary.run_id.clone(),
            worker_id: worker_id.to_owned(),
            instance_id: session.instance_id.clone(),
            session_id: session.session_id,
            capsule_epoch: session.capsule_epoch,
            process_identity: session.process_identity.clone(),
        })
    }

    pub fn set_observing(&mut self, target: Option<ObserveTarget>) {
        self.observing = target;
    }
}
