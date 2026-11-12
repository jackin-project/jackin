// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use jackin_core::{EnvName, Labels, Phase, ProviderName, SandboxName, TerminalText};
use openshell_sdk::raw::proto::GetSandboxConfigRequest;
use openshell_sdk::raw::proto::{
    CreateSandboxRequest, DeleteSandboxRequest, GetSandboxPolicyStatusRequest, GetSandboxRequest,
    ListSandboxProvidersRequest, ListSandboxesRequest, PolicyStatus, Sandbox, SandboxPhase,
    SandboxRestartPolicy, SandboxSpec, SandboxTemplate, StartSandboxRequest, StopSandboxRequest,
};

use super::Inner;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::gateway::{
    PolicyReport, PowerAction, Provisioning, SandboxCreate, SandboxDetail, SandboxQuery,
    SandboxStamp, SandboxView,
};
use crate::policy::PolicyDocument;

const PROVISIONING_FAILED: &str = "ProvisioningFailed";
const READY_CONDITION: &str = "Ready";
const PAGE_SIZE: i32 = 100;
const MAX_PAGES: usize = 1_000;
const MAX_RECORDS: usize = 10_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

fn protocol(message: impl ToString) -> GatewayError {
    GatewayError::Protocol {
        message: message.to_string(),
    }
}

struct Listing {
    records: usize,
    bytes: usize,
}

impl Listing {
    fn new() -> Listing {
        Listing {
            records: 0,
            bytes: 0,
        }
    }

    fn add(&mut self, bytes: usize) -> Result<(), GatewayError> {
        self.records += 1;
        self.bytes += bytes;
        if self.records > MAX_RECORDS {
            return Err(protocol("the listing has too many records"));
        }
        if self.bytes > MAX_BYTES {
            return Err(protocol("the listing is too large"));
        }
        Ok(())
    }
}

fn view_bytes(view: &SandboxView) -> usize {
    let labels: usize = view
        .labels
        .iter()
        .map(|(key, value)| key.as_str().len() + value.as_str().len())
        .sum();
    let message = match &view.provisioning {
        Provisioning::Normal => 0,
        Provisioning::Failed { message } => message.as_str().len(),
    };
    view.name.as_str().len() + labels + message
}

fn phase_of(sandbox: &Sandbox) -> Result<Phase, GatewayError> {
    let status = sandbox.status.as_ref();
    let exit_code = status.and_then(|status| status.exit_code);
    Ok(
        match SandboxPhase::try_from(status.map_or(0, |status| status.phase)) {
            Ok(SandboxPhase::Provisioning) => Phase::Provisioning,
            Ok(SandboxPhase::Ready) => Phase::Ready,
            Ok(SandboxPhase::Error) => Phase::Error { exit_code },
            Ok(SandboxPhase::Deleting) => Phase::Deleting,
            Ok(SandboxPhase::Stopping) => Phase::Stopping,
            Ok(SandboxPhase::Stopped) => Phase::Stopped,
            Ok(SandboxPhase::Starting) => Phase::Starting,
            Ok(SandboxPhase::Completed) => Phase::Completed {
                exit_code: exit_code
                    .ok_or_else(|| protocol("a completed sandbox has no exit code"))?,
            },
            Ok(SandboxPhase::Suspending) => Phase::Suspending,
            Ok(SandboxPhase::Suspended) => Phase::Suspended,
            Ok(SandboxPhase::Unspecified | SandboxPhase::Unknown) | Err(_) => Phase::Unknown,
        },
    )
}

fn provisioning_of(sandbox: &Sandbox) -> Provisioning {
    sandbox
        .status
        .as_ref()
        .and_then(|status| {
            status.conditions.iter().find(|condition| {
                condition.r#type == READY_CONDITION && condition.reason == PROVISIONING_FAILED
            })
        })
        .map_or(Provisioning::Normal, |condition| Provisioning::Failed {
            message: TerminalText::sanitize(condition.message.as_bytes()),
        })
}

pub(super) fn view_of(sandbox: &Sandbox) -> Result<SandboxView, GatewayError> {
    view_with(sandbox, |pairs| Labels::from_pairs(pairs).map_err(protocol))
}

fn listed_view_of(sandbox: &Sandbox) -> Result<SandboxView, GatewayError> {
    view_with(sandbox, |pairs| Ok(Labels::from_valid_pairs(pairs)))
}

fn view_with(
    sandbox: &Sandbox,
    labels: impl FnOnce(HashMap<String, String>) -> Result<Labels, GatewayError>,
) -> Result<SandboxView, GatewayError> {
    let metadata = sandbox
        .metadata
        .as_ref()
        .ok_or_else(|| protocol("a sandbox has no metadata"))?;
    let name = SandboxName::parse(&metadata.name).map_err(protocol)?;
    let labels = labels(metadata.labels.clone())?;
    Ok(SandboxView {
        name,
        phase: phase_of(sandbox)?,
        labels,
        provisioning: provisioning_of(sandbox),
        created_at: metadata.created_time.as_ref().map(|created| SandboxStamp {
            seconds: created.seconds,
            nanos: u32::try_from(created.nanos).unwrap_or(0),
        }),
    })
}

