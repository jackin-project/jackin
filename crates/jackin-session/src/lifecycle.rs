// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    ClosureViolationView, Config, InspectView, LabelError, LabelSelector, Labels, LaunchId, Phase,
    PolicyStatus, RoleName, SandboxLabels, SandboxName, SandboxSummary, SupervisorReading,
    Timestamp, WorkspaceName, plain_duration,
};
use jackin_openshell::{
    ClosureViolation, GatewayError, GatewayOp, Missing, PolicyError, PowerAction, ProviderProfile,
    RuleKey, SandboxDetail, SandboxQuery, SandboxView,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadableSandbox {
    pub name: SandboxName,
    pub phase: Phase,
    pub created_at: Option<Timestamp>,
    pub workspace: Option<WorkspaceName>,
    pub role: Option<RoleName>,
    pub launch: Option<LaunchId>,
    pub error: LabelError,
}

impl UnreadableSandbox {
    pub fn reason(&self) -> String {
        match &self.error {
            LabelError::Missing {
                key: "jackin.dev/agents",
            }
            | LabelError::Invalid {
                key: "jackin.dev/agents",
                ..
            } => "the agent hash label is missing or invalid".to_owned(),
            LabelError::Missing {
                key: "jackin.dev/lead-agent",
            }
            | LabelError::Invalid {
                key: "jackin.dev/lead-agent",
                ..
            }
            | LabelError::UnknownAgent(_) => {
                "the lead agent label is missing or invalid".to_owned()
            }
            _ => "one or more required sandbox labels are missing or invalid".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ManagedIdentity {
    name: SandboxName,
    workspace: WorkspaceName,
    launch: LaunchId,
    created_at: Timestamp,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ManagedIdentityError {
    #[error("sandbox {sandbox} is not managed")]
    Unmanaged { sandbox: SandboxName },
    #[error("the managed lifecycle identity of sandbox {sandbox} is incomplete")]
    Unavailable { sandbox: SandboxName },
    #[error("sandbox {sandbox} was replaced or changed lifecycle identity")]
    Replaced { sandbox: SandboxName },
    #[error("cancelled")]
    Cancelled,
}

impl ManagedIdentity {
    fn read(view: &SandboxView) -> Result<ManagedIdentity, ManagedIdentityError> {
        if !view.labels.is_managed() {
            return Err(ManagedIdentityError::Unmanaged {
                sandbox: view.name.clone(),
            });
        }
        Ok(ManagedIdentity {
            name: view.name.clone(),
            workspace: view.labels.workspace().ok_or_else(|| {
                ManagedIdentityError::Unavailable {
                    sandbox: view.name.clone(),
                }
            })?,
            launch: view
                .labels
                .launch()
                .ok_or_else(|| ManagedIdentityError::Unavailable {
                    sandbox: view.name.clone(),
                })?,
            created_at: view
                .created_at
                .ok_or_else(|| ManagedIdentityError::Unavailable {
                    sandbox: view.name.clone(),
                })?,
        })
    }

    fn verify(&self, view: &SandboxView) -> Result<(), ManagedIdentityError> {
        if view.phase == Phase::Deleting || Self::read(view).as_ref() != Ok(self) {
            return Err(ManagedIdentityError::Replaced {
                sandbox: self.name.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SandboxInspection {
    Readable(InspectView),
    Unreadable(UnreadableSandbox),
}

#[derive(Debug)]
pub enum SandboxObservation {
    Readable(SandboxSummary),
    Unreadable(UnreadableSandbox),
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
    #[error("{sandbox} was replaced or changed lifecycle identity; nothing was deleted")]
    Replaced { sandbox: SandboxName },
    #[error(transparent)]
    Identity(#[from] ManagedIdentityError),
    #[error("cancelled")]
    Cancelled,
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
    #[error("sandbox {} has unavailable metadata: {}", .0.name, .0.reason())]
    InspectionUnreadable(Box<UnreadableSandbox>),
    #[error(transparent)]
    Identity(#[from] ManagedIdentityError),
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

const INSPECT_SCHEMA: u32 = 2;

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
        created_at: view.created_at,
    })
}

fn unreadable(view: &SandboxView, error: LabelError) -> UnreadableSandbox {
    UnreadableSandbox {
        name: view.name.clone(),
        phase: view.phase,
        created_at: view.created_at,
        workspace: view.labels.workspace(),
        role: view.labels.role(),
        launch: view.labels.launch(),
        error,
    }
}

fn observation(view: &SandboxView) -> SandboxObservation {
    match summary(view) {
        Ok(summary) => SandboxObservation::Readable(summary),
        Err(LifecycleError::Labels { source, .. }) => {
            SandboxObservation::Unreadable(unreadable(view, source))
        }
        Err(_) => unreachable!(),
    }
}

fn lifecycle_gateway(error: GatewayError) -> LifecycleError {
    match error {
        GatewayError::Cancelled { .. } => LifecycleError::Cancelled,
        other => LifecycleError::Gateway(other),
    }
}

fn purge_gateway(error: GatewayError) -> PurgeError {
    match error {
        GatewayError::Cancelled { .. } => PurgeError::Cancelled,
        other => PurgeError::Gateway(other),
    }
}

fn purge_identity(error: ManagedIdentityError) -> PurgeError {
    match error {
        ManagedIdentityError::Replaced { sandbox } => PurgeError::Replaced { sandbox },
        other => PurgeError::Identity(other),
    }
}

fn labelled(name: &SandboxName, labels: &Labels) -> Result<SandboxLabels, LifecycleError> {
    SandboxLabels::from_labels(labels).map_err(|source| LifecycleError::Labels {
        sandbox: name.clone(),
        source,
    })
}

async fn sandbox_detail(
    session: &Session,
    sandbox: &SandboxName,
    cancel: &CancellationToken,
) -> Result<Option<SandboxDetail>, GatewayError> {
    if cancel.is_cancelled() {
        return Err(GatewayError::Cancelled {
            op: GatewayOp::Detail,
            group: None,
        });
    }
    match tokio::select! {
        biased;
        () = cancel.cancelled() => Err(GatewayError::Cancelled {
            op: GatewayOp::Detail,
            group: None,
        }),
        found = session.deps.gateway.sandbox_detail(sandbox) => found,
    } {
        Ok(detail) => Ok(Some(detail)),
        Err(GatewayError::NotFound {
            what: Missing::Sandbox(name),
        }) if &name == sandbox => Ok(None),
        Err(error) => Err(error),
    }
}

async fn verify_managed_identity(
    session: &Session,
    expected: &ManagedIdentity,
    cancel: &CancellationToken,
) -> Result<SandboxView, ManagedIdentityError> {
    let detail = sandbox_detail(session, &expected.name, cancel)
        .await
        .map_err(|error| match error {
            GatewayError::Cancelled { .. } => ManagedIdentityError::Cancelled,
            _ => ManagedIdentityError::Replaced {
                sandbox: expected.name.clone(),
            },
        })?
        .ok_or_else(|| ManagedIdentityError::Replaced {
            sandbox: expected.name.clone(),
        })?;
    expected.verify(&detail.view)?;
    Ok(detail.view)
}

impl Session {
    pub async fn purge(
        &self,
        config: &Config,
        sandbox: &SandboxName,
        mode: PurgeMode,
        cancel: &CancellationToken,
    ) -> Result<PurgeOutcome, PurgeError> {
        if cancel.is_cancelled() {
            return Err(PurgeError::Cancelled);
        }
        let Some(detail) = sandbox_detail(self, sandbox, cancel)
            .await
            .map_err(purge_gateway)?
        else {
            return Ok(PurgeOutcome::AlreadyGone {
                sandbox: sandbox.clone(),
            });
        };
        let view = detail.view;
        let identity = ManagedIdentity::read(&view)?;
        if view.phase == Phase::Deleting {
            return Err(ManagedIdentityError::Replaced {
                sandbox: sandbox.clone(),
            }
            .into());
        }
        let was = self
            .work_state_of(config, sandbox, view.phase, &view.labels, cancel)
            .await;
        if cancel.is_cancelled() {
            return Err(PurgeError::Cancelled);
        }
        if mode == PurgeMode::Guarded && was != WorkState::Exported {
            return Err(PurgeError::WorkNotExported {
                sandbox: sandbox.clone(),
                state: Box::new(was),
            });
        }
        verify_managed_identity(self, &identity, cancel)
            .await
            .map_err(purge_identity)?;
        if cancel.is_cancelled() {
            return Err(PurgeError::Cancelled);
        }
        self.deps.gateway.delete_sandbox(sandbox).await?;
        Ok(PurgeOutcome::Deleted {
            sandbox: sandbox.clone(),
            was,
        })
    }

    pub async fn list(&self, cancel: &CancellationToken) -> Result<SandboxListing, LifecycleError> {
        let views = self
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed()), cancel)
            .await
            .map_err(lifecycle_gateway)?;
        let mut listing = SandboxListing {
            sandboxes: Vec::new(),
            unreadable: Vec::new(),
        };
        for view in views {
            match SandboxLabels::from_labels(&view.labels) {
                Ok(labels) => listing.sandboxes.push(SandboxSummary {
                    name: view.name.clone(),
                    labels,
                    phase: view.phase,
                    created_at: view.created_at,
                }),
                Err(error) => listing.unreadable.push(unreadable(&view, error)),
            }
        }
        Ok(listing)
    }

    pub async fn inspect(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<SandboxInspection, LifecycleError> {
        let detail = sandbox_detail(self, sandbox, cancel)
            .await
            .map_err(lifecycle_gateway)?
            .filter(|detail| detail.view.labels.is_managed())
            .ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        let labels = match SandboxLabels::from_labels(&detail.view.labels) {
            Ok(labels) => labels,
            Err(error) => {
                return Ok(SandboxInspection::Unreadable(unreadable(
                    &detail.view,
                    error,
                )));
            }
        };
        let generated: Vec<RuleKey> = detail
            .providers
            .iter()
            .map(RuleKey::provider_rule)
            .collect();
        let mut violations = Vec::new();
        let mut profile_assets = Vec::new();
        profile_assets.extend(labels.planned.lead_agent.spec().profiles());
        for asset in profile_assets {
            let profile = ProviderProfile::parse(asset)?;
            violations.extend(
                detail
                    .policy
                    .document
                    .closure_violations(&profile, &generated),
            );
        }
        let supervisor = if detail.view.phase == Phase::Ready {
            crate::state::run_state(self.deps.gateway.as_ref(), sandbox, cancel)
                .await
                .map_err(lifecycle_gateway)?
        } else {
            SupervisorReading::Unreadable
        };
        Ok(SandboxInspection::Readable(InspectView {
            schema: INSPECT_SCHEMA,
            summary: SandboxSummary {
                name: detail.view.name.clone(),
                labels,
                phase: detail.view.phase,
                created_at: detail.view.created_at,
            },
            providers: detail.providers.clone(),
            policy: PolicyStatus {
                hash: (!detail.policy.hash.is_empty()).then(|| detail.policy.hash.clone()),
                version: (detail.policy.version > 0).then_some(detail.policy.version),
                loaded: detail.policy.loaded,
            },
            closure_violations: violations.iter().map(violation_view).collect(),
            supervisor,
        }))
    }

    pub async fn power(
        &self,
        sandbox: &SandboxName,
        action: PowerAction,
        cancel: &CancellationToken,
    ) -> Result<SandboxObservation, LifecycleError> {
        let detail = sandbox_detail(self, sandbox, cancel)
            .await
            .map_err(lifecycle_gateway)?
            .ok_or_else(|| LifecycleError::NotFound(sandbox.clone()))?;
        let view = detail.view;
        let identity = ManagedIdentity::read(&view)?;
        if view.phase == Phase::Deleting {
            return Err(ManagedIdentityError::Replaced {
                sandbox: sandbox.clone(),
            }
            .into());
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
        verify_managed_identity(self, &identity, cancel).await?;
        let mut view = self
            .deps
            .gateway
            .set_power(sandbox, action, cancel)
            .await
            .map_err(lifecycle_gateway)?;
        identity.verify(&view).map_err(LifecycleError::Identity)?;
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
            let current = sandbox_detail(self, sandbox, cancel)
                .await
                .map_err(lifecycle_gateway)?
                .ok_or_else(|| {
                    LifecycleError::Identity(ManagedIdentityError::Replaced {
                        sandbox: sandbox.clone(),
                    })
                })?;
            identity
                .verify(&current.view)
                .map_err(LifecycleError::Identity)?;
            view = current.view;
        }
        Ok(observation(&view))
    }
}
