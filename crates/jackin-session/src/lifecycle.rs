// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    ClosureViolationView, Config, InspectView, LabelError, LabelSelector, Labels, Phase,
    PolicyStatus, SandboxLabels, SandboxName, SandboxSummary, SupervisorReading,
};
use jackin_openshell::{
    ClosureViolation, GatewayError, PolicyError, PowerAction, ProviderProfile, RuleKey,
    SandboxQuery, SandboxView,
};
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
        state: Box<WorkState>,
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
    #[error(transparent)]
    Profile(#[from] PolicyError),
}

const INSPECT_SCHEMA: u32 = 1;

fn violation_view(violation: &ClosureViolation) -> ClosureViolationView {
    match violation {
        ClosureViolation::Overlap {
            rule,
            host,
            provider_host,
        } => ClosureViolationView {
            rule: rule.to_string(),
            host: host.to_string(),
            provider_host: provider_host.to_string(),
        },
        ClosureViolation::UnparsableHost { rule, host } => ClosureViolationView {
            rule: rule.to_string(),
            host: host.clone(),
            provider_host: String::new(),
        },
        ClosureViolation::InvalidPort { rule, port } => ClosureViolationView {
            rule: rule.to_string(),
            host: format!("port {port}"),
            provider_host: String::new(),
        },
    }
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
    pub async fn purge(
        &self,
        config: &Config,
        sandbox: &SandboxName,
        mode: PurgeMode,
    ) -> Result<PurgeOutcome, PurgeError> {
        let found = self.find_managed(sandbox).await?;
        let Some(view) = found.filter(|view| view.phase != Phase::Deleting) else {
            return Ok(PurgeOutcome::AlreadyGone {
                sandbox: sandbox.clone(),
            });
        };
        let was = self
            .work_state_of(config, sandbox, view.phase, &view.labels)
            .await;
        if mode == PurgeMode::Guarded && was != WorkState::Exported {
            return Err(PurgeError::WorkNotExported {
                sandbox: sandbox.clone(),
                state: Box::new(was),
            });
        }
        self.deps.gateway.delete_sandbox(sandbox).await?;
        Ok(PurgeOutcome::Deleted {
            sandbox: sandbox.clone(),
            was,
        })
    }

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

    pub async fn inspect(&self, sandbox: &SandboxName) -> Result<InspectView, LifecycleError> {
        let detail = match self.deps.gateway.sandbox_detail(sandbox).await {
            Ok(detail) if detail.view.labels.is_managed() => detail,
            Ok(_) | Err(GatewayError::NotFound { .. }) => {
                return Err(LifecycleError::NotFound(sandbox.clone()));
            }
            Err(error) => return Err(error.into()),
        };
        let labels = labelled(sandbox, &detail.view.labels)?;
        let generated: Vec<RuleKey> = detail
            .providers
            .iter()
            .map(RuleKey::provider_rule)
            .collect();
        let mut violations = Vec::new();
        for asset in labels.planned.agent.spec().profiles() {
            let profile = ProviderProfile::parse(asset)?;
            violations.extend(
                detail
                    .policy
                    .document
                    .closure_violations(&profile, &generated),
            );
        }
        let supervisor = if detail.view.phase == Phase::Ready {
            crate::state::run_state(self.deps.gateway.as_ref(), sandbox).await?
        } else {
            SupervisorReading::Unreadable
        };
        Ok(InspectView {
            schema: INSPECT_SCHEMA,
            summary: SandboxSummary {
                name: detail.view.name.clone(),
                labels,
                phase: detail.view.phase,
            },
            providers: detail.providers.clone(),
            policy: PolicyStatus {
                hash: (!detail.policy.hash.is_empty()).then(|| detail.policy.hash.clone()),
                version: (detail.policy.version > 0).then_some(detail.policy.version),
                loaded: detail.policy.loaded,
            },
            closure_violations: violations.iter().map(violation_view).collect(),
            supervisor,
        })
    }

    pub async fn power(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxSummary, LifecycleError> {
        let found = self.find_managed(sandbox).await?;
        let view = found.ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        labelled(sandbox, &view.labels)?;
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