fn response_view(
    response: openshell_sdk::raw::proto::SandboxResponse,
) -> Result<SandboxView, GatewayError> {
    let sandbox = response
        .sandbox
        .ok_or_else(|| protocol("the response has no sandbox"))?;
    view_of(&sandbox)
}

fn create_request(
    spec: &SandboxCreate,
    scope: openshell_sdk::raw::proto::WorkspaceSelector,
) -> CreateSandboxRequest {
    let main = spec.main();
    let mut command = vec![main.program.as_str().to_owned()];
    command.extend(main.args.iter().cloned());
    let environment: HashMap<String, String> = spec
        .environment()
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    let labels: HashMap<String, String> = spec
        .labels()
        .to_labels()
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    CreateSandboxRequest {
        workspace_scope: Some(scope),
        spec: Some(SandboxSpec {
            environment,
            template: Some(SandboxTemplate {
                image: spec.image().to_string(),
                ..SandboxTemplate::default()
            }),
            policy: Some(spec.policy().proto().clone()),
            providers: spec
                .providers()
                .iter()
                .map(ProviderName::to_string)
                .collect(),
            command,
            tty: true,
            restart_policy: SandboxRestartPolicy::Never as i32,
            ..SandboxSpec::default()
        }),
        name: spec.name().to_string(),
        labels,
        ..CreateSandboxRequest::default()
    }
}

impl Inner {
    pub(crate) async fn create_sandbox(
        &self,
        spec: &SandboxCreate,
    ) -> Result<SandboxView, GatewayError> {
        let request = create_request(spec, self.scope());
        let response = self
            .unary(
                GatewayOp::Create,
                Some(Missing::Sandbox(spec.name().clone())),
                self.grpc().create_sandbox(request),
            )
            .await?;
        response_view(response)
    }

    pub(crate) async fn get_sandbox_view(
        &self,
        name: &SandboxName,
    ) -> Result<SandboxView, GatewayError> {
        let response = self
            .unary(
                GatewayOp::Detail,
                Some(Missing::Sandbox(name.clone())),
                self.grpc().get_sandbox(GetSandboxRequest {
                    workspace_scope: Some(self.scope()),
                    name: name.to_string(),
                }),
            )
            .await?;
        response_view(response)
    }

    pub(crate) async fn sandbox_environment(
        &self,
        name: &SandboxName,
    ) -> Result<BTreeMap<EnvName, String>, GatewayError> {
        let response = self
            .unary(
                GatewayOp::Detail,
                Some(Missing::Sandbox(name.clone())),
                self.grpc().get_sandbox(GetSandboxRequest {
                    workspace_scope: Some(self.scope()),
                    name: name.to_string(),
                }),
            )
            .await?;
        let spec = response
            .sandbox
            .and_then(|sandbox| sandbox.spec)
            .ok_or_else(|| protocol("the sandbox record has no spec"))?;
        spec.environment
            .into_iter()
            .map(|(key, value)| Ok((EnvName::parse(&key).map_err(protocol)?, value)))
            .collect()
    }

    pub(crate) async fn find_sandboxes(
        &self,
        query: &SandboxQuery,
    ) -> Result<Vec<SandboxView>, GatewayError> {
        match query {
            SandboxQuery::Name(name) => match self.get_sandbox_view(name).await {
                Ok(view) => Ok(vec![view]),
                Err(GatewayError::NotFound { .. }) => Ok(Vec::new()),
                Err(error) => Err(error),
            },
            SandboxQuery::Labels(selector) => self.list_by_selector(&selector.render()).await,
        }
    }

    async fn list_by_selector(&self, selector: &str) -> Result<Vec<SandboxView>, GatewayError> {
        let mut views = Vec::new();
        let mut listing = Listing::new();
        let mut token = String::new();
        for _ in 0..MAX_PAGES {
            let page = self
                .unary(
                    GatewayOp::Find,
                    None,
                    self.grpc().list_sandboxes(ListSandboxesRequest {
                        workspace_scope: Some(self.scope()),
                        page_size: PAGE_SIZE,
                        page_token: token.clone(),
                        label_selector: selector.to_owned(),
                    }),
                )
                .await?;
            for sandbox in &page.sandboxes {
                let view = listed_view_of(sandbox)?;
                listing.add(view_bytes(&view))?;
                views.push(view);
            }
            if page.next_page_token.is_empty() {
                return Ok(views);
            }
            if page.next_page_token == token {
                return Err(protocol("the gateway repeated a page token"));
            }
            token = page.next_page_token;
        }
        Err(protocol("the sandbox listing has too many pages"))
    }

