// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `InstanceManifest` persistence methods.

use super::{
    AdmittedInstance, BackendResources, INSTANCE_MANIFEST_VERSION, InstanceIndex,
    InstanceIndexEntry, InstanceManifest, InstanceStatus, NewInstanceManifest, RegistrationState,
    now_rfc3339,
};
use anyhow::Context;

use jackin_core::Agent;

use std::path::Path;

impl InstanceManifest {
    /// Docker-backed instance: the `backend` field is left `None`, so the
    /// `docker` resources are the single source of truth (current behavior).
    pub fn new(input: NewInstanceManifest<'_>) -> Self {
        Self::build(input, None)
    }

    /// Instance launched by an explicit non-Docker backend (e.g.
    /// apple-container). `docker` is still populated for naming continuity, but
    /// `backend` records which backend actually owns the instance.
    pub fn new_with_backend(input: NewInstanceManifest<'_>, backend: BackendResources) -> Self {
        Self::build(input, Some(backend))
    }

    pub(crate) fn build(input: NewInstanceManifest<'_>, backend: Option<BackendResources>) -> Self {
        let now = now_rfc3339();
        Self {
            version: INSTANCE_MANIFEST_VERSION,
            instance_id: crate::naming::instance_id_from_container_base(input.container_base)
                .unwrap_or(input.container_base)
                .to_owned(),
            container_base: input.container_base.to_owned(),
            created_at: now.clone(),
            updated_at: now,
            workspace_name: input.workspace_name.map(ToOwned::to_owned),
            workspace_label: input.workspace_label.to_owned(),
            workdir: input.workdir.to_owned(),
            host_workdir_fingerprint: input.host_workdir_fingerprint.to_owned(),
            role_key: input.role_key.to_owned(),
            role_display_name: input.role_display_name.to_owned(),
            agent_runtime: input.agent_runtime.slug().to_owned(),
            role_source_git: input.role_source_git.to_owned(),
            role_source_ref: input.role_source_ref.map(ToOwned::to_owned),
            image_tag: input.image_tag.to_owned(),
            status: InstanceStatus::Active,
            last_attach_outcome: None,
            docker: input.docker,
            docker_identity: None,
            backend,
            sessions: Vec::new(),
            role_git_sha: input.role_git_sha,
            base_image_ref: input.base_image_ref,
            base_image_digest: input.base_image_digest,
            supported_agents: input.supported_agents,
            admitted_instances: Vec::new(),
        }
    }

    /// Record the launch-admitted instances, in launch order. Called by the
    /// host launch path once instances resolve; kept out of
    /// [`NewInstanceManifest`] so admission can be staged before persistence.
    pub fn set_admitted_instances(&mut self, admitted: impl IntoIterator<Item = AdmittedInstance>) {
        self.admitted_instances = admitted.into_iter().collect();
    }

    /// Whether `config_id` was admitted at launch.
    pub fn admits_instance(&self, config_id: &str) -> bool {
        self.admitted_instances
            .iter()
            .any(|admitted| admitted.config_id == config_id)
    }

    /// Owning account ID for an admitted instance config ID.
    pub fn account_for_instance(&self, config_id: &str) -> Option<&str> {
        self.admitted_instances
            .iter()
            .find(|admitted| admitted.config_id == config_id)
            .map(|admitted| admitted.account_id.as_str())
    }

    /// Agent runtime for an admitted instance config ID.
    pub fn agent_for_instance(&self, config_id: &str) -> Option<Agent> {
        self.admitted_instances
            .iter()
            .find(|admitted| admitted.config_id == config_id)
            .map(|admitted| admitted.agent)
    }

    /// Mark one admitted instance's host registration without changing its
    /// recorded config, agent, account, labels, or tab admission.
    pub fn mark_registration_state(&mut self, config_id: &str, state: RegistrationState) -> bool {
        let Some(admitted) = self
            .admitted_instances
            .iter_mut()
            .find(|admitted| admitted.config_id == config_id)
        else {
            return false;
        };
        if admitted.registration_state == state {
            return false;
        }
        admitted.registration_state = state;
        true
    }

    /// Registration state for one admitted instance.
    #[must_use]
    pub fn registration_state_for_instance(&self, config_id: &str) -> Option<RegistrationState> {
        self.admitted_instances
            .iter()
            .find(|admitted| admitted.config_id == config_id)
            .map(|admitted| admitted.registration_state)
    }

