// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::SandboxName;

use crate::launch::{LaunchEvent, LaunchEvents, LaunchStep, SkipReason, StepResult};

#[derive(Debug)]
pub(super) struct Tracker {
    next: usize,
    open: Option<(LaunchStep, StepResult)>,
}

fn implied(step: LaunchStep, running: bool) -> StepResult {
    match step {
        LaunchStep::Sandbox => StepResult::Skipped(SkipReason::SandboxExists),
        LaunchStep::Upload => StepResult::Skipped(SkipReason::AlreadyUploaded),
        LaunchStep::Supervisor | LaunchStep::Hooks | LaunchStep::Start if running => {
            StepResult::Skipped(SkipReason::AlreadyRunning)
        }
        LaunchStep::Image
        | LaunchStep::Provider
        | LaunchStep::Ready
        | LaunchStep::Credentials
        | LaunchStep::Supervisor
        | LaunchStep::Hooks
        | LaunchStep::Start => StepResult::Done,
    }
}

impl Tracker {
    pub(super) fn new() -> Tracker {
        Tracker {
            next: 0,
            open: None,
        }
    }

    pub(super) fn current(&self) -> Option<LaunchStep> {
        self.open.map(|(step, _)| step)
    }

    pub(super) fn skip(&mut self, reason: SkipReason) {
        if let Some((_, result)) = &mut self.open {
            *result = StepResult::Skipped(reason);
        }
    }

    pub(super) async fn enter(
        &mut self,
        events: &LaunchEvents,
        step: LaunchStep,
        known: Option<&SandboxName>,
    ) {
        let index = position(step);
        if index < self.next {
            return;
        }
        self.close(events, known).await;
        self.fill(events, index, false, known).await;
        events.emit(LaunchEvent::StepStarted(step)).await;
        self.open = Some((step, StepResult::Done));
        self.next = index + 1;
    }

    pub(super) async fn finish(
        &mut self,
        events: &LaunchEvents,
        running: bool,
        known: Option<&SandboxName>,
    ) {
        self.close(events, known).await;
        self.fill(events, LaunchStep::ALL.len(), running, known)
            .await;
    }

    async fn close(&mut self, events: &LaunchEvents, known: Option<&SandboxName>) {
        if let Some((step, result)) = self.open.take() {
            self.finished(events, step, result, known).await;
        }
    }

    async fn fill(
        &mut self,
        events: &LaunchEvents,
        until: usize,
        running: bool,
        known: Option<&SandboxName>,
    ) {
        while self.next < until {
            let step = LaunchStep::ALL[self.next];
            events.emit(LaunchEvent::StepStarted(step)).await;
            self.finished(events, step, implied(step, running), known)
                .await;
            self.next += 1;
        }
    }

    async fn finished(
        &mut self,
        events: &LaunchEvents,
        step: LaunchStep,
        result: StepResult,
        known: Option<&SandboxName>,
    ) {
        events
            .emit(LaunchEvent::StepFinished { step, result })
            .await;
        if step == LaunchStep::Sandbox
            && let Some(name) = known
        {
            events.emit(LaunchEvent::SandboxKnown(name.clone())).await;
        }
    }
}

fn position(step: LaunchStep) -> usize {
    match step {
        LaunchStep::Image => 0,
        LaunchStep::Provider => 1,
        LaunchStep::Sandbox => 2,
        LaunchStep::Ready => 3,
        LaunchStep::Credentials => 4,
        LaunchStep::Upload => 5,
        LaunchStep::Supervisor => 6,
        LaunchStep::Hooks => 7,
        LaunchStep::Start => 8,
    }
}
