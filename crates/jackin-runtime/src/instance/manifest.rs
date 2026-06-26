//! Instance index (`instances.json`) and per-container manifest: status tracking, session records.
//!
//! The index is the host-side registry of every container jackin' has
//! launched; the per-instance manifest records lifecycle status and agent
//! session history. Not responsible for Docker interaction — purely JSON
//! persistence under `~/.jackin/data/`.

use anyhow::Context;
use fs2::FileExt;
use jackin_core::agent::Agent;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

// Pure index/session data types now live in `jackin-core` so that
// `jackin-console` can use them without depending on `jackin-runtime`.
pub use jackin_core::instance::{
    InstanceIndexEntry, InstanceQuery, InstanceStatus, SessionRecord, SessionStatus,
};

pub const INSTANCE_MANIFEST_VERSION: u32 = 1;
pub const INSTANCE_INDEX_VERSION: u32 = 1;
const INSTANCE_INDEX_FILE: &str = "instances.json";
const INSTANCE_INDEX_LOCK_FILE: &str = "instances.json.lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerResources {
    pub role_container: String,
    /// `DinD` sidecar container name. `None` when the launch used
    /// `dind = "none"` (DinD-free role or `locked`/`hardened` profile without
    /// an explicit `DinD` grant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind_container: Option<String>,
    pub network: String,
    /// `DinD` TLS cert volume name. `None` when there is no `DinD` sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certs_volume: Option<String>,
}

impl DockerResources {
    /// Derive all four Docker resource names from the role container name.
    ///
    /// Invariant: all derived names follow the same suffix conventions used
    /// by `runtime::naming` helpers, so `docker inspect` on any of the four
    /// names produces results consistent with the naming registry.
    pub fn from_container_name(container_name: &str) -> Self {
        Self {
            role_container: container_name.to_owned(),
            dind_container: Some(crate::runtime::naming::dind_container_name(container_name)),
            network: crate::runtime::naming::role_network_name(container_name),
            certs_volume: Some(crate::runtime::naming::dind_certs_volume(container_name)),
        }
    }
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

impl InstanceManifest {
    pub fn new(input: NewInstanceManifest<'_>) -> Self {
        let now = now_rfc3339();
        Self {
            version: INSTANCE_MANIFEST_VERSION,
            instance_id: crate::instance::naming::instance_id_from_container_base(
                input.container_base,
            )
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
            sessions: Vec::new(),
            role_git_sha: input.role_git_sha,
            base_image_ref: input.base_image_ref,
            base_image_digest: input.base_image_digest,
            supported_agents: input.supported_agents,
        }
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
        paths: &jackin_core::paths::JackinPaths,
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
    /// `debug_log!` so unreadable manifests surface under `--debug`
    /// without polluting normal output: callers run inside discovery
    /// loops that iterate every entry in the instance index, so an
    /// always-on warning would emit N lines per command for any
    /// operator carrying a few stale state dirs.
    pub fn read_or_log(state_dir: &Path, site: &str) -> Option<Self> {
        match Self::read_optional(state_dir) {
            Ok(value) => value,
            Err(error) => {
                jackin_diagnostics::debug_log!(
                    "instance",
                    "{site}: skipping {} due to unreadable manifest: {error:#}",
                    state_dir.display(),
                );
                None
            }
        }
    }

    pub fn write(&self, state_dir: &Path) -> anyhow::Result<()> {
        let path = state_dir.join(".jackin/instance.json");
        let body = serde_json::to_string_pretty(self)?;
        jackin_config::atomic_write(&path, &body)
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
            jackin_diagnostics::debug_log!(
                "instance",
                "host_path_fingerprint: canonicalize({path}) failed ({error}); falling back to raw input",
            );
            path.to_owned()
        }
    };
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", crate::instance::naming::hex_lower(&digest))
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
            index.instances.push(manifest.to_index_entry());
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
        #[expect(
            clippy::disallowed_methods,
            reason = "instance index mutation is caller-governed and not part of frame rendering"
        )]
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
                index.instances.push(manifest.to_index_entry());
            }
            // Manifest absent → state already torn down by
            // `purge_container_filesystem`; nothing to tombstone.
            Ok(None) => {}
            Err(error) => {
                // Corrupt manifest: synthesize a minimal tombstone so
                // the operator still sees that this container was
                // purged. Log the read error so `--debug` surfaces the
                // underlying file fault for forensics.
                jackin_diagnostics::debug_log!(
                    "instance",
                    "mark_purged: manifest for `{container_base}` unreadable: {error}; synthesizing tombstone",
                );
                index.instances.push(InstanceIndexEntry {
                    instance_id: container_base.to_owned(),
                    container_base: container_base.to_owned(),
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
            if manifest.to_index_entry().matches(query) {
                manifests.push(manifest);
            }
        }
        manifests.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(manifests)
    }

    /// Errors if the file is missing or unreadable.
    #[cfg(any(test, feature = "test-support"))]
    pub fn read(data_dir: &Path) -> anyhow::Result<Self> {
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
            index.instances.push(manifest.to_index_entry());
        }
        index.sort();
        Ok(index)
    }

    fn write(&self, data_dir: &Path) -> anyhow::Result<()> {
        let body = serde_json::to_string_pretty(self)?;
        jackin_config::atomic_write(&data_dir.join(INSTANCE_INDEX_FILE), &body)
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
mod tests;