    /// Project this manifest to the lightweight index entry stored in
    /// `instances.json`. The inverse of reading a full manifest from disk.
    pub fn to_index_entry(&self) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: self.instance_id.clone(),
            container_base: self.container_base.clone(),
            workspace_name: self.workspace_name.clone(),
            workspace_label: self.workspace_label.clone(),
            workdir: self.workdir.clone(),
            role_key: self.role_key.clone(),
            agent_runtime: self.agent_runtime.clone(),
            status: self.status,
            updated_at: self.updated_at.clone(),
        }
    }

    pub fn mark_status(&mut self, status: InstanceStatus) {
        self.status = status;
        self.updated_at = now_rfc3339();
    }

    /// Refreshes `updated_at` so a side-channel mutation (e.g.
    /// `last_attach_outcome`) still moves the entry in the index.
    pub fn touch(&mut self) {
        self.updated_at = now_rfc3339();
    }

    /// Errors when the on-disk slug is unknown — corrupt manifest or a
    /// new agent added to the codebase but not migrated here.
    pub fn agent(&self) -> Result<Agent, crate::InstanceError> {
        self.agent_runtime
            .parse()
            .map_err(|_| crate::InstanceError::UnknownAgentRuntime {
                container_base: self.container_base.clone(),
                agent_runtime: self.agent_runtime.clone(),
            })
    }

    /// Promote the manifest to `RestoreAvailable` and persist the change
    /// to both `instance.json` and the workspace index. Used by every
    /// restore-discovery surface (hardline prompt, attach-time `DinD`
    /// loss, console "found restorable" path).
    pub fn mark_restore_available(
        &mut self,
        paths: &jackin_core::JackinPaths,
    ) -> anyhow::Result<()> {
        self.mark_status(InstanceStatus::RestoreAvailable);
        let state_dir = paths.data_dir.join(&self.container_base);
        self.write(&state_dir)?;
        InstanceIndex::update_manifest(&paths.data_dir, self)
    }

    pub const fn is_restore_candidate(&self) -> bool {
        matches!(
            self.status,
            InstanceStatus::Active
                | InstanceStatus::Running
                | InstanceStatus::Crashed
                | InstanceStatus::PreservedDirty
                | InstanceStatus::PreservedUnpushed
                | InstanceStatus::RestoreAvailable
                | InstanceStatus::FailedSetup
        )
    }

    /// Whether this instance should appear in the launch dialog (D10).
    ///
    /// Stricter than `is_restore_candidate`: excludes `Active`/`Running`
    /// because D13 means the launch path never re-attaches to a live
    /// container. Live instances only appear in the console instance picker.
    pub const fn is_launch_restore_candidate(&self) -> bool {
        matches!(
            self.status,
            InstanceStatus::Crashed
                | InstanceStatus::PreservedDirty
                | InstanceStatus::PreservedUnpushed
                | InstanceStatus::RestoreAvailable
                | InstanceStatus::FailedSetup
        )
    }

    pub fn read(state_dir: &Path) -> anyhow::Result<Self> {
        let path = state_dir.join(".jackin/instance.json");
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading instance manifest at {}", path.display()))?;
        Self::parse_and_validate(&bytes, &path)
    }

    /// `Ok(None)` when the manifest file does not exist; `Err(_)` for
    /// parse or I/O failures. Lets callers distinguish "no recorded
    /// state" (fall through to the no-restore path) from "state exists
    /// but unreadable" (must surface, not silently treat as missing).
    pub fn read_optional(state_dir: &Path) -> anyhow::Result<Option<Self>> {
        let path = state_dir.join(".jackin/instance.json");
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(Self::parse_and_validate(&bytes, &path)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(anyhow::Error::new(error)
                .context(format!("reading instance manifest at {}", path.display()))),
        }
    }

    pub(crate) fn parse_and_validate(bytes: &[u8], path: &Path) -> anyhow::Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .with_context(|| format!("parsing instance manifest at {}", path.display()))?;
        anyhow::ensure!(
            manifest.version == INSTANCE_MANIFEST_VERSION,
            "unsupported instance manifest version {} at {}",
            manifest.version,
            path.display()
        );
        Ok(manifest)
    }

    /// Collapses [`Self::read_optional`]'s three outcomes into the two
    /// the discovery surfaces care about — `Some` (use the manifest)
    /// vs `None` (skip the candidate). Callers run inside discovery loops, so
    /// unreadable candidates are skipped without emitting user-derived paths.
    pub fn read_optional_lossy(state_dir: &Path) -> Option<Self> {
        Self::read_optional(state_dir).unwrap_or_default()
    }

    pub fn write(&self, state_dir: &Path) -> anyhow::Result<()> {
        let path = state_dir.join(".jackin/instance.json");
        let body = serde_json::to_string_pretty(self)?;
        Ok(jackin_config::atomic_write(&path, &body)?)
    }
}
