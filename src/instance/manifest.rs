use crate::agent::Agent;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const INSTANCE_MANIFEST_VERSION: u32 = 1;
pub const INSTANCE_INDEX_VERSION: u32 = 1;
const INSTANCE_INDEX_FILE: &str = "instances.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Active,
    Running,
    CleanExited,
    Crashed,
    PreservedDirty,
    PreservedUnpushed,
    RestoreAvailable,
    Superseded,
    Purged,
    FailedSetup,
}

impl InstanceStatus {
    /// Snake-case label matching the serde representation. Stable
    /// across renders, prompt UIs, and manifest inspect output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Running => "running",
            Self::CleanExited => "clean_exited",
            Self::Crashed => "crashed",
            Self::PreservedDirty => "preserved_dirty",
            Self::PreservedUnpushed => "preserved_unpushed",
            Self::RestoreAvailable => "restore_available",
            Self::Superseded => "superseded",
            Self::Purged => "purged",
            Self::FailedSetup => "failed_setup",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerResources {
    pub role_container: String,
    pub dind_container: String,
    pub network: String,
    pub certs_volume: String,
}

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndexEntry {
    pub instance_id: String,
    pub container_base: String,
    pub workspace_name: Option<String>,
    pub workspace_label: String,
    pub workdir: String,
    pub role_key: String,
    pub agent_runtime: String,
    pub status: InstanceStatus,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndex {
    pub version: u32,
    pub instances: Vec<InstanceIndexEntry>,
}

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
}

#[derive(Debug, Clone, Copy)]
pub struct InstanceQuery<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub role_key: Option<&'a str>,
    pub agent_runtime: Option<Agent>,
}

impl InstanceManifest {
    pub fn new(input: NewInstanceManifest<'_>) -> Self {
        let now = now_rfc3339();
        Self {
            version: INSTANCE_MANIFEST_VERSION,
            instance_id: input
                .container_base
                .rsplit_once('-')
                .map_or(input.container_base, |(_, id)| id)
                .to_string(),
            container_base: input.container_base.to_string(),
            created_at: now.clone(),
            updated_at: now,
            workspace_name: input.workspace_name.map(ToOwned::to_owned),
            workspace_label: input.workspace_label.to_string(),
            workdir: input.workdir.to_string(),
            host_workdir_fingerprint: input.host_workdir_fingerprint.to_string(),
            role_key: input.role_key.to_string(),
            role_display_name: input.role_display_name.to_string(),
            agent_runtime: input.agent_runtime.slug().to_string(),
            role_source_git: input.role_source_git.to_string(),
            role_source_ref: input.role_source_ref.map(ToOwned::to_owned),
            image_tag: input.image_tag.to_string(),
            status: InstanceStatus::Active,
            last_attach_outcome: None,
            docker: input.docker,
        }
    }

    pub fn mark_status(&mut self, status: InstanceStatus) {
        self.status = status;
        self.updated_at = now_rfc3339();
    }

    /// Bump `updated_at` without changing status. Use when a non-status
    /// field (`last_attach_outcome`) changes and the index still needs
    /// to reflect the most recent activity.
    pub fn touch(&mut self) {
        self.updated_at = now_rfc3339();
    }