    pub(crate) async fn sandbox_detail(
        &self,
        name: &SandboxName,
    ) -> Result<SandboxDetail, GatewayError> {
        let response = self
            .unary(
                GatewayOp::Detail,
                Some(Missing::Sandbox(name.clone())),
                self.grpc().get_sandbox(GetSandboxRequest {
                    workspace_scope: Some(self.scope()),
                    name: name.to_string(),
                }),
            )
            .await?;
        let sandbox = response
            .sandbox
            .ok_or_else(|| protocol("the response has no sandbox"))?;
        let view = view_of(&sandbox)?;
        let config = self
            .unary(
                GatewayOp::Detail,
                Some(Missing::Sandbox(name.clone())),
                self.grpc().get_sandbox_config(GetSandboxConfigRequest {
                    workspace_scope: Some(self.scope()),
                    name: name.to_string(),
                }),
            )
            .await?;
        let proto = config
            .policy
            .ok_or_else(|| protocol("the sandbox has no active policy"))?;
        let document = PolicyDocument::from_proto(&proto).map_err(protocol)?;
        let loaded = self.policy_loaded(name).await?;
        let policy = PolicyReport {
            version: config.version,
            hash: config.policy_hash,
            loaded,
            document,
        };
        let providers = self.attached_providers(name).await?;
        Ok(SandboxDetail {
            view,
            providers,
            policy,
        })
    }

    async fn policy_loaded(&self, name: &SandboxName) -> Result<bool, GatewayError> {
        let status = self
            .unary(
                GatewayOp::Detail,
                Some(Missing::Sandbox(name.clone())),
                self.grpc()
                    .get_sandbox_policy_status(GetSandboxPolicyStatusRequest {
                        workspace_scope: Some(self.scope()),
                        sandbox: name.to_string(),
                        version: 0,
                        global: false,
                    }),
            )
            .await;
        match status {
            Ok(response) => Ok(response.revision.is_some_and(|revision| {
                revision.status == PolicyStatus::Loaded as i32
                    && response.active_version == revision.version
            })),
            Err(GatewayError::NotFound { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn attached_providers(
        &self,
        name: &SandboxName,
    ) -> Result<Vec<ProviderName>, GatewayError> {
        let mut providers = Vec::new();
        let mut listing = Listing::new();
        let mut token = String::new();
        for _ in 0..MAX_PAGES {
            let page = self
                .unary(
                    GatewayOp::Detail,
                    Some(Missing::Sandbox(name.clone())),
                    self.grpc()
                        .list_sandbox_providers(ListSandboxProvidersRequest {
                            workspace_scope: Some(self.scope()),
                            sandbox: name.to_string(),
                            page_size: PAGE_SIZE,
                            page_token: token.clone(),
                        }),
                )
                .await?;
            for provider in &page.providers {
                let name = provider
                    .metadata
                    .as_ref()
                    .map_or("", |meta| meta.name.as_str());
                let name = ProviderName::parse(name).map_err(protocol)?;
                listing.add(name.as_str().len())?;
                providers.push(name);
            }
            if page.next_page_token.is_empty() {
                return Ok(providers);
            }
            if page.next_page_token == token {
                return Err(protocol("the gateway repeated a page token"));
            }
            token = page.next_page_token;
        }
        Err(protocol("the provider listing has too many pages"))
    }

    pub(crate) async fn set_power(
        &self,
        name: &SandboxName,
        action: PowerAction,
    ) -> Result<SandboxView, GatewayError> {
        let missing = Some(Missing::Sandbox(name.clone()));
        let result = match action {
            PowerAction::Stop => {
                self.unary(
                    GatewayOp::Power,
                    missing,
                    self.grpc().stop_sandbox(StopSandboxRequest {
                        workspace_scope: Some(self.scope()),
                        name: name.to_string(),
                        request_id: String::new(),
                    }),
                )
                .await
            }
            PowerAction::Start => {
                self.unary(
                    GatewayOp::Power,
                    missing,
                    self.grpc().start_sandbox(StartSandboxRequest {
                        workspace_scope: Some(self.scope()),
                        name: name.to_string(),
                        request_id: String::new(),
                    }),
                )
                .await
            }
        };
        let view = response_view(result?)?;
        if action == PowerAction::Stop && view.phase.main_process_ended() {
            return Err(GatewayError::MainProcessFailed {
                sandbox: name.clone(),
            });
        }
        Ok(view)
    }

    pub(crate) async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError> {
        self.unary(
            GatewayOp::Delete,
            Some(Missing::Sandbox(name.clone())),
            self.grpc().delete_sandbox(DeleteSandboxRequest {
                workspace_scope: Some(self.scope()),
                name: name.to_string(),
                allow_missing: true,
                request_id: String::new(),
            }),
        )
        .await
        .map(|_| ())
        .or_else(|error| match error {
            GatewayError::NotFound { .. } => Ok(()),
            other => Err(other),
        })
    }
}
