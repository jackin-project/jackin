// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use openshell_sdk::raw::proto::{
    ConfigureProviderRefreshRequest, ConfigureProviderRefreshResponse, CreateProviderRequest,
    DeleteProviderProfileRequest, DeleteProviderProfileResponse, DeleteProviderRefreshRequest,
    DeleteProviderRefreshResponse, DeleteProviderRequest, DeleteProviderResponse, DeletionOutcome,
    GetProviderProfileRequest, GetProviderRequest, ImportProviderProfilesRequest,
    ImportProviderProfilesResponse, ProviderProfileDiagnostic, ProviderProfileResponse,
    ProviderResponse, UpdateProviderProfilesRequest, UpdateProviderProfilesResponse,
    UpdateProviderRequest,
};
use tonic::{Request, Response, Status};

use super::server::{Broken, Fake, RefreshRecord, scope_name};

fn provider_name(provider: &openshell_sdk::raw::proto::Provider) -> String {
    provider
        .metadata
        .as_ref()
        .map(|meta| meta.name.clone())
        .unwrap_or_default()
}

fn redacted(
    mut provider: openshell_sdk::raw::proto::Provider,
) -> openshell_sdk::raw::proto::Provider {
    for value in provider.credentials.values_mut() {
        "REDACTED".clone_into(value);
    }
    provider
}

fn diagnostic(message: &str) -> ProviderProfileDiagnostic {
    ProviderProfileDiagnostic {
        message: message.to_owned(),
        field: "id".to_owned(),
        ..ProviderProfileDiagnostic::default()
    }
}