    /// Parse `agent_runtime` into the typed enum. Errors when the on-disk
    /// slug is unknown (corrupt manifest or new agent added to the
    /// codebase but not migrated).
    pub fn agent(&self) -> anyhow::Result<Agent> {
        self.agent_runtime.parse().map_err(|_| {
            anyhow::anyhow!(
                "instance `{}` has unknown agent runtime {:?}",
                self.container_base,
                self.agent_runtime
            )
        })
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

    pub fn read(state_dir: &Path) -> anyhow::Result<Self> {
        let path = state_dir.join(".jackin/instance.json");
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading instance manifest at {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing instance manifest at {}", path.display()))
    }

    /// `Ok(None)` when the manifest file does not exist; `Err(_)` for
    /// parse or I/O failures. Lets callers distinguish "no recorded
    /// state" (fall through to the no-restore path) from "state exists
    /// but unreadable" (must surface, not silently treat as missing).
    pub fn read_optional(state_dir: &Path) -> anyhow::Result<Option<Self>> {
        let path = state_dir.join(".jackin/instance.json");
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
                format!("parsing instance manifest at {}", path.display())
            })?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(anyhow::Error::new(error)
                .context(format!("reading instance manifest at {}", path.display()))),
        }
    }

    pub fn write(&self, state_dir: &Path) -> anyhow::Result<()> {
        let manifest_dir = state_dir.join(".jackin");
        std::fs::create_dir_all(&manifest_dir)?;
        let path = manifest_dir.join("instance.json");
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

pub fn host_path_fingerprint(path: &str) -> String {
    let canonical = std::fs::canonicalize(path)
        .ok()
        .map_or_else(|| path.to_string(), |path| path.display().to_string());
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", crate::instance::naming::hex_lower(&digest))
}

impl InstanceIndexEntry {
    fn from_manifest(manifest: &InstanceManifest) -> Self {
        Self {
            instance_id: manifest.instance_id.clone(),
            container_base: manifest.container_base.clone(),
            workspace_name: manifest.workspace_name.clone(),
            workspace_label: manifest.workspace_label.clone(),
            workdir: manifest.workdir.clone(),
            role_key: manifest.role_key.clone(),
            agent_runtime: manifest.agent_runtime.clone(),
            status: manifest.status,
            updated_at: manifest.updated_at.clone(),
        }
    }

    fn matches(&self, query: InstanceQuery<'_>) -> bool {
        self.workspace_name.as_deref() == query.workspace_name
            && self.workspace_label == query.workspace_label
            && self.workdir == query.workdir
            && query
                .role_key
                .is_none_or(|role_key| self.role_key == role_key)
            && query
                .agent_runtime
                .is_none_or(|agent| self.agent_runtime == agent.slug())
    }
}

impl InstanceIndex {
    pub fn read_or_rebuild(data_dir: &Path) -> anyhow::Result<Self> {
        if let Some(index) = Self::read_optional(data_dir)? {
            return Ok(index);
        }
        let index = Self::rebuild(data_dir)?;
        index.write(data_dir)?;
        Ok(index)
    }

    /// Distinguish "file missing" (`Ok(None)` → rebuild path) from real
    /// read errors (parse failure, version mismatch, IO error other
    /// than `NotFound`). Real errors must propagate — silently
    /// rebuilding on a corrupted index throws away `Purged` tombstones
    /// whose state dir is already gone, and masks daemon/permission
    /// faults.
    fn read_optional(data_dir: &Path) -> anyhow::Result<Option<Self>> {
        let path = data_dir.join(INSTANCE_INDEX_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let index: Self = serde_json::from_slice(&bytes)
                    .with_context(|| format!("parsing instance index at {}", path.display()))?;
                anyhow::ensure!(
                    index.version == INSTANCE_INDEX_VERSION,
                    "unsupported instance index version {} at {}",
                    index.version,
                    path.display()
                );
                Ok(Some(index))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(anyhow::Error::new(error)
                .context(format!("reading instance index at {}", path.display()))),
        }
    }

    pub fn update_manifest(data_dir: &Path, manifest: &InstanceManifest) -> anyhow::Result<()> {
        let mut index = Self::read_or_rebuild(data_dir)?;
        index
            .instances
            .retain(|entry| entry.container_base != manifest.container_base);
        index
            .instances
            .push(InstanceIndexEntry::from_manifest(manifest));
        index.sort();
        index.write(data_dir)
    }

    pub fn remove(data_dir: &Path, container_base: &str) -> anyhow::Result<()> {
        let mut index = Self::read_or_rebuild(data_dir)?;
        index
            .instances
            .retain(|entry| entry.container_base != container_base);
        index.write(data_dir)
    }

    pub fn mark_purged(data_dir: &Path, container_base: &str) -> anyhow::Result<()> {
        let mut index = Self::read_or_rebuild(data_dir)?;
        Self::mark_purged_in_memory(&mut index, data_dir, container_base);
        index.sort();
        index.write(data_dir)
    }

    /// Batch-mark a set of containers as purged with a single index
    /// read/write. Skips containers already absent from the index when
    /// no manifest file exists on disk; otherwise backfills like
    /// [`Self::mark_purged`]. O(N + M): one pass over the index using
    /// `HashSet` membership instead of `find()` per container.
    pub fn mark_many_purged(data_dir: &Path, container_bases: &[&str]) -> anyhow::Result<()> {
        if container_bases.is_empty() {
            return Ok(());
        }
        let mut index = Self::read_or_rebuild(data_dir)?;
        let mut pending: std::collections::HashSet<&str> =
            container_bases.iter().copied().collect();
        let now = now_rfc3339();
        for entry in &mut index.instances {
            if pending.remove(entry.container_base.as_str()) {
                entry.status = InstanceStatus::Purged;
                entry.updated_at.clone_from(&now);
            }
        }
        // Containers without an existing index entry need a backfill
        // pass — read the manifest off disk or synthesize a tombstone.
        for container_base in pending {
            Self::backfill_purge_tombstone(&mut index, data_dir, container_base);
        }
        index.sort();
        index.write(data_dir)
    }

    fn mark_purged_in_memory(index: &mut Self, data_dir: &Path, container_base: &str) {
        if let Some(entry) = index
            .instances
            .iter_mut()
            .find(|entry| entry.container_base == container_base)
        {
            entry.status = InstanceStatus::Purged;
            entry.updated_at = now_rfc3339();
            return;
        }

        Self::backfill_purge_tombstone(index, data_dir, container_base);
    }

    /// Backfill: container not in the index but a manifest may exist
    /// on disk. Reads the manifest, synthesizes a minimal tombstone on
    /// parse failure, or no-ops when the state dir is already gone.
    fn backfill_purge_tombstone(index: &mut Self, data_dir: &Path, container_base: &str) {
        let state_dir = data_dir.join(container_base);
        match InstanceManifest::read_optional(&state_dir) {
            Ok(Some(mut manifest)) => {
                manifest.mark_status(InstanceStatus::Purged);
                index
                    .instances
                    .push(InstanceIndexEntry::from_manifest(&manifest));
            }
            // Manifest absent → state already torn down by
            // `purge_container_filesystem`; nothing to tombstone.
            Ok(None) => {}
            Err(error) => {
                // Corrupt manifest: synthesize a minimal tombstone so
                // the operator still sees that this container was
                // purged. Log the read error so `--debug` surfaces the
                // underlying file fault for forensics.
                crate::debug_log!(
                    "instance",
                    "mark_purged: manifest for `{container_base}` unreadable: {error}; synthesizing tombstone",
                );
                index.instances.push(InstanceIndexEntry {
                    instance_id: container_base.to_string(),
                    container_base: container_base.to_string(),
                    workspace_name: None,
                    workspace_label: String::new(),
                    workdir: String::new(),
                    role_key: String::new(),
                    agent_runtime: String::new(),
                    status: InstanceStatus::Purged,
                    updated_at: now_rfc3339(),
                });
            }
        }
    }

    pub fn matching_manifests(
        data_dir: &Path,
        query: InstanceQuery<'_>,
    ) -> anyhow::Result<Vec<InstanceManifest>> {
        let index = Self::read_or_rebuild(data_dir)?;
        let mut manifests = Vec::new();
        for entry in index
            .instances
            .into_iter()
            .filter(|entry| entry.matches(query))
        {
            let state_dir = data_dir.join(&entry.container_base);
            let Ok(manifest) = InstanceManifest::read(&state_dir) else {
                continue;
            };
            if InstanceIndexEntry::from_manifest(&manifest).matches(query) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(manifests)
    }

    /// Test-only accessor: error if the file is missing or unreadable.
    /// Production code uses [`Self::read_or_rebuild`] /
    /// [`Self::read_optional`].
    #[cfg(test)]
    pub(crate) fn read(data_dir: &Path) -> anyhow::Result<Self> {
        Self::read_optional(data_dir)?
            .ok_or_else(|| anyhow::anyhow!("instance index missing at {}", data_dir.display()))
    }

    fn rebuild(data_dir: &Path) -> anyhow::Result<Self> {
        let mut index = Self {
            version: INSTANCE_INDEX_VERSION,
            instances: Vec::new(),
        };
        if !data_dir.exists() {
            return Ok(index);
        }

        for entry in std::fs::read_dir(data_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            // Propagate parse errors; a corrupt manifest must not be
            // silently dropped from the rebuild.
            let Some(manifest) = InstanceManifest::read_optional(&entry.path())? else {
                continue;
            };
            index
                .instances
                .push(InstanceIndexEntry::from_manifest(&manifest));
        }
        index.sort();
        Ok(index)
    }

    fn write(&self, data_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(data_dir.join(INSTANCE_INDEX_FILE), bytes)?;
        Ok(())
    }

    fn sort(&mut self) {
        self.instances
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_manifest() -> InstanceManifest {
        InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        })
    }

    #[test]
    fn writes_manifest_under_jackin_state_dir() {
        let temp = tempdir().unwrap();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });
        manifest.mark_status(InstanceStatus::Running);

        manifest.write(temp.path()).unwrap();

        let body = std::fs::read_to_string(temp.path().join(".jackin/instance.json")).unwrap();
        assert!(body.contains(r#""version": 1"#));
        assert!(body.contains(r#""status": "running""#));
        assert!(body.contains(r#""role_key": "org/agent""#));
    }

    #[test]
    fn index_rebuilds_from_manifests_and_filters_by_query() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });
        manifest
            .write(&data_dir.join("jackin-workspace-agent-k7p9m2xq"))
            .unwrap();

        let matches = InstanceIndex::matching_manifests(
            data_dir,
            InstanceQuery {
                workspace_name: Some("workspace"),
                workspace_label: "workspace",
                workdir: "/workspace",
                role_key: Some("org/agent"),
                agent_runtime: Some(Agent::Claude),
            },
        )
        .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].container_base, "jackin-workspace-agent-k7p9m2xq");
        assert!(data_dir.join(INSTANCE_INDEX_FILE).exists());
    }

    #[test]
    fn index_update_replaces_existing_entry() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jackin-workspace-agent-k7p9m2xq",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jackin-org__agent",
            docker: DockerResources {
                role_container: "jackin-workspace-agent-k7p9m2xq".to_string(),
                dind_container: "jackin-workspace-agent-k7p9m2xq-dind".to_string(),
                network: "jackin-workspace-agent-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-workspace-agent-k7p9m2xq-dind-certs".to_string(),
            },
        });

        InstanceIndex::update_manifest(data_dir, &manifest).unwrap();
        manifest.mark_status(InstanceStatus::Running);
        InstanceIndex::update_manifest(data_dir, &manifest).unwrap();

        let index = InstanceIndex::read(data_dir).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].status, InstanceStatus::Running);
    }

    #[test]
    fn mark_many_purged_empty_slice_is_noop() {
        let data_dir = tempdir().unwrap();
        // No index file on disk yet.
        InstanceIndex::mark_many_purged(data_dir.path(), &[]).unwrap();
        // Empty slice must not even create an index file —
        // short-circuits before any read or write.
        assert!(!data_dir.path().join(INSTANCE_INDEX_FILE).exists());
    }

    #[test]
    fn mark_many_purged_tombstones_absent_and_present_in_one_pass() {
        let data_dir = tempdir().unwrap();
        let manifest_a = sample_manifest();
        let state_a = data_dir.path().join(manifest_a.container_base.as_str());
        manifest_a.write(&state_a).unwrap();
        InstanceIndex::update_manifest(data_dir.path(), &manifest_a).unwrap();

        // Container B has a manifest on disk but no index entry —
        // simulates a manifest written before an index update.
        let manifest_b_base = "jackin-workspace-agent-orphan01";
        let manifest_b = InstanceManifest {
            container_base: manifest_b_base.to_string(),
            ..manifest_a.clone()
        };
        let state_b = data_dir.path().join(manifest_b_base);
        manifest_b.write(&state_b).unwrap();

        InstanceIndex::mark_many_purged(
            data_dir.path(),
            &[manifest_a.container_base.as_str(), manifest_b_base],
        )
        .unwrap();

        let index = InstanceIndex::read(data_dir.path()).unwrap();
        assert_eq!(index.instances.len(), 2);
        assert!(
            index
                .instances
                .iter()
                .all(|e| e.status == InstanceStatus::Purged)
        );
    }

    #[test]
    fn mark_many_purged_is_idempotent() {
        let data_dir = tempdir().unwrap();
        let manifest = sample_manifest();
        let state_dir = data_dir.path().join(manifest.container_base.as_str());
        manifest.write(&state_dir).unwrap();
        InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

        InstanceIndex::mark_many_purged(data_dir.path(), &[manifest.container_base.as_str()])
            .unwrap();
        // Second call must not duplicate the entry or change the
        // status. Operator running `purge` twice (e.g. retry after a
        // partial failure) sees a stable tombstone.
        InstanceIndex::mark_many_purged(data_dir.path(), &[manifest.container_base.as_str()])
            .unwrap();

        let index = InstanceIndex::read(data_dir.path()).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].status, InstanceStatus::Purged);
    }

    #[test]
    fn index_mark_purged_retains_tombstone_after_state_removal() {
        let data_dir = tempdir().unwrap();
        let manifest = sample_manifest();
        let state_dir = data_dir.path().join(manifest.container_base.as_str());
        manifest.write(&state_dir).unwrap();
        InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

        InstanceIndex::mark_purged(data_dir.path(), &manifest.container_base).unwrap();
        std::fs::remove_dir_all(&state_dir).unwrap();

        let index = InstanceIndex::read(data_dir.path()).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].container_base, manifest.container_base);
        assert_eq!(index.instances[0].status, InstanceStatus::Purged);
    }
}
