// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use openshell_sdk::raw::proto::{
    CreateSandboxRequest, DeleteSandboxRequest, DeleteSandboxResponse, DeletionOutcome,
    GetSandboxConfigRequest, GetSandboxConfigResponse, GetSandboxPolicyStatusRequest,
    GetSandboxPolicyStatusResponse, GetSandboxRequest, HealthRequest, HealthResponse,
    ListSandboxProvidersRequest, ListSandboxProvidersResponse, ListSandboxesRequest,
    ListSandboxesResponse, ObjectMeta, PolicyStatus, Sandbox, SandboxCondition, SandboxPhase,
    SandboxPolicy, SandboxPolicyRevision, SandboxResponse, SandboxStatus, ServiceStatus,
    StartSandboxRequest, StopSandboxRequest,
};
use tonic::{Request, Response, Status};

use super::server::{
    Broken, Fake, Key, State, parse_selector, scope_name, validate_label_key, validate_label_value,
    validate_sandbox_name,
};

pub(crate) const CREATED_MILLIS: i64 = 1_700_000_000_000;

pub(crate) fn phase_of(sandbox: &Sandbox) -> i32 {
    sandbox.status.as_ref().map_or(0, |status| status.phase)
}

pub(crate) fn set_phase(sandbox: &mut Sandbox, phase: SandboxPhase) {
    sandbox
        .status
        .get_or_insert_with(SandboxStatus::default)
        .phase = phase as i32;
}

pub(crate) fn condition(kind: &str, status: &str, reason: &str, message: &str) -> SandboxCondition {
    SandboxCondition {
        r#type: kind.to_owned(),
        status: status.to_owned(),
        reason: reason.to_owned(),
        message: message.to_owned(),
        ..SandboxCondition::default()
    }
}

fn advance(state: &mut State, key: &Key) {
    if !state.auto_advance {
        return;
    }
    let known = state.known_images.clone();
    let Some(sandbox) = state.sandboxes.get_mut(key) else {
        return;
    };
    let image = sandbox
        .spec
        .as_ref()
        .and_then(|spec| spec.template.as_ref())
        .map(|template| template.image.clone())
        .unwrap_or_default();
    let phase = phase_of(sandbox);
    let exit = sandbox
        .spec
        .as_ref()
        .and_then(|spec| exiting_main(&spec.command));
    if phase == SandboxPhase::Provisioning as i32
        && exit.is_some()
        && known.as_ref().is_none_or(|known| known.contains(&image))
    {
        let code = exit.unwrap_or_default();
        let status = sandbox.status.get_or_insert_with(SandboxStatus::default);
        if code == 0 {
            status.phase = SandboxPhase::Completed as i32;
        } else {
            status.phase = SandboxPhase::Error as i32;
        }
        status.exit_code = Some(code);
    } else if phase == SandboxPhase::Provisioning as i32 {
        if known.is_none_or(|known| known.contains(&image)) {
            set_phase(sandbox, SandboxPhase::Ready);
        } else {
            set_phase(sandbox, SandboxPhase::Error);
            let status = sandbox.status.get_or_insert_with(SandboxStatus::default);
            status.conditions = vec![
                condition(
                    "ConfigurationReady",
                    "False",
                    "ConfigurationPending",
                    "Waiting for effective configuration validation",
                ),
                condition(
                    "Ready",
                    "False",
                    "ProvisioningFailed",
                    &format!("failed to resolve vm sandbox image '{image}'"),
                ),
            ];
        }
    } else if phase == SandboxPhase::Starting as i32 {
        set_phase(sandbox, SandboxPhase::Ready);
    }
}

fn exiting_main(command: &[String]) -> Option<i32> {
    match command {
        [shell, flag, script, ..] if shell == "/bin/sh" && flag == "-c" => {
            script.strip_prefix("exit ")?.trim().parse().ok()
        }
        _ => None,
    }
}

fn selected(state: &State, key: &Key) -> Result<Sandbox, Status> {
    state
        .sandboxes
        .get(key)
        .cloned()
        .ok_or_else(|| Status::not_found("sandbox not found"))
}

fn page_of(token: &str) -> usize {
    token.parse().unwrap_or(0)
}

impl Fake {
    pub(crate) async fn health_impl(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        self.enter("health").await?;
        Ok(Response::new(HealthResponse {
            status: ServiceStatus::Healthy as i32,
            version: "0.1.7".to_owned(),
        }))
    }

