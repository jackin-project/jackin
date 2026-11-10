// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use jackin_core::{ProfileId, ProviderName, Redactor};
use openshell_sdk::raw::proto::{
    ConfigureProviderRefreshRequest, CreateProviderRequest, DeleteProviderProfileRequest,
    DeleteProviderRefreshRequest, DeleteProviderRequest, GetProviderProfileRequest,
    GetProviderRequest, ImportProviderProfilesRequest, ObjectMeta, Provider,
    ProviderCredentialRefreshStrategy, ProviderProfile, ProviderProfileDiagnostic,
    ProviderProfileImportItem, UpdateProviderProfilesRequest, UpdateProviderRequest,
};

use super::Inner;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::gateway::{ProfileSync, ProviderMaterial, ProviderSpec, ProviderState};
use crate::policy::ProviderProfile as ProfileModel;

const PROFILE_SOURCE: &str = "jackin";
const CLIENT_ID_MATERIAL: &str = "client_id";
const REFRESH_TOKEN_MATERIAL: &str = "refresh_token";

struct RefreshConfiguration {
    material: HashMap<String, String>,
    expires_at: Option<i64>,
}

fn normalized(mut profile: ProviderProfile) -> ProviderProfile {
    profile.resource_version = 0;
    profile.source.clear();
    profile.scope.clear();
    profile
}

fn rejected(diagnostics: &[ProviderProfileDiagnostic], what: &str) -> GatewayError {
    match diagnostics.first() {
        Some(diagnostic) => GatewayError::InvalidArgument {
            field: diagnostic.field.clone(),
            message: diagnostic.message.clone(),
        },
        None => GatewayError::Protocol {
            message: format!("the gateway did not {what} the profile"),
        },
    }
}

fn redact(error: GatewayError, redactor: &Redactor) -> GatewayError {
    match error {
        GatewayError::Unreachable { endpoint, message } => GatewayError::Unreachable {
            endpoint,
            message: redactor.apply(&message),
        },
        GatewayError::Tls { message } => GatewayError::Tls {
            message: redactor.apply(&message),
        },
        GatewayError::PermissionDenied { what } => GatewayError::PermissionDenied {
            what: redactor.apply(&what),
        },
        GatewayError::FailedPrecondition { message } => GatewayError::FailedPrecondition {
            message: redactor.apply(&message),
        },
        GatewayError::ResourceExhausted { what } => GatewayError::ResourceExhausted {
            what: redactor.apply(&what),
        },
        GatewayError::InvalidArgument { field, message } => GatewayError::InvalidArgument {
            field,
            message: redactor.apply(&message),
        },
        GatewayError::Protocol { message } => GatewayError::Protocol {
            message: redactor.apply(&message),
        },
        other => other,
    }
}

fn provider_record(name: &ProviderName, profile_type: &str, workspace: &str) -> Provider {
    Provider {
        metadata: Some(ObjectMeta {
            name: name.to_string(),
            ..ObjectMeta::default()
        }),
        r#type: profile_type.to_owned(),
        profile_workspace: workspace.to_owned(),
        ..Provider::default()
    }
}

