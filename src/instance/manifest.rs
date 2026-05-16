use crate::agent::Agent;
use anyhow::Context;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const INSTANCE_MANIFEST_VERSION: u32 = 1;
pub const INSTANCE_INDEX_VERSION: u32 = 1;
const INSTANCE_INDEX_FILE: &str = "instances.json";
const INSTANCE_INDEX_LOCK_FILE: &str = "instances.json.lock";

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
    /// Snake-case label matching the serde representation.
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

    /// Compact UI label for dense table rows where horizontal space is
    /// scarce. Lives on the type so adding a variant forces the renderer
    /// to update; a parallel free-function mapping silently drifts.
    #[must_use]
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Running => "running",
            Self::CleanExited => "clean",
            Self::Crashed => "crashed",
            Self::PreservedDirty => "dirty",
            Self::PreservedUnpushed => "unpushed",
            Self::RestoreAvailable => "restore",
            Self::Superseded => "superseded",
            Self::Purged => "purged",
            Self::FailedSetup => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Exited,
    ContainerMissing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub name: String,
    pub agent_runtime: String,
    pub tmux_name: String,
    pub created_at: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attached_at: Option<String>,
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
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
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
            instance_id: crate::instance::naming::instance_id_from_container_base(
                input.container_base,
            )
            .unwrap_or(input.container_base)
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
            sessions: Vec::new(),
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
    pub fn agent(&self) -> anyhow::Result<Agent> {
        self.agent_runtime.parse().map_err(|_| {
            anyhow::anyhow!(
                "instance `{}` has unknown agent runtime {:?}",
                self.container_base,
                self.agent_runtime
            )
        })
    }

    /// Promote the manifest to `RestoreAvailable` and persist the change
    /// to both `instance.json` and the workspace index. Used by every
    /// restore-discovery surface (hardline prompt, attach-time `DinD`
    /// loss, console "found restorable" path).
    pub fn mark_restore_available(
        &mut self,
        paths: &crate::paths::JackinPaths,
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

    /// Collapses [`Self::read_optional`]'s three outcomes into the two
    /// the discovery surfaces care about — `Some` (use the manifest)
    /// vs `None` (skip the candidate). Logs parse/IO failures via
    /// `debug_log!` so an unreadable manifest still surfaces under
    /// `--debug` rather than silently disappearing from `--inspect`,
    /// hardline candidate scans, or attach-outcome recording.
    pub fn read_or_log(state_dir: &Path, site: &str) -> Option<Self> {
        match Self::read_optional(state_dir) {
            Ok(value) => value,
            Err(error) => {
                crate::debug_log!(
                    "instance",
                    "{site}: skipping {} due to unreadable manifest: {error}",
                    state_dir.display(),
                );
                None
            }
        }
    }

    pub fn write(&self, state_dir: &Path) -> anyhow::Result<()> {
        let path = state_dir.join(".jackin/instance.json");
        let body = serde_json::to_string_pretty(self)?;
        crate::config::persist::atomic_write(&path, &body)
    }
}