    pub(crate) async fn create_sandbox_impl(
        &self,
        request: Request<CreateSandboxRequest>,
    ) -> Result<Response<SandboxResponse>, Status> {
        self.enter("create_sandbox").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        validate_sandbox_name(&request.name)?;
        for (key, value) in &request.labels {
            validate_label_key(key)?;
            validate_label_value(value)?;
        }
        let mut state = self.shared.lock();
        let key = (workspace.clone(), request.name.clone());
        if state.sandboxes.contains_key(&key) {
            return Err(Status::already_exists("sandbox already exists"));
        }
        let labels = if state.store_labels {
            request.labels.clone()
        } else {
            std::collections::HashMap::default()
        };
        let sandbox = Sandbox {
            metadata: Some(ObjectMeta {
                id: Fake::next_id(&mut state),
                name: request.name.clone(),
                labels,
                workspace,
                created_time: openshell_core::time::timestamp_from_millis(CREATED_MILLIS).ok(),
                ..ObjectMeta::default()
            }),
            spec: request.spec.clone().map(|mut spec| {
                if !state.store_environment {
                    spec.environment.clear();
                }
                spec
            }),
            status: Some(SandboxStatus {
                phase: SandboxPhase::Provisioning as i32,
                ..SandboxStatus::default()
            }),
            ..Sandbox::default()
        };
        state.sandboxes.insert(key, sandbox.clone());
        Ok(Response::new(SandboxResponse {
            sandbox: Some(sandbox),
            ..SandboxResponse::default()
        }))
    }

    pub(crate) async fn get_sandbox_impl(
        &self,
        request: Request<GetSandboxRequest>,
    ) -> Result<Response<SandboxResponse>, Status> {
        self.enter("get_sandbox").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let mut state = self.shared.lock();
        advance(&mut state, &key);
        Ok(Response::new(SandboxResponse {
            sandbox: Some(selected(&state, &key)?),
            ..SandboxResponse::default()
        }))
    }

    pub(crate) async fn list_sandboxes_impl(
        &self,
        request: Request<ListSandboxesRequest>,
    ) -> Result<Response<ListSandboxesResponse>, Status> {
        self.enter("list_sandboxes").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let wanted = parse_selector(&request.label_selector)?;
        let mut state = self.shared.lock();
        let keys: Vec<Key> = state
            .sandboxes
            .keys()
            .filter(|key| key.0 == workspace)
            .cloned()
            .collect();
        for key in &keys {
            advance(&mut state, key);
        }
        let matching: Vec<Sandbox> = keys
            .iter()
            .filter_map(|key| state.sandboxes.get(key))
            .filter(|sandbox| {
                let labels = sandbox.metadata.as_ref().map(|meta| &meta.labels);
                wanted
                    .iter()
                    .all(|(key, value)| labels.is_some_and(|labels| labels.get(key) == Some(value)))
            })
            .cloned()
            .collect();
        let start = page_of(&request.page_token);
        let end = (start + state.page_cap).min(matching.len());
        let next = if end < matching.len() {
            end.to_string()
        } else {
            String::new()
        };
        Ok(Response::new(ListSandboxesResponse {
            sandboxes: matching[start.min(end)..end].to_vec(),
            next_page_token: next,
        }))
    }

    pub(crate) async fn list_sandbox_providers_impl(
        &self,
        request: Request<ListSandboxProvidersRequest>,
    ) -> Result<Response<ListSandboxProvidersResponse>, Status> {
        self.enter("list_sandbox_providers").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let state = self.shared.lock();
        let sandbox = selected(&state, &(workspace.clone(), request.sandbox))?;
        let names = sandbox.spec.map(|spec| spec.providers).unwrap_or_default();
        let providers: Vec<_> = names
            .iter()
            .filter_map(|name| state.providers.get(&(workspace.clone(), name.clone())))
            .cloned()
            .collect();
        let start = page_of(&request.page_token);
        let end = (start + state.page_cap).min(providers.len());
        let next = if end < providers.len() {
            end.to_string()
        } else {
            String::new()
        };
        Ok(Response::new(ListSandboxProvidersResponse {
            providers: providers[start.min(end)..end].to_vec(),
            next_page_token: next,
        }))
    }

    pub(crate) async fn delete_sandbox_impl(
        &self,
        request: Request<DeleteSandboxRequest>,
    ) -> Result<Response<DeleteSandboxResponse>, Status> {
        self.enter("delete_sandbox").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let mut state = self.shared.lock();
        if state.sandboxes.remove(&key).is_none() {
            return Err(Status::not_found("sandbox not found"));
        }
        Ok(Response::new(DeleteSandboxResponse {
            outcome: DeletionOutcome::Completed as i32,
            ..DeleteSandboxResponse::default()
        }))
    }