impl Inner {
    pub(crate) async fn ensure_profile(
        &self,
        profile: &ProfileModel,
    ) -> Result<ProfileSync, GatewayError> {
        let desired = profile.to_proto().map_err(|error| GatewayError::Protocol {
            message: error.to_string(),
        })?;
        if let Some(stored) = self.stored_profile(&profile.id).await? {
            return self.converge_profile(stored, desired).await;
        }
        match self.import_profile(desired.clone()).await {
            Ok(()) => Ok(ProfileSync::Imported),
            Err(rejection @ GatewayError::InvalidArgument { .. }) => {
                match self.stored_profile(&profile.id).await? {
                    Some(stored) => self.converge_profile(stored, desired).await,
                    None => Err(rejection),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn stored_profile(
        &self,
        id: &ProfileId,
    ) -> Result<Option<ProviderProfile>, GatewayError> {
        let current = self
            .unary(
                GatewayOp::Profile,
                Some(Missing::Profile(id.clone())),
                self.grpc().get_provider_profile(GetProviderProfileRequest {
                    workspace_scope: Some(self.scope()),
                    id: id.to_string(),
                }),
            )
            .await;
        match current {
            Ok(response) => response
                .profile
                .map(Some)
                .ok_or_else(|| GatewayError::Protocol {
                    message: "the response has no profile".to_owned(),
                }),
            Err(GatewayError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn converge_profile(
        &self,
        stored: ProviderProfile,
        desired: ProviderProfile,
    ) -> Result<ProfileSync, GatewayError> {
        let version = stored.resource_version;
        if normalized(stored) == normalized(desired.clone()) {
            return Ok(ProfileSync::Unchanged);
        }
        self.update_profile(desired, version).await?;
        Ok(ProfileSync::Updated)
    }

    pub(crate) async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError> {
        self.unary(
            GatewayOp::Profile,
            Some(Missing::Profile(id.clone())),
            self.grpc()
                .delete_provider_profile(DeleteProviderProfileRequest {
                    workspace_scope: Some(self.scope()),
                    id: id.to_string(),
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

    async fn import_profile(&self, desired: ProviderProfile) -> Result<(), GatewayError> {
        let response = self
            .unary(
                GatewayOp::Profile,
                None,
                self.grpc()
                    .import_provider_profiles(ImportProviderProfilesRequest {
                        workspace_scope: Some(self.scope()),
                        profiles: vec![ProviderProfileImportItem {
                            profile: Some(desired),
                            source: PROFILE_SOURCE.to_owned(),
                        }],
                        request_id: String::new(),
                    }),
            )
            .await?;
        if response.imported {
            Ok(())
        } else {
            Err(rejected(&response.diagnostics, "import"))
        }
    }

    async fn update_profile(
        &self,
        mut desired: ProviderProfile,
        version: u64,
    ) -> Result<(), GatewayError> {
        desired.resource_version = version;
        let id = desired.id.clone();
        let response = self
            .unary(
                GatewayOp::Profile,
                None,
                self.grpc()
                    .update_provider_profiles(UpdateProviderProfilesRequest {
                        workspace_scope: Some(self.scope()),
                        profile: Some(ProviderProfileImportItem {
                            profile: Some(desired),
                            source: PROFILE_SOURCE.to_owned(),
                        }),
                        expected_resource_version: version,
                        id,
                        request_id: String::new(),
                    }),
            )
            .await?;
        if response.updated {
            Ok(())
        } else {
            Err(rejected(&response.diagnostics, "update"))
        }
    }

    pub(crate) async fn provider_state(
        &self,
        name: &ProviderName,
    ) -> Result<ProviderState, GatewayError> {
        match self.get_provider(name).await {
            Ok(_) => Ok(ProviderState::Present),
            Err(GatewayError::NotFound { .. }) => Ok(ProviderState::Absent),
            Err(error) => Err(error),
        }
    }

    async fn get_provider(&self, name: &ProviderName) -> Result<Provider, GatewayError> {
        let response = self
            .unary(
                GatewayOp::Provider,
                Some(Missing::Provider(name.clone())),
                self.grpc().get_provider(GetProviderRequest {
                    workspace_scope: Some(self.scope()),
                    name: name.to_string(),
                }),
            )
            .await?;
        response.provider.ok_or_else(|| GatewayError::Protocol {
            message: "the response has no provider".to_owned(),
        })
    }

    pub(crate) async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError> {
        let (credentials, refresh, redactor) = match &spec.material {
            ProviderMaterial::StaticKey { key } => (
                HashMap::from([(spec.keys.access_token.to_owned(), key.expose().to_owned())]),
                None,
                Redactor::new([key]),
            ),
            ProviderMaterial::OAuth2RefreshToken {
                access_token,
                refresh_token,
                client_id,
                scopes,
                expires_at,
                additional_credentials,
            } => {
                let mut credentials = HashMap::from([(
                    spec.keys.access_token.to_owned(),
                    access_token.expose().to_owned(),
                )]);
                credentials.extend(
                    additional_credentials
                        .iter()
                        .filter(|(key, value)| !key.is_empty() && !value.is_empty())
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                let mut material = HashMap::from([
                    (CLIENT_ID_MATERIAL.to_owned(), client_id.clone()),
                    (
                        REFRESH_TOKEN_MATERIAL.to_owned(),
                        refresh_token.expose().to_owned(),
                    ),
                ]);
                if !scopes.is_empty() {
                    material.insert("scope".to_owned(), scopes.join(" "));
                }
                (
                    credentials,
                    Some(RefreshConfiguration {
                        material,
                        expires_at: *expires_at,
                    }),
                    Redactor::new([access_token, refresh_token]),
                )
            }
        };
        self.unary(
            GatewayOp::Profile,
            Some(Missing::Profile(spec.profile.clone())),
            self.grpc().get_provider_profile(GetProviderProfileRequest {
                workspace_scope: Some(self.scope()),
                id: spec.profile.to_string(),
            }),
        )
        .await?;
        self.write_provider(spec, credentials, refresh)
            .await
            .map_err(|error| redact(error, &redactor))
    }

    async fn write_provider(
        &self,
        spec: &ProviderSpec,
        credentials: HashMap<String, String>,
        refresh: Option<RefreshConfiguration>,
    ) -> Result<(), GatewayError> {
        let mut record =
            provider_record(&spec.name, spec.profile.as_str(), self.workspace.as_str());
        record.credentials = credentials;
        match self.provider_state(&spec.name).await? {
            ProviderState::Absent => match self.create_provider(spec, record.clone()).await {
                Ok(()) => {}
                Err(error @ GatewayError::AlreadyExists { .. }) => {
                    match self.provider_state(&spec.name).await? {
                        ProviderState::Present => self.update_provider(spec, record).await?,
                        ProviderState::Absent => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            },
            ProviderState::Present => self.update_provider(spec, record).await?,
        }
        let Some(refresh) = refresh else {
            return Ok(());
        };
        let mut request = ConfigureProviderRefreshRequest {
            workspace_scope: Some(self.scope()),
            provider: spec.name.to_string(),
            credential_key: spec.keys.access_token.to_owned(),
            strategy: ProviderCredentialRefreshStrategy::Oauth2RefreshToken as i32,
            material: refresh.material,
            secret_material_keys: vec![REFRESH_TOKEN_MATERIAL.to_owned()],
            ..ConfigureProviderRefreshRequest::default()
        };
        if let Some(expires_at) = refresh.expires_at {
            let timestamp = request.expiration_time.get_or_insert_with(Default::default);
            timestamp.seconds = expires_at.div_euclid(1_000);
            timestamp.nanos =
                i32::try_from(expires_at.rem_euclid(1_000) * 1_000_000).map_err(|_| {
                    GatewayError::Protocol {
                        message: "the OAuth expiration nanoseconds are not representable"
                            .to_owned(),
                    }
                })?;
        }
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(spec.name.clone())),
            self.grpc().configure_provider_refresh(request),
        )
        .await?;
        Ok(())
    }

    async fn create_provider(
        &self,
        spec: &ProviderSpec,
        record: Provider,
    ) -> Result<(), GatewayError> {
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(spec.name.clone())),
            self.grpc().create_provider(CreateProviderRequest {
                workspace_scope: Some(self.scope()),
                provider: Some(record),
                request_id: String::new(),
            }),
        )
        .await
        .map(|_| ())
    }

    async fn update_provider(
        &self,
        spec: &ProviderSpec,
        mut record: Provider,
    ) -> Result<(), GatewayError> {
        let current = self.get_provider(&spec.name).await?;
        let removed = current
            .credentials
            .keys()
            .filter(|key| !record.credentials.contains_key(*key))
            .map(|key| (key.clone(), String::new()))
            .collect::<Vec<_>>();
        record.credentials.extend(removed);
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(spec.name.clone())),
            self.grpc()
                .delete_provider_refresh(DeleteProviderRefreshRequest {
                    workspace_scope: Some(self.scope()),
                    provider: spec.name.to_string(),
                    credential_key: spec.keys.access_token.to_owned(),
                    allow_missing: true,
                    request_id: String::new(),
                }),
        )
        .await?;
        record.r#type.clear();
        record.profile_workspace.clear();
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(spec.name.clone())),
            self.grpc().update_provider(UpdateProviderRequest {
                workspace_scope: Some(self.scope()),
                provider: Some(record),
                ..UpdateProviderRequest::default()
            }),
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError> {
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(name.clone())),
            self.grpc().delete_provider(DeleteProviderRequest {
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
