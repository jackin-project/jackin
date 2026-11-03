// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::time::Duration;

use jackin_core::{
    AttemptId, Phase, SandboxLabels, SandboxName, SupervisorReading, SupervisorState,
};
use jackin_image::BuiltImage;
use jackin_openshell::{Provisioning, SandboxView};

use crate::launch::probe::SandboxProbe;
use crate::launch::{AttachTarget, LaunchError, LaunchStep};
use crate::plan::{LaunchLimits, LaunchPlan};

#[derive(Debug)]
pub(super) enum Observed {
    Absent,
    Ambiguous(Vec<SandboxName>),
    Settling(SandboxView),
    Ready(SandboxView, SandboxProbe),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Boot {
    Observed,
    Powered,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Waited {
    pub(super) ready: Duration,
    pub(super) supervisor: Duration,
    pub(super) attempt: Duration,
}

#[derive(Debug)]
pub(super) struct Done {
    pub(super) steps: BTreeSet<LaunchStep>,
    pub(super) image: Option<BuiltImage>,
    pub(super) created: u8,
    pub(super) boot: Boot,
    pub(super) started: Option<AttemptId>,
    pub(super) waited: Waited,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Wait {
    Ready,
    Supervisor,
    Attempt,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HookPlan {
    pub(super) setup: bool,
    pub(super) setup_done: bool,
    pub(super) preflight: bool,
}

#[derive(Debug)]
pub(super) enum Act {
    Image,
    Provider,
    Create(BuiltImage),
    Replace(SandboxName),
    Power(SandboxName),
    Wait(Wait),
    Credentials(SandboxName),
    Upload(SandboxName),
    Hooks(SandboxName, HookPlan),
    Start(SandboxName, AttemptId),
}

#[derive(Debug)]
pub(super) struct Finish {
    pub(super) target: AttachTarget,
    pub(super) running: bool,
}

#[derive(Debug)]
pub(super) enum Decision {
    Run(Act),
    Finish(Finish),
    Fail(LaunchStep, LaunchError),
}

impl Done {
    pub(super) fn new() -> Done {
        Done {
            steps: BTreeSet::new(),
            image: None,
            created: 0,
            boot: Boot::Observed,
            started: None,
            waited: Waited::default(),
        }
    }

    pub(super) fn forget_sandbox(&mut self) {
        self.steps.retain(|step| *step == LaunchStep::Provider);
        self.boot = Boot::Observed;
        self.started = None;
    }

    pub(super) fn absorb(&mut self, observed: &Observed) {
        if let Observed::Ready(_, probe) = observed {
            self.steps.insert(LaunchStep::Ready);
            if self.boot == Boot::Powered
                && probe.supervisor == SupervisorReading::Reported(SupervisorState::Waiting)
            {
                self.boot = Boot::Observed;
            }
        }
    }

    pub(super) fn wait(&mut self, kind: Wait, spent: Duration) {
        let slot = match kind {
            Wait::Ready => &mut self.waited.ready,
            Wait::Supervisor => &mut self.waited.supervisor,
            Wait::Attempt => &mut self.waited.attempt,
        };
        *slot += spent;
    }
}

fn waiting(
    spent: Duration,
    limit: Duration,
    kind: Wait,
    timeout: impl FnOnce(Duration) -> (LaunchStep, LaunchError),
) -> Decision {
    if spent >= limit {
        let (step, error) = timeout(spent);
        Decision::Fail(step, error)
    } else {
        Decision::Run(Act::Wait(kind))
    }
}

pub(super) fn decide(
    plan: &LaunchPlan,
    observed: &Observed,
    done: &Done,
    limits: &LaunchLimits,
) -> Decision {
    let Some(image) = &done.image else {
        return Decision::Run(Act::Image);
    };
    if !done.steps.contains(&LaunchStep::Provider) {
        return Decision::Run(Act::Provider);
    }
    if let Some(stale) = stale(plan, image, observed) {
        return Decision::Fail(LaunchStep::Sandbox, stale);
    }
    match observed {
        Observed::Absent => Decision::Run(Act::Create(image.clone())),
        Observed::Ambiguous(sandboxes) => Decision::Fail(
            LaunchStep::Sandbox,
            LaunchError::AmbiguousLaunch {
                launch: plan.launch_id().clone(),
                sandboxes: sandboxes.clone(),
            },
        ),
        Observed::Settling(view) => settling(view, done, limits),
        Observed::Ready(view, probe) => ready(view, probe, done, limits),
    }
}

fn stale(plan: &LaunchPlan, image: &BuiltImage, observed: &Observed) -> Option<LaunchError> {
    let (Observed::Settling(view) | Observed::Ready(view, _)) = observed else {
        return None;
    };
    let wanted = plan.draft().labels().clone().complete(&image.id);
    let found = match SandboxLabels::from_labels(&view.labels) {
        Ok(found) => found,
        Err(error) => {
            return Some(LaunchError::SandboxLabels {
                sandbox: view.name.clone(),
                error,
            });
        }
    };
    (found.image != wanted.image || found.planned.docker != wanted.planned.docker).then(|| {
        LaunchError::StaleSandbox {
            launch: plan.launch_id().clone(),
            sandbox: view.name.clone(),
            found_image: found.image,
            wanted_image: wanted.image,
            found_docker: found.planned.docker,
            wanted_docker: wanted.planned.docker,
        }
    })
}

fn settling(view: &SandboxView, done: &Done, limits: &LaunchLimits) -> Decision {
    let sandbox = view.name.clone();
    let recreatable = done.created >= 1 && !done.steps.contains(&LaunchStep::Ready);
    let wait_ready = |phase: Phase| {
        waiting(done.waited.ready, limits.ready, Wait::Ready, |after| {
            (
                LaunchStep::Ready,
                LaunchError::ReadyTimeout {
                    sandbox: sandbox.clone(),
                    phase,
                    after,
                },
            )
        })
    };
    match view.phase {
        Phase::Provisioning
        | Phase::Starting
        | Phase::Stopping
        | Phase::Suspending
        | Phase::Unknown => wait_ready(view.phase),
        Phase::Stopped if done.boot == Boot::Powered => wait_ready(Phase::Stopped),
        Phase::Stopped => Decision::Run(Act::Power(sandbox.clone())),
        Phase::Suspended => Decision::Fail(
            LaunchStep::Ready,
            LaunchError::SandboxSuspended {
                sandbox: sandbox.clone(),
            },
        ),
        Phase::Error { exit_code: None } if recreatable => {
            let Provisioning::Failed { message } = &view.provisioning else {
                return ended(view);
            };
            if done.created < limits.create_attempts.get() {
                Decision::Run(Act::Replace(sandbox.clone()))
            } else {
                Decision::Fail(
                    LaunchStep::Sandbox,
                    LaunchError::ProvisioningFailed {
                        sandbox: sandbox.clone(),
                        attempts: done.created,
                        message: message.clone(),
                    },
                )
            }
        }
        Phase::Deleting if recreatable => wait_ready(Phase::Deleting),
        Phase::Ready | Phase::Completed { .. } | Phase::Error { .. } | Phase::Deleting => {
            ended(view)
        }
    }
}

fn ended(view: &SandboxView) -> Decision {
    Decision::Fail(
        LaunchStep::Ready,
        LaunchError::MainProcessEnded {
            sandbox: view.name.clone(),
            phase: view.phase,
        },
    )
}

fn ready(view: &SandboxView, probe: &SandboxProbe, done: &Done, limits: &LaunchLimits) -> Decision {
    let sandbox = &view.name;
    if !done.steps.contains(&LaunchStep::Credentials) {
        return Decision::Run(Act::Credentials(sandbox.clone()));
    }
    if !probe.uploaded {
        return Decision::Run(Act::Upload(sandbox.clone()));
    }
    let silent = || {
        waiting(
            done.waited.supervisor,
            limits.supervisor,
            Wait::Supervisor,
            |after| {
                (
                    LaunchStep::Supervisor,
                    LaunchError::SupervisorTimeout {
                        sandbox: sandbox.clone(),
                        after,
                    },
                )
            },
        )
    };
    let SupervisorReading::Reported(state) = &probe.supervisor else {
        return silent();
    };
    let stale = done.boot == Boot::Powered && *state != SupervisorState::Waiting;
    if stale {
        return silent();
    }
    supervised(sandbox, probe, *state, done, limits)
}

fn supervised(
    sandbox: &SandboxName,
    probe: &SandboxProbe,
    state: SupervisorState,
    done: &Done,
    limits: &LaunchLimits,
) -> Decision {
    let target = |running: bool| {
        Decision::Finish(Finish {
            target: AttachTarget {
                sandbox: sandbox.clone(),
                state,
            },
            running,
        })
    };
    match (state, done.started) {
        (SupervisorState::Running { .. }, None) => target(true),
        (
            SupervisorState::Running { attempt } | SupervisorState::Exited { attempt, .. },
            Some(started),
        ) if attempt == started => target(false),
        (SupervisorState::Waiting | SupervisorState::Exited { .. }, None) => {
            let Some(next) = state.next_attempt() else {
                return Decision::Fail(
                    LaunchStep::Start,
                    LaunchError::AttemptsExhausted {
                        sandbox: sandbox.clone(),
                    },
                );
            };
            if done.steps.contains(&LaunchStep::Hooks) {
                Decision::Run(Act::Start(sandbox.clone(), next))
            } else {
                Decision::Run(Act::Hooks(
                    sandbox.clone(),
                    HookPlan {
                        setup: probe.hooks.setup && !probe.setup_done,
                        setup_done: probe.hooks.setup && probe.setup_done,
                        preflight: probe.hooks.preflight,
                    },
                ))
            }
        }
        (_, Some(started)) => waiting(done.waited.attempt, limits.attempt, Wait::Attempt, |_| {
            (
                LaunchStep::Start,
                LaunchError::AttemptTimeout {
                    sandbox: sandbox.clone(),
                    attempt: started,
                    last: SupervisorReading::Reported(state),
                },
            )
        }),
    }
}
