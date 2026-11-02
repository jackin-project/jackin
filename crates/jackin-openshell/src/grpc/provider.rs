// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use jackin_core::{ProviderName, Redactor};
use openshell_sdk::raw::proto::{
    ConfigureProviderRefreshRequest, CreateProviderRequest, DeleteProviderRefreshRequest,
    DeleteProviderRequest, GetProviderProfileRequest, GetProviderRequest,
    ImportProviderProfilesRequest, ObjectMeta, Provider, ProviderCredentialRefreshStrategy,
    ProviderProfile, ProviderProfileDiagnostic, ProviderProfileImportItem,
    UpdateProviderProfilesRequest, UpdateProviderRequest,
};

use super::Inner;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::gateway::{ProfileSync, ProviderMaterial, ProviderSpec, ProviderState};
use crate::policy::ProviderProfile as ProfileModel;

const PROFILE_SOURCE: &str = "jackin";
const CLIENT_ID_MATERIAL: &str = "client_id";
const REFRESH_TOKEN_MATERIAL: &str = "refresh_token";

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
        let current = self
            .unary(
                GatewayOp::Profile,
                Some(Missing::Profile(profile.id.clone())),
                self.grpc().get_provider_profile(GetProviderProfileRequest {
                    workspace_scope: Some(self.scope()),
                    id: profile.id.to_string(),
                }),
            )
            .await;
        let stored = match current {
            Ok(response) => response.profile,
            Err(GatewayError::NotFound { .. }) => {
                self.import_profile(desired).await?;
                return Ok(ProfileSync::Imported);
            }
            Err(error) => return Err(error),
        };
        let Some(stored) = stored else {
            return Err(GatewayError::Protocol {
                message: "the response has no profile".to_owned(),
            });
        };
        let version = stored.resource_version;
        if normalized(stored) == normalized(desired.clone()) {
            return Ok(ProfileSync::Unchanged);
        }
        self.update_profile(desired, version).await?;
        Ok(ProfileSync::Updated)
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
        let ProviderMaterial::ChatgptLogin {
            access_token,
            refresh_token,
            client_id,
            chatgpt_account_id,
        } = &spec.material;
        let redactor = Redactor::new([access_token, refresh_token]);
        self.unary(
            GatewayOp::Profile,
            Some(Missing::Profile(spec.profile.clone())),
            self.grpc().get_provider_profile(GetProviderProfileRequest {
                workspace_scope: Some(self.scope()),
                id: spec.profile.to_string(),
            }),
        )
        .await?;
        let credentials = HashMap::from([
            (
                spec.keys.access_token.to_owned(),
                access_token.expose().to_owned(),
            ),
            (
                spec.keys.account_id.to_owned(),
                chatgpt_account_id.to_string(),
            ),
        ]);
        let material = HashMap::from([
            (CLIENT_ID_MATERIAL.to_owned(), client_id.clone()),
            (
                REFRESH_TOKEN_MATERIAL.to_owned(),
                refresh_token.expose().to_owned(),
            ),
        ]);
        let result = self.write_provider(spec, credentials, material).await;
        result.map_err(|error| redact(error, &redactor))
    }

    async fn write_provider(
        &self,
        spec: &ProviderSpec,
        credentials: HashMap<String, String>,
        material: HashMap<String, String>,
    ) -> Result<(), GatewayError> {
        let mut record =
            provider_record(&spec.name, spec.profile.as_str(), self.workspace.as_str());
        record.credentials = credentials;
        match self.get_provider(&spec.name).await {
            Err(GatewayError::NotFound { .. }) => {
                self.unary(
                    GatewayOp::Provider,
                    Some(Missing::Provider(spec.name.clone())),
                    self.grpc().create_provider(CreateProviderRequest {
                        workspace_scope: Some(self.scope()),
                        provider: Some(record),
                        request_id: String::new(),
                    }),
                )
                .await?;
            }
            Ok(_) => {
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
                .await?;
            }
            Err(error) => return Err(error),
        }
        self.unary(
            GatewayOp::Provider,
            Some(Missing::Provider(spec.name.clone())),
            self.grpc()
                .configure_provider_refresh(ConfigureProviderRefreshRequest {
                    workspace_scope: Some(self.scope()),
                    provider: spec.name.to_string(),
                    credential_key: spec.keys.access_token.to_owned(),
                    strategy: ProviderCredentialRefreshStrategy::Oauth2RefreshToken as i32,
                    material,
                    secret_material_keys: vec![REFRESH_TOKEN_MATERIAL.to_owned()],
                    ..ConfigureProviderRefreshRequest::default()
                }),
        )
        .await?;
        Ok(())
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
