// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    ClosureViolationView, Config, GatewayName, InspectView, LabelError, LabelSelector, Labels,
    Phase, PolicyStatus, ProviderName, SandboxLabels, SandboxName, SandboxSummary,
    SupervisorReading, WorkspaceName, plain_duration,
};
use jackin_openshell::{
    ClosureViolation, GatewayError, GatewayOp, PolicyError, PowerAction, ProfileEndpoint,
    ProviderProfile, RuleKey, SandboxQuery, SandboxView,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::export::WorkState;
use crate::launch::RECOVERY_LOOKUP_BUDGET;
use crate::service::{Located, POLL, Session, read_until_cancelled};

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
    #[error("{sandbox} exists but is not a jackin sandbox; nothing was deleted")]
    NotManaged { sandbox: SandboxName },
    #[error(
        "gateway {gateway} could not confirm whether the delete of {sandbox} took effect ({cause})"
    )]
    DeleteUnconfirmed {
        sandbox: SandboxName,
        gateway: GatewayName,
        cause: Box<GatewayError>,
    },
    #[error("cancelled before {sandbox} was deleted; nothing was deleted")]
    Cancelled { sandbox: SandboxName },
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
    #[error("sandbox {0} exists but is not a jackin sandbox")]
    NotManaged(SandboxName),
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
    #[error("cancelled while {sandbox} was changing to {wanted}; the request was already sent")]
    PowerCancelled { sandbox: SandboxName, wanted: Phase },
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

