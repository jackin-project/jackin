// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Instance index registry methods.

use super::{
    INSTANCE_INDEX_VERSION, InstanceIndex, InstanceIndexEntry, InstanceManifest, InstanceQuery,
    InstanceStatus,
};
use anyhow::Context;
use fs4::FileExt;

use std::path::Path;

pub(crate) const INSTANCE_INDEX_FILE: &str = "instances.json";
pub(crate) const INSTANCE_INDEX_LOCK_FILE: &str = "instances.json.lock";

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
    pub(crate) fn read_optional(data_dir: &Path) -> anyhow::Result<Option<Self>> {
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
    pub(crate) fn with_lock<F>(data_dir: &Path, mutate: F) -> anyhow::Result<()>
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
        FileExt::lock(&lock)
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
    pub(crate) fn backfill_purge_tombstone(
        index: &mut Self,
        data_dir: &Path,
        container_base: &str,
    ) {
        let state_dir = data_dir.join(container_base);
        match InstanceManifest::read_optional(&state_dir) {
            Ok(Some(mut manifest)) => {
                manifest.mark_status(InstanceStatus::Purged);
                index.instances.push(manifest.to_index_entry());
            }
            // Manifest absent → state already torn down by
            // `purge_container_filesystem`; nothing to tombstone.
            Ok(None) => {}
            Err(_) => {
                // Corrupt manifest: synthesize a minimal tombstone so
                // the operator still sees that this container was purged.
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
            let Some(manifest) = InstanceManifest::read_optional_lossy(&state_dir) else {
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
    pub fn read(data_dir: &Path) -> anyhow::Result<Self> {
        Self::read_optional(data_dir)?.ok_or_else(|| {
            crate::InstanceError::IndexMissing {
                path: data_dir.to_path_buf(),
            }
            .into()
        })
    }

    pub(crate) fn rebuild(data_dir: &Path) -> anyhow::Result<Self> {
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

    pub(crate) fn write(&self, data_dir: &Path) -> anyhow::Result<()> {
        let body = serde_json::to_string_pretty(self)?;
        Ok(jackin_config::atomic_write(
            &data_dir.join(INSTANCE_INDEX_FILE),
            &body,
        )?)
    }

    pub(crate) fn sort(&mut self) {
        self.instances
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
