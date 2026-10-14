// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    BackendResources, DockerIdentity, DockerResources, InstanceIndexEntry, InstanceStatus,
    SessionRecord,
};

use jackin_core::Agent;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INSTANCE_MANIFEST_VERSION: u32 = 3;
pub const INSTANCE_INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceManifest {
    pub version: u32,
    pub instance_id: String,
    pub container_base: String,
    pub created_at: String,
    pub updated_at: String,
    pub workspace_name: Option<String>,
    pub workspace_label: String,
    pub workdir: String,
    pub host_workdir_fingerprint: String,
    pub role_key: String,
    pub role_display_name: String,
    pub agent_runtime: String,
    pub role_source_git: String,
    pub role_source_ref: Option<String>,
    pub image_tag: String,
    pub status: InstanceStatus,
    pub last_attach_outcome: Option<String>,
    pub docker: DockerResources,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_identity: Option<DockerIdentity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendResources>,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,

    #[serde(default)]
    pub role_git_sha: Option<String>,

    #[serde(default)]
    pub base_image_ref: Option<String>,

    #[serde(default)]
    pub base_image_digest: Option<String>,

    #[serde(default)]
    pub supported_agents: Vec<Agent>,

    pub admitted_instances: Vec<AdmittedInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmittedInstance {
    pub config_id: String,

    pub agent: Agent,

    pub account_id: String,

    #[serde(default)]
    pub registration_state: RegistrationState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationState {
    #[default]
    Current,

    Disabled,

    Removed,
}

impl RegistrationState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Disabled => "disabled",
            Self::Removed => "removed",
        }
    }
}

impl AdmittedInstance {
    pub fn new(config_id: impl Into<String>, agent: Agent, account_id: impl Into<String>) -> Self {
        Self {
            config_id: config_id.into(),
            agent,
            account_id: account_id.into(),
            registration_state: RegistrationState::Current,
        }
    }
}

impl From<&jackin_config::ResolvedInstance> for AdmittedInstance {
    fn from(instance: &jackin_config::ResolvedInstance) -> Self {
        Self::new(
            instance.config_id.clone(),
            instance.agent,
            instance.account_id.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndex {
    pub version: u32,
    pub instances: Vec<InstanceIndexEntry>,
}

#[derive(Debug)]
pub struct NewInstanceManifest<'a> {
    pub container_base: &'a str,
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub host_workdir_fingerprint: &'a str,
    pub role_key: &'a str,
    pub role_display_name: &'a str,
    pub agent_runtime: Agent,
    pub role_source_git: &'a str,
    pub role_source_ref: Option<&'a str>,
    pub image_tag: &'a str,
    pub docker: DockerResources,

    pub role_git_sha: Option<String>,

    pub base_image_ref: Option<String>,

    pub base_image_digest: Option<String>,

    pub supported_agents: Vec<Agent>,
}

pub fn host_path_fingerprint(path: &str) -> String {
    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(_) => path.to_owned(),
    };
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(digest))
}
