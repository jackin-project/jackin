// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Manifest format versions, records, and host fingerprint.

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
    /// Missing identity means this record cannot authorize Docker teardown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_identity: Option<DockerIdentity>,
    /// Backend that launched this instance. `None` for legacy/Docker manifests
    /// (the `docker` field above is the source of truth then); `Some` records
    /// the backend explicitly for apple-container and future backends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendResources>,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    /// Pinned role-repo commit SHA baked into the image at launch (D7).
    /// Consumed by Tier 3 rebuild so restore does not re-resolve HEAD.
    #[serde(default)]
    pub role_git_sha: Option<String>,
    /// Base/construct image tag used when this image was built (D7/D16).
    /// Persisted now for the planned faithful Tier-3 base pinning; not yet read
    /// back (current Tier 3 rebuilds from `role_git_sha` only).
    #[serde(default)]
    pub base_image_ref: Option<String>,
    /// Base/construct image digest at launch time (D16). Reserved for faithful
    /// Tier-3 base pinning; always written as `None` today and not yet consumed.
    #[serde(default)]
    pub base_image_digest: Option<String>,
    /// Agents baked into the image at launch (D7). Persisted for restore
    /// diagnostics; the live supported-agent set is read from the role manifest,
    /// so this field is not yet read back. Serializes as the lowercase slugs.
    #[serde(default)]
    pub supported_agents: Vec<Agent>,
    /// Instances admitted at launch, in launch order. Host-side tab
    /// validation checks spawned tabs against this set so a tab can never
    /// reference an instance (or account) the launch did not authorize.
    /// The field is required by the v3 manifest contract. An empty vector is
    /// an explicit v3 admission set, never a legacy-manifest fallback.
    pub admitted_instances: Vec<AdmittedInstance>,
}

/// One launch-admitted instance: its exact config ID, agent runtime, and
/// owning account ID.
/// Identifiers only — never credential material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmittedInstance {
    /// Instance config ID (`"claude-work"`).
    pub config_id: String,
    /// Agent runtime bound to the instance config.
    pub agent: Agent,
    /// Owning account ID (`"work"`).
    pub account_id: String,
    /// Current host-registration state observed after launch. This never
    /// changes the recorded identity or materialized session credentials.
    #[serde(default)]
    pub registration_state: RegistrationState,
}

/// Host registration state for an already admitted running instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationState {
    /// Registration is currently enabled and resolvable.
    #[default]
    Current,
    /// Registration remains known but is disabled for new grants.
    Disabled,
    /// Registration was removed or no longer resolves to the recorded entry.
    Removed,
}

impl RegistrationState {
    /// Stable operator-facing state label.
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
    /// Record one admitted instance.
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
    /// Pinned role-repo commit SHA at launch time (D7).
    pub role_git_sha: Option<String>,
    /// Base/construct image tag at launch time (D7/D16).
    pub base_image_ref: Option<String>,
    /// Base/construct image digest at launch time (D16).
    pub base_image_digest: Option<String>,
    /// Agents baked into the image at launch (D7).
    pub supported_agents: Vec<Agent>,
}

/// SHA-256 of the canonical host path.
///
/// Falls back to the raw input when `canonicalize` fails (path does not exist
/// yet, unreadable, symlink loop). A bare `canonicalize().ok()` would silently
/// produce identical fingerprints across hosts with the same broken input.
pub fn host_path_fingerprint(path: &str) -> String {
    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(_) => path.to_owned(),
    };
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(digest))
}
