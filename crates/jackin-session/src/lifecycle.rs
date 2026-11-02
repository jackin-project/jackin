// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    LabelError, LabelSelector, Labels, Phase, SandboxLabels, SandboxName, SandboxSummary,
};
use jackin_openshell::{GatewayError, PowerAction, SandboxQuery, SandboxView};
use thiserror::Error;

use crate::export::WorkState;
use crate::service::{POLL, Session};

#[derive(Debug)]
pub struct SandboxListing {
    pub sandboxes: Vec<SandboxSummary>,
    pub unreadable: Vec<UnreadableSandbox>,
}

#[derive(Debug)]
pub struct UnreadableSandbox {
    pub name: SandboxName,
    pub error: LabelError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PurgeMode {
    Guarded,
    Forced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurgeOutcome {
    Deleted {
        sandbox: SandboxName,
        was: WorkState,
    },
    AlreadyGone {
        sandbox: SandboxName,
    },
}

#[derive(Debug, Error)]
pub enum PurgeError {
    #[error("the work in {sandbox:?} is not exported: {state:?}")]
    WorkNotExported {
        sandbox: SandboxName,
        state: WorkState,
    },
    #[error(transparent)]
    Gateway(#[from] GatewayError),
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("the labels of {sandbox:?} are invalid")]
    Labels {
        sandbox: SandboxName,
        source: LabelError,
    },
    #[error("sandbox {0:?} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox:?} did not reach {wanted:?} within {after:?}")]
    PowerTimeout {
        sandbox: SandboxName,
        wanted: Phase,
        after: Duration,
    },
    #[error("the main process of {sandbox:?} has ended: {phase:?}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
}

fn summary(view: &SandboxView) -> Result<SandboxSummary, LifecycleError> {
    labelled(&view.name, &view.labels).map(|labels| SandboxSummary {
        name: view.name.clone(),
        labels,
        phase: view.phase,
    })
}

fn labelled(name: &SandboxName, labels: &Labels) -> Result<SandboxLabels, LifecycleError> {
    SandboxLabels::from_labels(labels).map_err(|source| LifecycleError::Labels {
        sandbox: name.clone(),
        source,
    })
}

impl Session {
    pub async fn list(&self) -> Result<SandboxListing, LifecycleError> {
        let views = self
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed()))
            .await?;
        let mut listing = SandboxListing {
            sandboxes: Vec::new(),
            unreadable: Vec::new(),
        };
        for view in views {
            match SandboxLabels::from_labels(&view.labels) {
                Ok(labels) => listing.sandboxes.push(SandboxSummary {
                    name: view.name,
                    labels,
                    phase: view.phase,
                }),
                Err(error) => listing.unreadable.push(UnreadableSandbox {
                    name: view.name,
                    error,
                }),
            }
        }
        Ok(listing)
    }

    pub async fn power(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxSummary, LifecycleError> {
        let found = self.find_managed(sandbox).await?;
        let view = found.ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        if view.phase.main_process_ended() {
            return Err(LifecycleError::MainProcessEnded {
                sandbox: sandbox.clone(),
                phase: view.phase,
            });
        }
        let wanted = match action {
            PowerAction::Start => Phase::Ready,
            PowerAction::Stop => Phase::Stopped,
        };
        let mut view = self.deps.gateway.set_power(sandbox, action).await?;
        let limit = self.deps.limits.ready;
        let mut waited = Duration::ZERO;
        while view.phase != wanted {
            if view.phase.main_process_ended() {
                return Err(LifecycleError::MainProcessEnded {
                    sandbox: sandbox.clone(),
                    phase: view.phase,
                });
            }
            if waited >= limit {
                return Err(LifecycleError::PowerTimeout {
                    sandbox: sandbox.clone(),
                    wanted,
                    after: waited,
                });
            }
            self.deps.host.clock.sleep(POLL).await;
            waited += POLL;
            view = self
                .find_managed(sandbox)
                .await?
                .ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        }
        summary(&view)
    }
}