impl Fake {
    pub(crate) async fn get_provider_profile_impl(
        &self,
        request: Request<GetProviderProfileRequest>,
    ) -> Result<Response<ProviderProfileResponse>, Status> {
        self.enter("get_provider_profile").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.id);
        let mut state = self.shared.lock();
        let profile = state
            .profiles
            .get(&key)
            .cloned()
            .ok_or_else(|| Status::not_found("provider profile not found"))?;
        if let Some(mut hook) = state.after_get_profile.take() {
            hook(&mut state);
        }
        Ok(Response::new(ProviderProfileResponse {
            profile: Some(profile),
        }))
    }

    pub(crate) async fn import_provider_profiles_impl(
        &self,
        request: Request<ImportProviderProfilesRequest>,
    ) -> Result<Response<ImportProviderProfilesResponse>, Status> {
        self.enter("import_provider_profiles").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let mut state = self.shared.lock();
        if let Some(mut hook) = state.before_import.take() {
            hook(&mut state);
        }
        if let Some((field, message)) = state.import_rejection.clone() {
            return Ok(Response::new(ImportProviderProfilesResponse {
                diagnostics: vec![ProviderProfileDiagnostic {
                    message,
                    field,
                    ..ProviderProfileDiagnostic::default()
                }],
                profiles: Vec::new(),
                imported: false,
            }));
        }
        let mut stored = Vec::new();
        for item in request.profiles {
            let mut profile = item.profile.unwrap_or_default();
            let key = (workspace.clone(), profile.id.clone());
            if state.profiles.contains_key(&key) {
                return Ok(Response::new(ImportProviderProfilesResponse {
                    diagnostics: vec![diagnostic("custom provider profile already exists")],
                    profiles: Vec::new(),
                    imported: false,
                }));
            }
            profile.resource_version = 1;
            "user".clone_into(&mut profile.source);
            state.profiles.insert(key, profile.clone());
            stored.push(profile);
        }
        Ok(Response::new(ImportProviderProfilesResponse {
            diagnostics: Vec::new(),
            profiles: stored,
            imported: true,
        }))
    }

    pub(crate) async fn update_provider_profiles_impl(
        &self,
        request: Request<UpdateProviderProfilesRequest>,
    ) -> Result<Response<UpdateProviderProfilesResponse>, Status> {
        self.enter("update_provider_profiles").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.id);
        if request.expected_resource_version == 0 {
            return Err(Status::invalid_argument(
                "updates without a non-zero version are rejected",
            ));
        }
        let mut state = self.shared.lock();
        let current = state
            .profiles
            .get(&key)
            .cloned()
            .ok_or_else(|| Status::not_found("provider profile not found"))?;
        if current.resource_version != request.expected_resource_version {
            return Err(Status::aborted("resource version conflict"));
        }
        let mut profile = request
            .profile
            .and_then(|item| item.profile)
            .unwrap_or_default();
        profile.resource_version = current.resource_version + 1;
        "user".clone_into(&mut profile.source);
        state.profiles.insert(key, profile.clone());
        state.profile_updates += 1;
        Ok(Response::new(UpdateProviderProfilesResponse {
            diagnostics: Vec::new(),
            profile: Some(profile),
            updated: true,
        }))
    }

    pub(crate) async fn create_provider_impl(
        &self,
        request: Request<CreateProviderRequest>,
    ) -> Result<Response<ProviderResponse>, Status> {
        self.enter("create_provider").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let provider = request.provider.unwrap_or_default();
        let name = provider_name(&provider);
        let mut state = self.shared.lock();
        if let Some(mut hook) = state.before_create_provider.take() {
            hook(&mut state);
        }
        let profile_key = (provider.profile_workspace.clone(), provider.r#type.clone());
        if !state.profiles.contains_key(&profile_key) {
            return Err(Status::invalid_argument(format!(
                "provider profile '{}' was not found in the requested scope",
                provider.r#type
            )));
        }
        let key = (workspace, name);
        if state.providers.contains_key(&key) {
            return Err(Status::already_exists("provider already exists"));
        }
        state.providers.insert(key, provider.clone());
        Ok(Response::new(ProviderResponse {
            provider: Some(redacted(provider)),
            ..ProviderResponse::default()
        }))
    }

    pub(crate) async fn get_provider_impl(
        &self,
        request: Request<GetProviderRequest>,
    ) -> Result<Response<ProviderResponse>, Status> {
        self.enter("get_provider").await?;
        let request = request.into_inner();
        let key = (scope_name(request.workspace_scope.as_ref())?, request.name);
        let state = self.shared.lock();
        let provider = state
            .providers
            .get(&key)
            .cloned()
            .ok_or_else(|| Status::not_found("provider not found"))?;
        Ok(Response::new(ProviderResponse {
            provider: Some(redacted(provider)),
            ..ProviderResponse::default()
        }))
    }

    pub(crate) async fn update_provider_impl(
        &self,
        request: Request<UpdateProviderRequest>,
    ) -> Result<Response<ProviderResponse>, Status> {
        self.enter("update_provider").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let incoming = request.provider.unwrap_or_default();
        let name = provider_name(&incoming);
        let mut state = self.shared.lock();
        let key = (workspace.clone(), name.clone());
        if !state.providers.contains_key(&key) {
            return Err(Status::not_found("provider not found"));
        }
        let owned: Vec<String> = state
            .refresh
            .keys()
            .filter(|(ws, provider, _)| *ws == workspace && *provider == name)
            .map(|(_, _, credential)| credential.clone())
            .filter(|credential| incoming.credentials.contains_key(credential))
            .collect();
        if !owned.is_empty() {
            return Err(Status::failed_precondition(format!(
                "credentials managed by provider refresh cannot be updated or deleted with provider update: {}",
                owned.join(", ")
            )));
        }
        let Some(existing) = state.providers.get_mut(&key) else {
            return Err(Status::not_found("provider not found"));
        };
        for (credential, value) in incoming.credentials {
            if value.is_empty() {
                existing.credentials.remove(&credential);
            } else {
                existing.credentials.insert(credential, value);
            }
        }
        let provider = existing.clone();
        Ok(Response::new(ProviderResponse {
            provider: Some(redacted(provider)),
            ..ProviderResponse::default()
        }))
    }

    pub(crate) async fn delete_provider_impl(
        &self,
        request: Request<DeleteProviderRequest>,
    ) -> Result<Response<DeleteProviderResponse>, Status> {
        self.enter("delete_provider").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let mut state = self.shared.lock();
        let key = (workspace.clone(), request.name.clone());
        if !state.providers.contains_key(&key) {
            return Err(
                if state.broken == Some(Broken::DeleteAbsentRefused)
                    && state.deleted_providers.contains(&key)
                {
                    Status::failed_precondition("provider was deleted before")
                } else {
                    Status::not_found("provider not found")
                },
            );
        }
        let attached: Vec<&str> = state
            .sandboxes
            .iter()
            .filter(|((sandbox_workspace, _), sandbox)| {
                *sandbox_workspace == workspace
                    && sandbox
                        .spec
                        .as_ref()
                        .is_some_and(|spec| spec.providers.contains(&request.name))
            })
            .map(|((_, name), _)| name.as_str())
            .collect();
        let refusal = (!attached.is_empty()
            && state.broken != Some(Broken::DeleteAttachedProviders))
        .then(|| {
            Status::failed_precondition(format!(
                "provider '{}' is attached to sandbox(es): {}",
                request.name,
                attached.join(", ")
            ))
        });
        let deletes_anyway = state.broken == Some(Broken::DeleteAfterRefusal);
        match refusal {
            Some(status) if !deletes_anyway => Err(status),
            refusal => {
                state.providers.remove(&key);
                state.deleted_providers.insert(key.clone());
                state.refresh.retain(|(ws, provider, _), _| {
                    !(*ws == workspace && *provider == request.name)
                });
                refusal.map_or_else(
                    || {
                        Ok(Response::new(DeleteProviderResponse {
                            outcome: DeletionOutcome::Completed as i32,
                        }))
                    },
                    Err,
                )
            }
        }
    }

    pub(crate) async fn configure_provider_refresh_impl(
        &self,
        request: Request<ConfigureProviderRefreshRequest>,
    ) -> Result<Response<ConfigureProviderRefreshResponse>, Status> {
        self.enter("configure_provider_refresh").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        if request.material.contains_key("token_url") {
            return Err(Status::invalid_argument(
                "refresh token endpoints must be defined by the provider profile, not material",
            ));
        }
        for required in ["client_id", "refresh_token"] {
            if !request.material.contains_key(required) {
                return Err(Status::invalid_argument(format!(
                    "{required} material is required by the provider profile"
                )));
            }
        }
        if request
            .secret_material_keys
            .iter()
            .any(|key| !request.material.contains_key(key))
        {
            return Err(Status::invalid_argument(
                "secret_material_keys entry is not present in material",
            ));
        }
        let mut state = self.shared.lock();
        if !state
            .providers
            .contains_key(&(workspace.clone(), request.provider.clone()))
        {
            return Err(Status::not_found("provider not found"));
        }
        state.refresh.insert(
            (workspace, request.provider, request.credential_key),
            RefreshRecord {
                strategy: request.strategy,
                material: request.material,
                secret_keys: request.secret_material_keys,
            },
        );
        Ok(Response::new(ConfigureProviderRefreshResponse::default()))
    }

    pub(crate) async fn delete_provider_refresh_impl(
        &self,
        request: Request<DeleteProviderRefreshRequest>,
    ) -> Result<Response<DeleteProviderRefreshResponse>, Status> {
        self.enter("delete_provider_refresh").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let mut state = self.shared.lock();
        if !state
            .providers
            .contains_key(&(workspace.clone(), request.provider.clone()))
        {
            return Err(Status::not_found("provider not found"));
        }
        state
            .refresh
            .remove(&(workspace, request.provider, request.credential_key));
        Ok(Response::new(DeleteProviderRefreshResponse {
            outcome: DeletionOutcome::Completed as i32,
        }))
    }

    pub(crate) async fn delete_provider_profile_impl(
        &self,
        request: Request<DeleteProviderProfileRequest>,
    ) -> Result<Response<DeleteProviderProfileResponse>, Status> {
        self.enter("delete_provider_profile").await?;
        let request = request.into_inner();
        let workspace = scope_name(request.workspace_scope.as_ref())?;
        let key = (workspace.clone(), request.id.clone());
        let mut state = self.shared.lock();
        if !state.profiles.contains_key(&key) {
            return if request.allow_missing {
                Ok(Response::new(DeleteProviderProfileResponse {
                    outcome: DeletionOutcome::AlreadyAbsent as i32,
                }))
            } else {
                Err(Status::not_found("provider profile not found"))
            };
        }
        let mut blocking: Vec<String> = state
            .providers
            .iter()
            .filter(|((provider_workspace, _), provider)| {
                *provider_workspace == workspace
                    && provider.profile_workspace == workspace
                    && provider.r#type == request.id
            })
            .map(|((_, name), _)| name.clone())
            .collect();
        blocking.sort();
        if !blocking.is_empty() {
            return Err(Status::failed_precondition(format!(
                "provider profile '{}' is in use by providers: {}",
                request.id,
                blocking.join(", ")
            )));
        }
        state.profiles.remove(&key);
        Ok(Response::new(DeleteProviderProfileResponse {
            outcome: DeletionOutcome::Completed as i32,
        }))
    }
}
