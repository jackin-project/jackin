// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    ClosureViolationView, Config, InspectView, LabelError, LabelSelector, Labels, Phase,
    PolicyStatus, ProviderName, SandboxLabels, SandboxName, SandboxSummary, SupervisorReading,
    WorkspaceName, plain_duration,
};
use jackin_openshell::{
    ClosureViolation, GatewayError, PolicyError, PowerAction, ProfileEndpoint, ProviderProfile,
    RuleKey, SandboxQuery, SandboxView,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::export::WorkState;
use crate::service::{POLL, Session};

#[derive(Debug)]
pub struct SandboxListing {
    pub sandboxes: Vec<SandboxSummary>,
    pub unreadable: Vec<UnreadableSandbox>,
}

#[derive(Clone, Debug)]
pub struct UnreadableSandbox {
    pub name: SandboxName,
    pub workspace: Option<WorkspaceName>,
    pub role: Option<jackin_core::RoleName>,
    pub phase: Phase,
    pub created_at: Option<jackin_core::Timestamp>,
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
    #[error("the work in {sandbox} is not exported: {state}")]
    WorkNotExported {
        sandbox: SandboxName,
        state: Box<WorkState>,
    },
    #[error(
        "{sandbox} was replaced, or cannot be told apart from a replacement, while its work was inspected; nothing was deleted"
    )]
    Replaced { sandbox: SandboxName },
    #[error(transparent)]
    Gateway(#[from] GatewayError),
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("the labels of {sandbox} are invalid")]
    Labels {
        sandbox: SandboxName,
        source: LabelError,
    },
    #[error("sandbox {0} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox} did not reach {wanted} within {}", plain_duration(.after))]
    PowerTimeout {
        sandbox: SandboxName,
        wanted: Phase,
        after: Duration,
    },
    #[error("the main process of {sandbox} has ended: {phase}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    #[error(transparent)]
    Profile(#[from] PolicyError),
    #[error("cancelled")]
    Cancelled,
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

fn created_at(view: &SandboxView) -> Option<jackin_core::Timestamp> {
    view.created_at
        .map(|created| jackin_core::Timestamp(created.seconds))
}

fn summary(view: &SandboxView) -> Result<SandboxSummary, LifecycleError> {
    labelled(&view.name, &view.labels).map(|labels| SandboxSummary {
        name: view.name.clone(),
        labels,
        phase: view.phase,
        created_at: created_at(view),
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
        match self.find_managed(sandbox).await? {
            Some(now) if now.phase != Phase::Deleting => {
                let replaced = match (view.created_at, now.created_at) {
                    (Some(before), Some(current)) => current != before,
                    _ => true,
                };
                if replaced {
                    return Err(PurgeError::Replaced {
                        sandbox: sandbox.clone(),
                    });
                }
            }
            Some(_) | None => {
                return Ok(PurgeOutcome::AlreadyGone {
                    sandbox: sandbox.clone(),
                });
            }
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
            let created = created_at(&view);
            match SandboxLabels::from_labels(&view.labels) {
                Ok(labels) => listing.sandboxes.push(SandboxSummary {
                    name: view.name,
                    labels,
                    phase: view.phase,
                    created_at: created,
                }),
                Err(error) => listing.unreadable.push(UnreadableSandbox {
                    name: view.name,
                    workspace: view.labels.workspace(),
                    role: view.labels.role(),
                    phase: view.phase,
                    created_at: created,
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
        let mut violations = Vec::new();
        let mut profile_assets = Vec::new();
        let rules = detail.policy.document.network_rules();
        let mut generated: Vec<RuleKey> = Vec::new();
        for provider in &detail.providers {
            let key = RuleKey::provider_rule(provider);
            if !generated.contains(&key) {
                generated.push(key);
            }
            if let Some((_, agent)) = provider.account_agent() {
                profile_assets.extend(agent.spec().profiles());
            }
        }
        if detail.view.phase == Phase::Ready {
            let instances = self.sandbox_instances(sandbox).await?;
            for instance in instances.canonical() {
                let key = RuleKey::provider_rule(&ProviderName::for_account(
                    &instance.account,
                    instance.agent,
                ));
                if !generated.contains(&key) {
                    generated.push(key);
                }
                profile_assets.extend(instance.agent.spec().profiles());
                let tuple = jackin_core::lookup_tuple(instance.agent, instance.provider)
                    .map_err(|_| PolicyError::InvalidHost("unknown tuple".to_owned()))?;
                profile_assets.push(jackin_core::tuple_profile_asset(tuple));
            }
        }
        for rule in &rules {
            if !generated.contains(&rule.key) {
                continue;
            }
            let retained = ProviderProfile {
                id: jackin_core::ProfileId::parse("jackin-retained-closure")
                    .map_err(|_| PolicyError::InvalidHost("retained closure".to_owned()))?,
                yaml: "",
                endpoints: rule
                    .endpoints
                    .iter()
                    .map(|endpoint| ProfileEndpoint {
                        host: endpoint.host.clone(),
                        port: endpoint.port,
                    })
                    .collect(),
                binaries: Vec::new(),
            };
            violations.extend(
                detail
                    .policy
                    .document
                    .closure_violations(&retained, &generated),
            );
        }
        for asset in profile_assets {
            let profile = ProviderProfile::parse(asset)?;
            violations.extend(
                detail
                    .policy
                    .document
                    .closure_violations(&profile, &generated),
            );
        }
        violations.sort();
        violations.dedup();
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
                created_at: created_at(&detail.view),
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
        cancel: &CancellationToken,
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
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(LifecycleError::Cancelled),
                () = self.deps.host.clock.sleep(POLL) => {}
            }
            waited += POLL;
            view = self
                .find_managed(sandbox)
                .await?
                .ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        }
        summary(&view)
    }

    pub async fn power_unreadable(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
        cancel: &CancellationToken,
    ) -> Result<(), LifecycleError> {
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
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(LifecycleError::Cancelled),
                () = self.deps.host.clock.sleep(POLL) => {}
            }
            waited += POLL;
            view = self
                .find_managed(sandbox)
                .await?
                .ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        }
        Ok(())
    }
}