fn lifecycle_gateway(error: GatewayError) -> LifecycleError {
    match error {
        GatewayError::Cancelled { .. } => LifecycleError::Cancelled,
        other => LifecycleError::Gateway(other),
    }
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
        cancel: &CancellationToken,
    ) -> Result<PurgeOutcome, PurgeError> {
        let cancelled = || PurgeError::Cancelled {
            sandbox: sandbox.clone(),
        };
        let view = match self.locate(sandbox, cancel).await {
            Ok(Located::Found(view)) if view.phase != Phase::Deleting => *view,
            Ok(Located::Found(_) | Located::Absent) => {
                return Ok(PurgeOutcome::AlreadyGone {
                    sandbox: sandbox.clone(),
                });
            }
            Ok(Located::Unmanaged) => {
                return Err(PurgeError::NotManaged {
                    sandbox: sandbox.clone(),
                });
            }
            Err(GatewayError::Cancelled { .. }) => return Err(cancelled()),
            Err(error) => return Err(error.into()),
        };
        let was = self
            .work_state_of(config, sandbox, view.phase, &view.labels, cancel)
            .await;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        if mode == PurgeMode::Guarded && was != WorkState::Exported {
            return Err(PurgeError::WorkNotExported {
                sandbox: sandbox.clone(),
                state: Box::new(was),
            });
        }
        match self.locate(sandbox, cancel).await {
            Ok(Located::Found(now)) if now.phase != Phase::Deleting => {
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
            Ok(Located::Found(_) | Located::Absent) => {
                return Ok(PurgeOutcome::AlreadyGone {
                    sandbox: sandbox.clone(),
                });
            }
            Ok(Located::Unmanaged) => {
                return Err(PurgeError::Replaced {
                    sandbox: sandbox.clone(),
                });
            }
            Err(GatewayError::Cancelled { .. }) => return Err(cancelled()),
            Err(error) => return Err(error.into()),
        }
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        match self.deps.gateway.delete_sandbox(sandbox).await {
            Ok(()) => Ok(PurgeOutcome::Deleted {
                sandbox: sandbox.clone(),
                was,
            }),
            Err(error) if error.outcome_unknown() => {
                self.settle_delete(sandbox, &view, was, error).await
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn settle_delete(
        &self,
        sandbox: &SandboxName,
        deleted: &SandboxView,
        was: WorkState,
        error: GatewayError,
    ) -> Result<PurgeOutcome, PurgeError> {
        let outcome = PurgeOutcome::Deleted {
            sandbox: sandbox.clone(),
            was,
        };
        let unconfirmed = |cause| PurgeError::DeleteUnconfirmed {
            sandbox: sandbox.clone(),
            gateway: self.gateway_name().clone(),
            cause: Box::new(cause),
        };
        let fresh = CancellationToken::new();
        let lookup = self.locate(sandbox, &fresh);
        let budget = self.deps.host.clock.sleep(RECOVERY_LOOKUP_BUDGET);
        tokio::select! {
            biased;
            found = lookup => match found {
                Ok(Located::Absent) => Ok(outcome),
                Ok(Located::Found(now)) if now.phase == Phase::Deleting => Ok(outcome),
                Ok(Located::Found(now)) => match (deleted.created_at, now.created_at) {
                    (Some(before), Some(current)) if current != before => Ok(outcome),
                    _ => Err(unconfirmed(error)),
                },
                Ok(Located::Unmanaged) | Err(_) => Err(unconfirmed(error)),
            },
            () = budget => Err(unconfirmed(error)),
        }
    }

    pub async fn list(&self, cancel: &CancellationToken) -> Result<SandboxListing, LifecycleError> {
        let views = read_until_cancelled(
            cancel,
            GatewayOp::Find,
            self.deps
                .gateway
                .find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed())),
        )
        .await
        .map_err(lifecycle_gateway)?;
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

    pub async fn inspect(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<InspectView, LifecycleError> {
        let before = self.found_until(sandbox, cancel).await?;
        let detail = match read_until_cancelled(
            cancel,
            GatewayOp::Detail,
            self.deps.gateway.sandbox_detail(sandbox),
        )
        .await
        {
            Ok(detail) => detail,
            Err(GatewayError::NotFound { .. }) => {
                return Err(LifecycleError::NotFound(sandbox.clone()));
            }
            Err(error) => return Err(lifecycle_gateway(error)),
        };
        if !detail.view.labels.is_managed() {
            return Err(LifecycleError::NotManaged(sandbox.clone()));
        }
        let require_same = |view: &SandboxView| {
            if view.name == *sandbox
                && before.created_at.is_some()
                && view.created_at == before.created_at
            {
                return Ok(());
            }
            Err(LifecycleError::Gateway(GatewayError::FailedPrecondition {
                message: format!(
                    "sandbox {sandbox} changed or could not be identified during inspection"
                ),
            }))
        };
        require_same(&detail.view)?;
        let after = self.found_until(sandbox, cancel).await?;
        require_same(&after)?;
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
            let instances = self
                .instances_of(sandbox, &detail.view, cancel)
                .await
                .map_err(lifecycle_gateway)?;
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
            read_until_cancelled(
                cancel,
                GatewayOp::ReadFiles,
                crate::state::run_state(self.deps.gateway.as_ref(), sandbox),
            )
            .await
            .map_err(lifecycle_gateway)?
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
        let view = self.power_view(sandbox, action, cancel, true).await?;
        summary(&view)
    }

    pub async fn power_unreadable(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
        cancel: &CancellationToken,
    ) -> Result<(), LifecycleError> {
        self.power_view(sandbox, action, cancel, false)
            .await
            .map(drop)
    }

    async fn found_until(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<SandboxView, LifecycleError> {
        match self.locate(sandbox, cancel).await {
            Ok(found) => found.require(
                || LifecycleError::NotFound(sandbox.clone()),
                || LifecycleError::NotManaged(sandbox.clone()),
            ),
            Err(error) => Err(lifecycle_gateway(error)),
        }
    }

    async fn power_view(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
        cancel: &CancellationToken,
        require_labels: bool,
    ) -> Result<SandboxView, LifecycleError> {
        let view = self.found_until(sandbox, cancel).await?;
        if require_labels {
            labelled(sandbox, &view.labels)?;
        }
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
        if cancel.is_cancelled() {
            return Err(LifecycleError::Cancelled);
        }
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
                () = cancel.cancelled() => return Err(LifecycleError::PowerCancelled {
                    sandbox: sandbox.clone(),
                    wanted,
                }),
                () = self.deps.host.clock.sleep(POLL) => {}
            }
            waited += POLL;
            view = match self.found_until(sandbox, cancel).await {
                Err(LifecycleError::Cancelled) => {
                    return Err(LifecycleError::PowerCancelled {
                        sandbox: sandbox.clone(),
                        wanted,
                    });
                }
                other => other?,
            };
        }
        Ok(view)
    }
}