    pub(crate) async fn stop_sandbox_impl(
        &self,
        request: Request<StopSandboxRequest>,
    ) -> Result<Response<SandboxResponse>, Status> {
        self.enter("stop_sandbox").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let mut state = self.shared.lock();
        let mut sandbox = selected(&state, &key)?;
        let phase = phase_of(&sandbox);
        if phase == SandboxPhase::Stopped as i32 && state.broken == Some(Broken::StopOnStopped) {
            return Err(Status::failed_precondition(
                "sandbox must be Ready, Suspended, Error, or in an automatic restart to stop",
            ));
        }
        if phase == SandboxPhase::Ready as i32 {
            set_phase(&mut sandbox, SandboxPhase::Stopped);
            state.sandboxes.insert(key, sandbox.clone());
        } else if ![
            SandboxPhase::Stopped as i32,
            SandboxPhase::Completed as i32,
            SandboxPhase::Error as i32,
        ]
        .contains(&phase)
        {
            return Err(Status::failed_precondition(
                "sandbox must be Ready, Suspended, Error, or in an automatic restart to stop",
            ));
        }
        Ok(Response::new(SandboxResponse {
            sandbox: Some(sandbox),
            ..SandboxResponse::default()
        }))
    }

    pub(crate) async fn start_sandbox_impl(
        &self,
        request: Request<StartSandboxRequest>,
    ) -> Result<Response<SandboxResponse>, Status> {
        self.enter("start_sandbox").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let mut state = self.shared.lock();
        let mut sandbox = selected(&state, &key)?;
        let phase = phase_of(&sandbox);
        if phase == SandboxPhase::Stopped as i32 {
            set_phase(&mut sandbox, SandboxPhase::Starting);
            state.sandboxes.insert(key, sandbox.clone());
            return Ok(Response::new(SandboxResponse {
                sandbox: Some(sandbox),
                ..SandboxResponse::default()
            }));
        }
        if phase == SandboxPhase::Error as i32 || phase == SandboxPhase::Completed as i32 {
            set_phase(&mut sandbox, SandboxPhase::Starting);
            if let Some(status) = sandbox.status.as_mut() {
                status.exit_code = None;
            }
            state.sandboxes.insert(key, sandbox);
            return Err(Status::failed_precondition(
                "start sandbox failed: VM sandbox is already running generation e8f2e798",
            ));
        }
        Err(Status::failed_precondition("sandbox cannot be started"))
    }

    pub(crate) async fn get_sandbox_config_impl(
        &self,
        request: Request<GetSandboxConfigRequest>,
    ) -> Result<Response<GetSandboxConfigResponse>, Status> {
        self.enter("get_sandbox_config").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let state = self.shared.lock();
        let sandbox = selected(&state, &key)?;
        let info = state.policy_info.get(&key).cloned().unwrap_or_default();
        let mut policy = sandbox
            .spec
            .and_then(|spec| spec.policy)
            .unwrap_or_else(|| SandboxPolicy {
                version: 1,
                ..SandboxPolicy::default()
            });
        for (name, rule) in info.extra_rules {
            policy.network_policies.insert(name, rule);
        }
        Ok(Response::new(GetSandboxConfigResponse {
            policy: Some(policy),
            version: info.version,
            policy_hash: info.hash,
            ..GetSandboxConfigResponse::default()
        }))
    }

    pub(crate) async fn get_sandbox_policy_status_impl(
        &self,
        request: Request<GetSandboxPolicyStatusRequest>,
    ) -> Result<Response<GetSandboxPolicyStatusResponse>, Status> {
        self.enter("get_sandbox_policy_status").await?;
        let request = request.into_inner();
        let key = (
            scope_name(request.workspace_scope.as_ref())?,
            request.sandbox,
        );
        let state = self.shared.lock();
        let sandbox = selected(&state, &key)?;
        let info = state.policy_info.get(&key).cloned().unwrap_or_default();
        Ok(Response::new(GetSandboxPolicyStatusResponse {
            revision: Some(SandboxPolicyRevision {
                version: info.version,
                policy_hash: format!("stored-{}", info.hash),
                status: if info.loaded {
                    PolicyStatus::Loaded as i32
                } else {
                    PolicyStatus::Pending as i32
                },
                policy: sandbox.spec.and_then(|spec| spec.policy),
                ..SandboxPolicyRevision::default()
            }),
            active_version: if info.loaded { info.version } else { 0 },
        }))
    }
}
