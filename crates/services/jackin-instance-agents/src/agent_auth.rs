// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{SlotLayout, xdg_cache_rel};
use jackin_instance_credentials::AuthProvisionOutcome;

use jackin_config::{AuthForwardMode, ProfileSelector};

use std::collections::BTreeMap;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AgentRuntimeState {
    pub agent: jackin_core::Agent,

    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProvisionedInstanceAuth {
    pub agent: jackin_core::Agent,
    pub account_id: String,
    pub mode: AuthForwardMode,
    pub home_dir: Option<PathBuf>,
    pub credential_paths: Vec<PathBuf>,
    pub forward_auth: bool,
    pub slot_suffix: Option<String>,
    pub container_home_rel: String,
    pub container_store_rel: String,
    pub folder_target: String,

    pub cache_source_dir: Option<PathBuf>,

    pub container_cache_rel: Option<String>,
}

impl ProvisionedInstanceAuth {
    pub fn new(
        binding: &InstanceAuthBinding,
        home_dir: Option<PathBuf>,
        credential_paths: Vec<PathBuf>,
        forward_auth: bool,
        layout: SlotLayout,
    ) -> Self {
        Self {
            agent: binding.agent,
            account_id: binding.account_id.clone(),
            mode: binding.mode,
            home_dir,
            credential_paths,
            forward_auth,
            slot_suffix: layout.suffix,
            container_home_rel: layout.home_rel,
            container_store_rel: layout.store_rel,
            folder_target: layout.folder_target,
            cache_source_dir: None,
            container_cache_rel: None,
        }
    }

    pub fn with_xdg_cache(
        mut self,
        role_home: &Path,
        binding: &InstanceAuthBinding,
        suffix: Option<&str>,
    ) -> anyhow::Result<Self> {
        let Some(container_cache_rel) = xdg_cache_rel(binding.agent, suffix) else {
            return Ok(self);
        };
        let cache_source_dir = binding.xdg_roots.as_ref().map_or_else(
            || role_home.join(&container_cache_rel),
            |roots| roots.cache.clone(),
        );
        std::fs::create_dir_all(&cache_source_dir)?;
        self.cache_source_dir = Some(cache_source_dir);
        self.container_cache_rel = Some(container_cache_rel);
        Ok(self)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProvisionedAuth {
    pub slots: BTreeMap<String, ProvisionedInstanceAuth>,
}

impl ProvisionedAuth {
    #[must_use]
    pub fn instance_key(account_id: &str, agent: jackin_core::Agent) -> String {
        format!("{account_id}@{}", agent.slug())
    }

    #[must_use]
    pub fn for_agent(&self, agent: jackin_core::Agent) -> Option<&ProvisionedInstanceAuth> {
        self.slots.values().find(|slot| slot.agent == agent)
    }
}

#[derive(Debug, Clone)]
pub struct InstanceAuthBinding {
    pub key: String,
    pub agent: jackin_core::Agent,
    pub account_id: String,
    pub mode: AuthForwardMode,
    pub sync_source_dir: Option<PathBuf>,

    pub source_provider: Option<jackin_config::AiProvider>,

    pub source_selector: Option<ProfileSelector>,

    pub xdg_roots: Option<jackin_config::XdgRoots>,
    pub selected_source: Option<crate::SelectedAuthSourceSnapshot>,
}

impl InstanceAuthBinding {
    #[must_use]
    pub fn new(
        account_id: impl Into<String>,
        agent: jackin_core::Agent,
        mode: AuthForwardMode,
        sync_source_dir: Option<PathBuf>,
    ) -> Self {
        let account_id = account_id.into();
        let key = ProvisionedAuth::instance_key(&account_id, agent);
        Self {
            key,
            agent,
            account_id,
            mode,
            sync_source_dir,
            source_provider: None,
            source_selector: None,
            xdg_roots: None,
            selected_source: None,
        }
    }

    #[must_use]
    pub fn selected_source_revision(&self) -> Option<&str> {
        self.selected_source
            .as_ref()
            .map(crate::SelectedAuthSourceSnapshot::content_revision)
    }

    pub fn effective_selected_source_dir(&self) -> Option<PathBuf> {
        let xdg_agent_dir = match self.agent {
            jackin_core::Agent::Amp => Some("amp"),
            jackin_core::Agent::Opencode => Some("opencode"),
            _ => None,
        };
        xdg_agent_dir
            .and_then(|agent_dir| {
                self.xdg_roots
                    .as_ref()
                    .map(|roots| roots.data.join(agent_dir))
            })
            .or_else(|| self.sync_source_dir.clone())
    }

    pub fn provision_source_dir(&self) -> Option<&Path> {
        self.selected_source
            .as_ref()
            .map(crate::SelectedAuthSourceSnapshot::materialized_source_dir)
            .or(self.sync_source_dir.as_deref())
    }
}

pub const DEFAULT_ACCOUNT_ID: &str = "default";

#[derive(Debug)]
pub struct AgentAuthProvision {
    pub key: String,
    pub auth: ProvisionedInstanceAuth,
    pub outcome: AuthProvisionOutcome,
}

pub fn emit_agent_auth_provision(
    agent: jackin_core::Agent,
    mode: AuthForwardMode,
    result: Result<AuthProvisionOutcome, jackin_telemetry::schema::enums::ErrorType>,
) {
    use jackin_telemetry::{Attr, FieldSet, Value, event, schema};

    let auth_mode = match mode {
        AuthForwardMode::Sync => schema::enums::AuthMode::Sync,
        AuthForwardMode::ApiKey => schema::enums::AuthMode::ApiKey,
        AuthForwardMode::OAuthToken => schema::enums::AuthMode::OauthToken,
        AuthForwardMode::Ignore => schema::enums::AuthMode::Ignore,
    };
    let configured_source = match mode {
        AuthForwardMode::Sync => schema::enums::CredentialSourceType::AgentHome,
        AuthForwardMode::ApiKey | AuthForwardMode::OAuthToken => {
            schema::enums::CredentialSourceType::Environment
        }
        AuthForwardMode::Ignore => schema::enums::CredentialSourceType::None,
    };
    let (source, outcome, error_type) = match result {
        Ok(AuthProvisionOutcome::Synced | AuthProvisionOutcome::TokenMode) => (
            configured_source,
            schema::enums::OutcomeValue::Success,
            None,
        ),
        Ok(AuthProvisionOutcome::Skipped) => (
            schema::enums::CredentialSourceType::None,
            schema::enums::OutcomeValue::Skip,
            None,
        ),
        Ok(AuthProvisionOutcome::HostMissing) => (
            schema::enums::CredentialSourceType::None,
            schema::enums::OutcomeValue::Failure,
            Some(schema::enums::ErrorType::CredentialUnavailable),
        ),
        Err(error_type) => (
            configured_source,
            schema::enums::OutcomeValue::Error,
            Some(error_type),
        ),
    };
    let mut attrs = vec![
        Attr {
            key: schema::attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent.slug()),
        },
        Attr {
            key: schema::attrs::AUTH_MODE,
            value: Value::Str(auth_mode.as_str()),
        },
        Attr {
            key: schema::attrs::CREDENTIAL_SOURCE_TYPE,
            value: Value::Str(source.as_str()),
        },
        Attr {
            key: schema::attrs::OUTCOME,
            value: Value::Str(outcome.as_str()),
        },
    ];
    if let Some(error_type) = error_type {
        attrs.push(Attr {
            key: schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type.as_str()),
        });
    }
    let _telemetry_result =
        jackin_telemetry::emit_event(&event::AUTH_PROVISION, FieldSet::new(&attrs, None));
}