/// SHA-256 of the canonical host path.
///
/// Falls back to the raw input when `canonicalize` fails (path does
/// not exist yet, unreadable, symlink loop) and logs the underlying
/// error via `debug_log!` so an operator hitting a fingerprint
/// mismatch can correlate it back to the canonicalize fault. A bare
/// `canonicalize().ok()` would silently produce identical fingerprints
/// across hosts with the same broken input.
pub fn host_path_fingerprint(path: &str) -> String {
    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(error) => {
            crate::debug_log!(
                "instance",
                "host_path_fingerprint: canonicalize({path}) failed ({error}); falling back to raw input",
            );
            path.to_string()
        }
    };
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

    pub fn matches(&self, query: InstanceQuery<'_>) -> bool {
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
        Self::with_lock(data_dir, |index| {
            index
                .instances
                .retain(|entry| entry.container_base != manifest.container_base);
            index
                .instances
                .push(InstanceIndexEntry::from_manifest(manifest));
            Ok(())
        })
    }

    pub fn remove(data_dir: &Path, container_base: &str) -> anyhow::Result<()> {
        Self::with_lock(data_dir, |index| {
            index
                .instances
                .retain(|entry| entry.container_base != container_base);
            Ok(())
        })
    }

    /// Removes entries in a single lock pass.
    pub fn remove_many(data_dir: &Path, container_bases: &[&str]) -> anyhow::Result<()> {
        if container_bases.is_empty() {
            return Ok(());
        }
        let set: std::collections::HashSet<&str> = container_bases.iter().copied().collect();
        Self::with_lock(data_dir, |index| {
            index
                .instances
                .retain(|entry| !set.contains(entry.container_base.as_str()));
            Ok(())
        })
    }

    pub fn mark_purged(data_dir: &Path, container_base: &str) -> anyhow::Result<()> {
        Self::mark_many_purged(data_dir, &[container_base])
    }

    /// Run `mutate` under an exclusive flock on `instances.json.lock`
    /// after reading the current index, then write the result back
    /// atomically. Prevents two concurrent `update_manifest` calls from
    /// racing read-modify-write and clobbering each other's entries —
    /// the per-name lock in `claim_container_name` protects names, not
    /// the index payload.
    fn with_lock<F>(data_dir: &Path, mutate: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut Self) -> anyhow::Result<()>,
    {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("create data dir {}", data_dir.display()))?;
        let lock_path = data_dir.join(INSTANCE_INDEX_LOCK_FILE);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("open index lock {}", lock_path.display()))?;
        FileExt::lock_exclusive(&lock)
            .with_context(|| format!("acquire index lock {}", lock_path.display()))?;
        let result = (|| {
            let mut index = Self::read_or_rebuild(data_dir)?;
            mutate(&mut index)?;
            index.sort();
            index.write(data_dir)
        })();
        // Drop the handle (which releases the flock); leave the lock
        // file in place so future opens reuse the same inode.
        drop(lock);
        result
    }

    /// Batch-mark a set of containers as purged with a single index
    /// read/write. Containers already absent from the index get a
    /// backfilled tombstone read from disk (or a synthesized minimal
    /// row when the manifest is corrupt).
    ///
    /// One pass over the index using `HashSet` membership avoids the
    /// O(N×M) cost of `find()`-per-container when a class-wide purge
    /// touches many entries.
    pub fn mark_many_purged(data_dir: &Path, container_bases: &[&str]) -> anyhow::Result<()> {
        if container_bases.is_empty() {
            return Ok(());
        }
        Self::with_lock(data_dir, |index| {
            let mut pending: std::collections::HashSet<&str> =
                container_bases.iter().copied().collect();
            let now = now_rfc3339();
            for entry in &mut index.instances {
                if pending.remove(entry.container_base.as_str()) {
                    entry.status = InstanceStatus::Purged;
                    entry.updated_at.clone_from(&now);
                }
            }
            for container_base in pending {
                Self::backfill_purge_tombstone(index, data_dir, container_base);
            }
            Ok(())
        })
    }

    /// Container is not in the index but a manifest may still exist on
    /// disk. Synthesizes a minimal tombstone on parse failure so the
    /// operator still sees the purge.
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
            let Some(manifest) = InstanceManifest::read_or_log(&state_dir, "matching_manifests")
            else {
                continue;
            };
            if InstanceIndexEntry::from_manifest(&manifest).matches(query) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(manifests)
    }

    /// Errors if the file is missing or unreadable.
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
        let body = serde_json::to_string_pretty(self)?;
        crate::config::persist::atomic_write(&data_dir.join(INSTANCE_INDEX_FILE), &body)
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
            container_base: "jk-k7p9m2xq-workspace-agent",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jk_org_agent",
            docker: DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agent".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agent-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agent-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agent-dind-certs".to_string(),
            },
        })
    }

    #[test]
    fn writes_manifest_under_jackin_state_dir() {
        let temp = tempdir().unwrap();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jk-k7p9m2xq-workspace-agent",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jk_org_agent",
            docker: DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agent".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agent-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agent-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agent-dind-certs".to_string(),
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
            container_base: "jk-k7p9m2xq-workspace-agent",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jk_org_agent",
            docker: DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agent".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agent-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agent-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agent-dind-certs".to_string(),
            },
        });
        manifest
            .write(&data_dir.join("jk-k7p9m2xq-workspace-agent"))
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
        assert_eq!(matches[0].container_base, "jk-k7p9m2xq-workspace-agent");
        assert!(data_dir.join(INSTANCE_INDEX_FILE).exists());
    }

    #[test]
    fn index_update_replaces_existing_entry() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        let mut manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: "jk-k7p9m2xq-workspace-agent",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "org/agent",
            role_display_name: "Agent",
            agent_runtime: Agent::Claude,
            role_source_git: "https://example.invalid/role.git",
            role_source_ref: Some("main"),
            image_tag: "jk_org_agent",
            docker: DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agent".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agent-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agent-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agent-dind-certs".to_string(),
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
        InstanceIndex::mark_many_purged(data_dir.path(), &[]).unwrap();
        // Empty slice must not create an index file — short-circuits
        // before any read or write.
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
        let manifest_b_base = "jk-orphan01-workspace-agent";
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
    fn remove_many_empty_slice_is_noop() {
        let data_dir = tempdir().unwrap();
        InstanceIndex::remove_many(data_dir.path(), &[]).unwrap();
        assert!(!data_dir.path().join("instances.json").exists());
    }

    #[test]
    fn remove_many_deletes_only_named_entries() {
        let data_dir = tempdir().unwrap();
        let manifest = sample_manifest();
        let other_base = "jk-a1b2c3d4-other-agent";
        let mut other = manifest.clone();
        other.container_base = other_base.to_string();
        InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();
        InstanceIndex::update_manifest(data_dir.path(), &other).unwrap();

        InstanceIndex::remove_many(data_dir.path(), &[manifest.container_base.as_str()]).unwrap();

        let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].container_base, other_base);
    }

    #[test]
    fn remove_many_with_absent_name_is_noop() {
        let data_dir = tempdir().unwrap();
        let manifest = sample_manifest();
        InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

        InstanceIndex::remove_many(data_dir.path(), &["jk-nothere-agent"]).unwrap();

        let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
        assert_eq!(index.instances.len(), 1);
        assert_eq!(index.instances[0].container_base, manifest.container_base);
    }

    #[test]
    fn remove_many_with_duplicate_names_removes_once() {
        let data_dir = tempdir().unwrap();
        let manifest = sample_manifest();
        InstanceIndex::update_manifest(data_dir.path(), &manifest).unwrap();

        InstanceIndex::remove_many(
            data_dir.path(),
            &[
                manifest.container_base.as_str(),
                manifest.container_base.as_str(),
            ],
        )
        .unwrap();

        let index = InstanceIndex::read_or_rebuild(data_dir.path()).unwrap();
        assert!(index.instances.is_empty());
    }

    #[test]
    fn instance_manifest_write_replaces_partial_file() {
        // Simulate a previous crash that left a half-written JSON.
        // Atomic write must replace it cleanly; a regression to a
        // direct `std::fs::write` would either preserve the truncated
        // content on a short write or interleave bytes.
        let temp = tempdir().unwrap();
        let state_dir = temp.path();
        std::fs::create_dir_all(state_dir.join(".jackin")).unwrap();
        std::fs::write(state_dir.join(".jackin/instance.json"), b"{ partial").unwrap();
        sample_manifest().write(state_dir).unwrap();
        let body = std::fs::read_to_string(state_dir.join(".jackin/instance.json")).unwrap();
        assert!(body.contains(r#""version": 1"#));
        assert!(!body.contains("partial"));
    }

    #[test]
    fn index_write_leaves_no_temp_file_on_success() {
        // Atomic write is `tempfile + rename`; on success the temp must
        // be gone. A regression that wrote in-place would leave the
        // temp behind (or worse, never rename) — assert the data dir
        // contains exactly the canonical file.
        let temp = tempdir().unwrap();
        let data_dir = temp.path();
        InstanceIndex::update_manifest(data_dir, &sample_manifest()).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(data_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("instances"))
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                INSTANCE_INDEX_FILE.to_string(),
                INSTANCE_INDEX_LOCK_FILE.to_string(),
            ]
        );
    }

    #[test]
    fn update_manifest_concurrent_writes_serialize_via_lock() {
        // Two threads racing `update_manifest` for *different* manifests
        // must both end up in the index. A regression that drops the
        // index flock would lose one of the two entries.
        let temp = tempdir().unwrap();
        let data_dir = temp.path().to_path_buf();
        let a = sample_manifest();
        let mut b = sample_manifest();
        b.container_base = "jackin-other-7p9m2xqk".to_string();
        b.instance_id = "7p9m2xqk".to_string();

        let d1 = data_dir.clone();
        let h1 = std::thread::spawn(move || InstanceIndex::update_manifest(&d1, &a).unwrap());
        let d2 = data_dir.clone();
        let h2 = std::thread::spawn(move || InstanceIndex::update_manifest(&d2, &b).unwrap());
        h1.join().unwrap();
        h2.join().unwrap();

        let index = InstanceIndex::read(&data_dir).unwrap();
        assert_eq!(index.instances.len(), 2);
    }

    #[test]
    fn host_path_fingerprint_differs_for_distinct_canonical_paths() {
        // Two existing dirs with distinct canonical paths must yield
        // distinct fingerprints (catches a regression that hashes the
        // raw input even when canonicalize succeeds).
        let temp = tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let fa = host_path_fingerprint(&a.display().to_string());
        let fb = host_path_fingerprint(&b.display().to_string());
        assert_ne!(fa, fb);
        assert!(fa.starts_with("sha256:"));
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
