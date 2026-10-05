// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable custody of shared Docker resources. Callers hold the lifecycle writer
//! lease throughout persistence and daemon mutation.

use anyhow::{Context, ensure};
use jackin_core::{DaemonServerId, JackinPaths, NetworkId};
use rand::TryRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;

const VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 16 * 1024;

/// Pending creation is distinct from both disabled networking and captured ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SharedNetworkCustody {
    Disabled,
    Pending { name: String },
    Owned { name: String, id: NetworkId },
}

/// Volumes lack an immutable Docker ID; their fresh lifetime name is identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SharedCertsVolumeCustody {
    Disabled,
    Pending { name: String },
    Owned { name: String },
}

/// Resource class that owns one shared Docker lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedDockerOwnerKind {
    /// A role container launch.
    Role,
    /// A retained or transient DinD prewarm.
    Prewarm,
}

/// Durable container identity state. A pending name is reserved before the
/// create request; labels let recovery resolve a create whose reply was lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SharedContainerCustody {
    Disabled,
    Pending { name: String },
    Owned { name: String, id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LifetimeDisposition {
    Active,
    Retiring,
    Retired,
}

/// Generation and physical names never change, including across adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedDockerLifetime {
    version: u32,
    disposition: LifetimeDisposition,
    daemon_server_id: DaemonServerId,
    owner_kind: SharedDockerOwnerKind,
    namespace_owner_kind: SharedDockerOwnerKind,
    owner: String,
    namespace_owner: String,
    generation: String,
    network: SharedNetworkCustody,
    certs_volume: SharedCertsVolumeCustody,
    role_container: SharedContainerCustody,
    dind_container: SharedContainerCustody,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LifetimeRecord {
    version: u32,
    disposition: LifetimeDisposition,
    daemon_server_id: DaemonServerId,
    owner_kind: SharedDockerOwnerKind,
    namespace_owner_kind: SharedDockerOwnerKind,
    owner: String,
    namespace_owner: String,
    generation: String,
    network: SharedNetworkCustody,
    certs_volume: SharedCertsVolumeCustody,
    role_container: SharedContainerCustody,
    dind_container: SharedContainerCustody,
}

impl<'de> Deserialize<'de> for SharedDockerLifetime {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = LifetimeRecord::deserialize(deserializer)?;
        let lifetime = Self {
            version: record.version,
            disposition: record.disposition,
            daemon_server_id: record.daemon_server_id,
            owner_kind: record.owner_kind,
            namespace_owner_kind: record.namespace_owner_kind,
            owner: record.owner,
            namespace_owner: record.namespace_owner,
            generation: record.generation,
            network: record.network,
            certs_volume: record.certs_volume,
            role_container: record.role_container,
            dind_container: record.dind_container,
        };
        lifetime.validate().map_err(serde::de::Error::custom)?;
        Ok(lifetime)
    }
}

impl SharedDockerLifetime {
    pub fn fresh(
        daemon_server_id: &DaemonServerId,
        owner: &str,
        network_enabled: bool,
        dind_enabled: bool,
    ) -> anyhow::Result<Self> {
        Self::fresh_for(
            daemon_server_id,
            owner,
            SharedDockerOwnerKind::Role,
            network_enabled,
            dind_enabled,
        )
    }

    /// Reserve the same resource set for a standalone DinD prewarm.
    pub fn fresh_prewarm(daemon_server_id: &DaemonServerId, owner: &str) -> anyhow::Result<Self> {
        Self::fresh_for(
            daemon_server_id,
            owner,
            SharedDockerOwnerKind::Prewarm,
            true,
            true,
        )
    }

    fn fresh_for(
        daemon_server_id: &DaemonServerId,
        owner: &str,
        owner_kind: SharedDockerOwnerKind,
        network_enabled: bool,
        dind_enabled: bool,
    ) -> anyhow::Result<Self> {
        validate_owner(owner)?;
        ensure!(
            !dind_enabled || network_enabled,
            "DinD requires an owned network lifetime"
        );
        let mut entropy = [0_u8; 16];
        rand::rngs::SysRng
            .try_fill_bytes(&mut entropy)
            .context("generating shared Docker lifetime identity")?;
        let generation = hex::encode(entropy);
        Ok(Self {
            version: VERSION,
            disposition: LifetimeDisposition::Active,
            daemon_server_id: daemon_server_id.clone(),
            owner_kind,
            namespace_owner_kind: owner_kind,
            owner: owner.to_owned(),
            namespace_owner: owner.to_owned(),
            network: if network_enabled {
                SharedNetworkCustody::Pending {
                    name: format!("{owner}-net-{generation}"),
                }
            } else {
                SharedNetworkCustody::Disabled
            },
            certs_volume: if dind_enabled {
                SharedCertsVolumeCustody::Pending {
                    name: format!("{owner}-dind-certs-{generation}"),
                }
            } else {
                SharedCertsVolumeCustody::Disabled
            },
            role_container: if owner_kind == SharedDockerOwnerKind::Role {
                SharedContainerCustody::Pending {
                    name: owner.to_owned(),
                }
            } else {
                SharedContainerCustody::Disabled
            },
            dind_container: if dind_enabled {
                SharedContainerCustody::Pending {
                    name: crate::naming::dind_container_name(owner),
                }
            } else {
                SharedContainerCustody::Disabled
            },
            generation,
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }
    #[must_use]
    pub const fn owner_kind(&self) -> SharedDockerOwnerKind {
        self.owner_kind
    }
    #[must_use]
    pub const fn namespace_owner_kind(&self) -> SharedDockerOwnerKind {
        self.namespace_owner_kind
    }
    #[must_use]
    pub const fn daemon_server_id(&self) -> &DaemonServerId {
        &self.daemon_server_id
    }
    #[must_use]
    pub fn namespace_owner(&self) -> &str {
        &self.namespace_owner
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub const fn network(&self) -> &SharedNetworkCustody {
        &self.network
    }
    #[must_use]
    pub const fn certs_volume(&self) -> &SharedCertsVolumeCustody {
        &self.certs_volume
    }
    #[must_use]
    pub const fn role_container(&self) -> &SharedContainerCustody {
        &self.role_container
    }
    #[must_use]
    pub const fn dind_container(&self) -> &SharedContainerCustody {
        &self.dind_container
    }
    #[must_use]
    pub const fn is_retired(&self) -> bool {
        matches!(self.disposition, LifetimeDisposition::Retired)
    }
    #[must_use]
    pub fn network_name(&self) -> Option<&str> {
        match &self.network {
            SharedNetworkCustody::Disabled => None,
            SharedNetworkCustody::Pending { name } | SharedNetworkCustody::Owned { name, .. } => {
                Some(name)
            }
        }
    }
    #[must_use]
    pub const fn network_id(&self) -> Option<&NetworkId> {
        match &self.network {
            SharedNetworkCustody::Owned { id, .. } => Some(id),
            _ => None,
        }
    }
    #[must_use]
    pub fn certs_volume_name(&self) -> Option<&str> {
        match &self.certs_volume {
            SharedCertsVolumeCustody::Disabled => None,
            SharedCertsVolumeCustody::Pending { name }
            | SharedCertsVolumeCustody::Owned { name } => Some(name),
        }
    }
    #[must_use]
    pub fn dind_container_name(&self) -> Option<&str> {
        match &self.dind_container {
            SharedContainerCustody::Disabled => None,
            SharedContainerCustody::Pending { name }
            | SharedContainerCustody::Owned { name, .. } => Some(name),
        }
    }

    pub fn capture_network(&mut self, id: NetworkId) -> anyhow::Result<()> {
        ensure!(
            self.disposition != LifetimeDisposition::Retired,
            "shared Docker lifetime is retired"
        );
        match &self.network {
            SharedNetworkCustody::Pending { name } => {
                self.network = SharedNetworkCustody::Owned {
                    name: name.clone(),
                    id,
                };
                Ok(())
            }
            SharedNetworkCustody::Owned { id: captured, .. } if *captured == id => Ok(()),
            _ => anyhow::bail!("network custody cannot be recaptured or enabled after allocation"),
        }
    }

    pub fn capture_certs_volume(&mut self) -> anyhow::Result<()> {
        ensure!(
            self.disposition != LifetimeDisposition::Retired,
            "shared Docker lifetime is retired"
        );
        ensure!(
            matches!(self.network, SharedNetworkCustody::Owned { .. }),
            "certificate volume capture requires captured network custody"
        );
        match &self.certs_volume {
            SharedCertsVolumeCustody::Pending { name } => {
                self.certs_volume = SharedCertsVolumeCustody::Owned { name: name.clone() };
                Ok(())
            }
            SharedCertsVolumeCustody::Owned { .. } => Ok(()),
            SharedCertsVolumeCustody::Disabled => {
                anyhow::bail!("certificate volume was disabled at allocation")
            }
        }
    }

    /// Capture the role or sidecar container ID immediately after Docker
    /// returns it. Safe to repeat with the same ID during recovery.
    pub fn capture_container(&mut self, dind: bool, id: &str) -> anyhow::Result<()> {
        ensure!(
            self.disposition != LifetimeDisposition::Retired,
            "shared Docker lifetime is retired"
        );
        ensure!(
            is_lower_hex(id, 64),
            "Docker container ID must be 64 lowercase hexadecimal characters"
        );
        let custody = if dind {
            &mut self.dind_container
        } else {
            &mut self.role_container
        };
        match custody {
            SharedContainerCustody::Pending { name } => {
                *custody = SharedContainerCustody::Owned {
                    name: name.clone(),
                    id: id.to_owned(),
                };
                Ok(())
            }
            SharedContainerCustody::Owned { id: captured, .. } if captured == id => Ok(()),
            _ => {
                anyhow::bail!("container custody cannot be recaptured or enabled after allocation")
            }
        }
    }

    /// Atomically move the only canonical authority to the next owner.
    pub fn transfer(&self, paths: &JackinPaths, owner: &str) -> anyhow::Result<Self> {
        validate_owner(owner)?;
        ensure!(
            self.disposition == LifetimeDisposition::Active,
            "shared Docker lifetime is retired"
        );
        ensure!(
            self.owner_kind == SharedDockerOwnerKind::Prewarm
                && self.namespace_owner_kind == SharedDockerOwnerKind::Prewarm,
            "only a prewarm lifetime can transfer to a role owner"
        );
        ensure!(
            matches!(self.network, SharedNetworkCustody::Owned { .. })
                && matches!(self.certs_volume, SharedCertsVolumeCustody::Owned { .. })
                && matches!(self.dind_container, SharedContainerCustody::Owned { .. }),
            "prewarm transfer requires captured shared resource identities"
        );
        let current = Self::read_record(&self.path(paths), &self.daemon_server_id)?
            .context("cannot transfer missing shared Docker lifetime custody")?;
        ensure!(
            current == *self,
            "cannot transfer a changed shared Docker lifetime"
        );
        ensure!(
            Self::load(paths, &self.daemon_server_id, &self.owner)?.as_ref() == Some(self),
            "cannot transfer ambiguous shared Docker lifetime custody"
        );
        if owner != self.owner {
            ensure!(
                Self::load_for_cleanup(paths, &self.daemon_server_id, owner)?.is_none(),
                "target owner already has a shared Docker lifetime"
            );
        }
        let mut transferred = self.clone();
        transferred.owner = owner.to_owned();
        transferred.owner_kind = SharedDockerOwnerKind::Role;
        transferred.role_container = SharedContainerCustody::Pending {
            name: owner.to_owned(),
        };
        transferred.validate()?;
        transferred.persist_record(paths)?;
        Ok(transferred)
    }

    /// Must complete successfully before the first Docker creation request.
    pub fn save_pending(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        self.validate()?;
        ensure!(
            self.disposition == LifetimeDisposition::Active,
            "shared Docker lifetime is retired"
        );
        ensure!(
            !matches!(self.network, SharedNetworkCustody::Owned { .. })
                && !matches!(self.certs_volume, SharedCertsVolumeCustody::Owned { .. })
                && !matches!(self.role_container, SharedContainerCustody::Owned { .. })
                && !matches!(self.dind_container, SharedContainerCustody::Owned { .. }),
            "initial shared Docker lifetime must be pending or explicitly disabled"
        );
        ensure!(
            Self::read_record(&self.path(paths), &self.daemon_server_id)?.is_none(),
            "shared Docker generation was already allocated or retired"
        );
        ensure!(
            Self::load_for_cleanup(paths, &self.daemon_server_id, &self.owner)?.is_none(),
            "unretired shared Docker lifetime already exists for {}",
            self.owner
        );
        self.persist_record(paths)
    }

    /// Atomically save capture progress without replacing another generation.
    /// The caller's writer lease excludes concurrent load/save/retire races.
    pub fn save(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        self.validate()?;
        ensure!(
            self.disposition != LifetimeDisposition::Retired,
            "shared Docker lifetime is retired"
        );
        {
            let previous = Self::read_record(&self.path(paths), &self.daemon_server_id)?
                .context("shared Docker lifetime must be persisted pending before capture")?;
            ensure!(
                previous.disposition == self.disposition,
                "shared Docker generation disposition cannot regress or advance during capture"
            );
            ensure!(
                previous.owner == self.owner,
                "shared Docker lifetime ownership requires atomic transfer"
            );
            ensure!(
                previous.generation == self.generation
                    && previous.namespace_owner == self.namespace_owner,
                "unretired shared Docker lifetime already exists for {}",
                self.owner
            );
            ensure!(
                previous.owner_kind == self.owner_kind
                    && previous.namespace_owner_kind == self.namespace_owner_kind,
                "shared Docker owner kind cannot change during capture"
            );
            ensure!(
                previous.network_name() == self.network_name()
                    && previous.certs_volume_name() == self.certs_volume_name(),
                "shared Docker physical names cannot change"
            );
            ensure!(
                previous.network_id().is_none() || previous.network_id() == self.network_id(),
                "captured network identity cannot change or regress"
            );
            ensure!(
                !matches!(
                    previous.certs_volume,
                    SharedCertsVolumeCustody::Owned { .. }
                ) || matches!(self.certs_volume, SharedCertsVolumeCustody::Owned { .. }),
                "captured certificate volume cannot regress"
            );
            ensure!(
                container_capture_does_not_regress(&previous.role_container, &self.role_container),
                "captured role container identity cannot change or regress"
            );
            ensure!(
                container_capture_does_not_regress(&previous.dind_container, &self.dind_container),
                "captured DinD container identity cannot change or regress"
            );
        }
        if let Some(previous) = Self::load_for_cleanup(paths, &self.daemon_server_id, &self.owner)?
        {
            ensure!(
                previous.generation == self.generation,
                "unretired shared Docker lifetime already exists for {}",
                self.owner
            );
        }
        self.persist_record(paths)
    }

    fn persist_record(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        let path = self.path(paths);
        let parent = path
            .parent()
            .context("shared Docker lifetime path has no parent")?;
        std::fs::create_dir_all(parent)?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(staged.as_file_mut(), self)?;
        staged.write_all(b"\n")?;
        staged.as_file().sync_all()?;
        staged
            .persist(&path)
            .map_err(|error| error.error)
            .with_context(|| format!("persisting shared Docker lifetime at {}", path.display()))?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }

    pub fn load(
        paths: &JackinPaths,
        daemon_server_id: &DaemonServerId,
        owner: &str,
    ) -> anyhow::Result<Option<Self>> {
        Ok(Self::load_for_cleanup(paths, daemon_server_id, owner)?
            .filter(|record| record.disposition == LifetimeDisposition::Active))
    }

    /// Load active or retiring authority for idempotent recovery cleanup.
    pub fn load_for_cleanup(
        paths: &JackinPaths,
        daemon_server_id: &DaemonServerId,
        owner: &str,
    ) -> anyhow::Result<Option<Self>> {
        validate_owner(owner)?;
        let mut selected = None;
        Self::visit_records(paths, daemon_server_id, |record| {
            if record.disposition != LifetimeDisposition::Retired && record.owner == owner {
                ensure!(
                    selected.is_none(),
                    "multiple shared Docker lifetimes claim owner {owner}"
                );
                selected = Some(record);
            }
            Ok(())
        })?;
        Ok(selected)
    }

    pub fn retired_for_owner(
        paths: &JackinPaths,
        daemon_server_id: &DaemonServerId,
        owner: &str,
    ) -> anyhow::Result<Vec<Self>> {
        validate_owner(owner)?;
        let mut retired = Vec::new();
        Self::visit_records(paths, daemon_server_id, |record| {
            if record.disposition == LifetimeDisposition::Retired && record.owner == owner {
                retired.push(record);
            }
            Ok(())
        })?;
        Ok(retired)
    }

    /// Read every daemon namespace before global lifecycle effects. Unknown
    /// storage and invalid tombstones fail the census; no record is inferred
    /// from Docker labels or from the currently selected daemon.
    pub fn inventory_all(paths: &JackinPaths) -> anyhow::Result<Vec<Self>> {
        let root = paths.jackin_home.join("shared-docker-lifetimes");
        let metadata = match std::fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error).context("inspecting shared Docker lifetime inventory root");
            }
        };
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "shared Docker lifetime inventory root must be a real directory"
        );
        let mut active = Vec::new();
        let mut owners = std::collections::HashSet::new();
        for namespace in std::fs::read_dir(&root)? {
            let namespace = namespace?;
            let metadata = namespace.file_type()?;
            ensure!(
                metadata.is_dir() && !metadata.is_symlink(),
                "shared Docker lifetime namespace must be a real directory"
            );
            let namespace_path = namespace.path();
            let namespace_name = namespace.file_name();
            let namespace_name = namespace_name
                .to_str()
                .context("shared Docker lifetime namespace is not UTF-8")?;
            ensure!(
                is_lower_hex(namespace_name, 64),
                "unknown shared Docker lifetime namespace path"
            );
            for entry in std::fs::read_dir(&namespace_path)? {
                let entry = entry?;
                let metadata = entry.file_type()?;
                ensure!(
                    metadata.is_file() && !metadata.is_symlink(),
                    "shared Docker lifetime inventory entry must be a regular file"
                );
                let path = entry.path();
                let name = entry.file_name();
                let name = name
                    .to_str()
                    .context("shared Docker lifetime file name is not UTF-8")?;
                ensure!(
                    name.strip_suffix(".json")
                        .is_some_and(|generation| is_lower_hex(generation, 32)),
                    "unknown shared Docker lifetime storage path"
                );
                let record = Self::read_unscoped_record(&path)?
                    .context("shared Docker lifetime disappeared during inventory")?;
                ensure!(
                    store_dir(paths, record.daemon_server_id()) == namespace_path,
                    "shared Docker lifetime daemon ID does not match its namespace"
                );
                if record.disposition != LifetimeDisposition::Retired {
                    ensure!(
                        owners.insert((record.daemon_server_id.clone(), record.owner.clone())),
                        "multiple shared Docker lifetimes claim one daemon owner"
                    );
                    active.push(record);
                }
            }
        }
        Ok(active)
    }

    fn visit_records(
        paths: &JackinPaths,
        daemon_server_id: &DaemonServerId,
        mut visit: impl FnMut(Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let directory = store_dir(paths, daemon_server_id);
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("reading shared Docker lifetime store"),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            // Atomic writes stage non-JSON temporary files in this directory.
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let record = Self::read_record(&path, daemon_server_id)?
                .context("shared Docker lifetime disappeared while scanning custody")?;
            visit(record)?;
        }
        Ok(())
    }

    fn read_record(
        path: &std::path::Path,
        daemon_server_id: &DaemonServerId,
    ) -> anyhow::Result<Option<Self>> {
        let record = Self::read_unscoped_record(path)?;
        if let Some(record) = &record {
            ensure!(
                record.daemon_server_id == *daemon_server_id,
                "shared Docker lifetime belongs to another daemon server"
            );
        }
        Ok(record)
    }

    fn read_unscoped_record(path: &std::path::Path) -> anyhow::Result<Option<Self>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("reading shared Docker lifetime at {}", path.display())
                });
            }
        };
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file(),
            "shared Docker lifetime must be a regular file"
        );
        ensure!(
            metadata.len() <= MAX_RECORD_BYTES,
            "shared Docker lifetime exceeds record bound"
        );
        let mut bytes = Vec::new();
        file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_RECORD_BYTES,
            "shared Docker lifetime exceeds record bound"
        );
        let record: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing shared Docker lifetime at {}", path.display()))?;
        ensure!(
            path.file_name()
                .is_some_and(|name| name == format!("{}.json", record.generation).as_str()),
            "shared Docker lifetime generation does not match its canonical record path"
        );
        Ok(Some(record))
    }

    /// Persist cleanup intent before the first destructive effect. Repeating
    /// this call resumes the same immutable generation.
    pub fn begin_retirement(&self, paths: &JackinPaths) -> anyhow::Result<Self> {
        ensure!(
            self.disposition != LifetimeDisposition::Retired,
            "shared Docker lifetime is already retired"
        );
        let current = Self::read_record(&self.path(paths), &self.daemon_server_id)?
            .context("cannot retire missing shared Docker lifetime custody")?;
        let mut expected = self.clone();
        expected.disposition = current.disposition;
        ensure!(
            current == expected,
            "cannot retire a changed shared Docker lifetime"
        );
        if current.disposition == LifetimeDisposition::Retiring {
            return Ok(current);
        }
        ensure!(
            current.disposition == LifetimeDisposition::Active,
            "only active shared Docker custody can begin retirement"
        );
        let mut retiring = self.clone();
        retiring.disposition = LifetimeDisposition::Retiring;
        retiring.persist_record(paths)?;
        Ok(retiring)
    }

    /// Call only after resources and their instance records are durably removed.
    /// The tombstone permanently prevents replay of this generation.
    pub fn retire(&self, paths: &JackinPaths) -> anyhow::Result<()> {
        ensure!(
            self.disposition == LifetimeDisposition::Retiring,
            "shared Docker lifetime must be retiring before retirement"
        );
        let current = Self::read_record(&self.path(paths), &self.daemon_server_id)?
            .context("cannot retire missing shared Docker lifetime custody")?;
        ensure!(
            current == *self,
            "cannot retire a changed shared Docker lifetime"
        );
        ensure!(
            Self::load_for_cleanup(paths, &self.daemon_server_id, &self.owner)?.as_ref()
                == Some(self),
            "cannot retire ambiguous shared Docker lifetime custody"
        );
        let mut retired = self.clone();
        retired.disposition = LifetimeDisposition::Retired;
        retired.persist_record(paths)
    }

    fn path(&self, paths: &JackinPaths) -> PathBuf {
        store_dir(paths, &self.daemon_server_id).join(format!("{}.json", self.generation))
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.version == VERSION,
            "unsupported shared Docker lifetime version {}",
            self.version
        );
        validate_owner(&self.owner)?;
        validate_owner(&self.namespace_owner)?;
        ensure!(
            is_lower_hex(&self.generation, 32),
            "shared Docker generation must contain 128 bits of lowercase hexadecimal identity"
        );
        match (self.owner_kind, &self.role_container) {
            (
                SharedDockerOwnerKind::Role,
                SharedContainerCustody::Pending { name }
                | SharedContainerCustody::Owned { name, .. },
            ) => ensure!(
                name == &self.owner,
                "role container name differs from lifetime owner"
            ),
            (SharedDockerOwnerKind::Role, SharedContainerCustody::Disabled) => {
                anyhow::bail!("role lifetime must reserve its role container")
            }
            (SharedDockerOwnerKind::Prewarm, SharedContainerCustody::Disabled) => {}
            (SharedDockerOwnerKind::Prewarm, _) => {
                anyhow::bail!("prewarm lifetime cannot own a role container")
            }
        }
        ensure!(
            self.namespace_owner_kind == SharedDockerOwnerKind::Prewarm
                || self.namespace_owner_kind == SharedDockerOwnerKind::Role,
            "invalid physical Docker namespace owner kind"
        );
        if self.namespace_owner_kind == SharedDockerOwnerKind::Prewarm {
            ensure!(
                self.certs_volume_name().is_some(),
                "physical prewarm owner must retain DinD resources"
            );
            ensure!(
                self.owner_kind == SharedDockerOwnerKind::Role
                    || self.owner == self.namespace_owner,
                "prewarm owner transfer changed its immutable Docker namespace"
            );
        } else {
            ensure!(
                self.owner_kind == SharedDockerOwnerKind::Role,
                "role namespace cannot be owned by a prewarm record"
            );
        }
        match (&self.dind_container, self.certs_volume_name()) {
            (SharedContainerCustody::Disabled, None) => {}
            (
                SharedContainerCustody::Pending { name }
                | SharedContainerCustody::Owned { name, .. },
                Some(_),
            ) => ensure!(
                name == &crate::naming::dind_container_name(&self.namespace_owner),
                "DinD container name differs from lifetime namespace owner"
            ),
            _ => anyhow::bail!("DinD container and certificate volume allocation differ"),
        }
        if self.owner_kind == SharedDockerOwnerKind::Prewarm {
            ensure!(
                self.certs_volume_name().is_some(),
                "prewarm lifetime must reserve DinD resources"
            );
        }
        for custody in [&self.role_container, &self.dind_container] {
            if let SharedContainerCustody::Owned { id, .. } = custody {
                ensure!(
                    is_lower_hex(id, 64),
                    "Docker container ID must be 64 lowercase hexadecimal characters"
                );
            }
        }
        if let Some(name) = self.network_name() {
            ensure!(
                name == format!("{}-net-{}", self.namespace_owner, self.generation),
                "network name does not match immutable lifetime"
            );
        }
        if let Some(name) = self.certs_volume_name() {
            ensure!(
                name == format!("{}-dind-certs-{}", self.namespace_owner, self.generation),
                "certificate volume name does not match immutable lifetime"
            );
            ensure!(
                self.network_name().is_some(),
                "DinD certificate volume requires networking"
            );
        }
        ensure!(
            !matches!(self.certs_volume, SharedCertsVolumeCustody::Owned { .. })
                || matches!(self.network, SharedNetworkCustody::Owned { .. }),
            "captured certificate volume requires captured network custody"
        );
        Ok(())
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn container_capture_does_not_regress(
    previous: &SharedContainerCustody,
    current: &SharedContainerCustody,
) -> bool {
    match (previous, current) {
        (
            SharedContainerCustody::Owned {
                name: previous_name,
                id: previous_id,
            },
            SharedContainerCustody::Owned { name, id },
        ) => previous_name == name && previous_id == id,
        (SharedContainerCustody::Owned { .. }, _) => false,
        (
            SharedContainerCustody::Pending {
                name: previous_name,
            },
            SharedContainerCustody::Pending { name },
        ) => previous_name == name,
        (
            SharedContainerCustody::Pending {
                name: previous_name,
            },
            SharedContainerCustody::Owned { name, .. },
        ) => previous_name == name,
        (SharedContainerCustody::Disabled, SharedContainerCustody::Disabled) => true,
        _ => false,
    }
}

fn store_dir(paths: &JackinPaths, daemon_server_id: &DaemonServerId) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(b"jackin-shared-docker-lifetimes-v1\0");
    digest.update(daemon_server_id.as_str().as_bytes());
    paths
        .jackin_home
        .join("shared-docker-lifetimes")
        .join(hex::encode(digest.finalize()))
}

fn validate_owner(owner: &str) -> anyhow::Result<()> {
    ensure!(
        !owner.is_empty()
            && owner.len() <= 128
            && owner.as_bytes()[0].is_ascii_alphanumeric()
            && owner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "shared Docker lifetime owner must be a bounded resource name"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon() -> DaemonServerId {
        DaemonServerId::parse("test-daemon-A").unwrap()
    }

    #[test]
    fn fresh_generations_have_distinct_physical_names() -> anyhow::Result<()> {
        let first = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        let second = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        assert_ne!(first.generation(), second.generation());
        assert_ne!(first.network_name(), second.network_name());
        assert_ne!(first.certs_volume_name(), second.certs_volume_name());
        assert!(matches!(
            first.network(),
            SharedNetworkCustody::Pending { .. }
        ));
        assert!(first.network_id().is_none());
        Ok(())
    }

    #[test]
    fn missing_unknown_and_malformed_custody_are_rejected() -> anyhow::Result<()> {
        let lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        let original = serde_json::to_value(&lifetime)?;
        for field in [
            "version",
            "disposition",
            "daemon_server_id",
            "owner",
            "namespace_owner",
            "generation",
            "network",
            "certs_volume",
        ] {
            let mut record = original.clone();
            record.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<SharedDockerLifetime>(record).is_err());
        }
        for network in [
            serde_json::json!({"state": "unknown"}),
            serde_json::json!({"state": "disabled", "name": "attacker"}),
            serde_json::json!({"state": "owned", "name": lifetime.network_name(), "id": "short"}),
            serde_json::json!({"state": "pending", "name": "attacker"}),
        ] {
            let mut record = original.clone();
            record["network"] = network;
            assert!(serde_json::from_value::<SharedDockerLifetime>(record).is_err());
        }
        let mut record = original;
        record["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SharedDockerLifetime>(record).is_err());
        Ok(())
    }

    #[test]
    fn pending_capture_and_retirement_preserve_one_lifetime() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let mut lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        lifetime.save_pending(&paths)?;
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?,
            Some(lifetime.clone())
        );
        let pending = lifetime.clone();
        let id = NetworkId::parse(&"a".repeat(64))?;
        lifetime.capture_network(id.clone())?;
        lifetime.capture_certs_volume()?;
        lifetime.save(&paths)?;
        assert!(pending.save(&paths).is_err());
        assert!(pending.retire(&paths).is_err());
        assert!(lifetime.save_pending(&paths).is_err());
        assert!(
            lifetime
                .capture_network(NetworkId::parse(&"b".repeat(64))?)
                .is_err()
        );
        let next = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        assert!(next.save_pending(&paths).is_err());
        lifetime.retire(&paths)?;
        next.save_pending(&paths)?;
        Ok(())
    }

    #[test]
    fn transfer_preserves_generation_and_names() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let mut original = SharedDockerLifetime::fresh(&daemon(), "jackin-prewarm", true, true)?;
        original.save_pending(&paths)?;
        original.capture_network(NetworkId::parse(&"a".repeat(64))?)?;
        original.capture_certs_volume()?;
        original.save(&paths)?;
        let adopted = original.transfer(&paths, "jackin-role")?;
        assert_eq!(adopted.generation(), original.generation());
        assert_eq!(adopted.network_name(), original.network_name());
        assert_eq!(adopted.certs_volume_name(), original.certs_volume_name());
        adopted.save(&paths)?;
        assert!(original.retire(&paths).is_err());
        assert!(original.save(&paths).is_err());
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?,
            Some(adopted)
        );
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-prewarm")?.is_none());
        Ok(())
    }

    #[test]
    fn disabled_network_is_explicit_and_cannot_be_captured() -> anyhow::Result<()> {
        let mut disabled = SharedDockerLifetime::fresh(&daemon(), "jackin-role", false, false)?;
        assert!(matches!(disabled.network(), SharedNetworkCustody::Disabled));
        assert!(disabled.network_name().is_none());
        assert!(
            disabled
                .capture_network(NetworkId::parse(&"a".repeat(64))?)
                .is_err()
        );
        assert!(disabled.capture_certs_volume().is_err());
        assert!(SharedDockerLifetime::fresh(&daemon(), "jackin-role", false, true).is_err());
        for owner in ["", "../role", "/role", ".", ".."] {
            assert!(SharedDockerLifetime::fresh(&daemon(), owner, true, true).is_err());
        }
        Ok(())
    }

    #[test]
    fn oversized_and_wrong_owner_records_are_rejected() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, false)?;
        lifetime.save_pending(&paths)?;
        let path = lifetime.path(&paths);
        std::fs::write(&path, vec![b' '; (MAX_RECORD_BYTES + 1) as usize])?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-role").is_err());
        let other = SharedDockerLifetime::fresh(&daemon(), "jackin-other", true, false)?;
        std::fs::write(&path, serde_json::to_vec(&other)?)?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-role").is_err());
        Ok(())
    }

    #[test]
    fn daemon_custody_isolated_even_when_record_is_copied() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let daemon_b = DaemonServerId::parse("test-daemon-B")?;
        let original = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, false)?;
        original.save_pending(&paths)?;
        assert!(SharedDockerLifetime::load(&paths, &daemon_b, "jackin-role")?.is_none());
        let mut foreign_snapshot = original.clone();
        foreign_snapshot.daemon_server_id = daemon_b.clone();
        assert!(foreign_snapshot.retire(&paths).is_err());
        let foreign_directory = store_dir(&paths, &daemon_b);
        std::fs::create_dir_all(&foreign_directory)?;
        std::fs::copy(original.path(&paths), foreign_snapshot.path(&paths))?;
        assert!(SharedDockerLifetime::load(&paths, &daemon_b, "jackin-role").is_err());
        assert!(foreign_snapshot.save(&paths).is_err());
        assert!(foreign_snapshot.retire(&paths).is_err());
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?,
            Some(original)
        );
        Ok(())
    }

    #[test]
    fn custody_survives_disposable_container_tree_purge() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, false)?;
        lifetime.save_pending(&paths)?;
        let disposable = paths.data_dir.join(lifetime.owner());
        std::fs::create_dir_all(&disposable)?;
        std::fs::write(disposable.join("instance.json"), b"disposable")?;
        std::fs::remove_dir_all(&paths.data_dir)?;
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), lifetime.owner())?,
            Some(lifetime)
        );
        Ok(())
    }

    #[test]
    fn duplicate_owner_generations_are_rejected() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let first = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, false)?;
        let second = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, false)?;
        first.save_pending(&paths)?;
        assert!(second.save_pending(&paths).is_err());
        std::fs::write(second.path(&paths), serde_json::to_vec(&second)?)?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-role").is_err());
        Ok(())
    }

    #[test]
    fn captured_custody_cannot_be_saved_without_pending_record() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let mut lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        lifetime.capture_network(NetworkId::parse(&"a".repeat(64))?)?;
        lifetime.capture_certs_volume()?;
        assert!(lifetime.save(&paths).is_err());
        assert!(lifetime.save_pending(&paths).is_err());
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?.is_none());
        assert!(!lifetime.path(&paths).exists());
        Ok(())
    }

    #[test]
    fn retirement_tombstone_rejects_pending_and_owned_replays() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let pending = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        pending.save_pending(&paths)?;
        let mut owned = pending.clone();
        owned.capture_network(NetworkId::parse(&"a".repeat(64))?)?;
        owned.capture_certs_volume()?;
        owned.save(&paths)?;
        owned.retire(&paths)?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?.is_none());
        assert!(owned.path(&paths).exists());
        let tombstone = SharedDockerLifetime::read_record(&owned.path(&paths), &daemon())?.unwrap();
        assert_eq!(tombstone.disposition, LifetimeDisposition::Retired);
        assert!(owned.save(&paths).is_err());
        assert!(owned.save_pending(&paths).is_err());
        assert!(pending.save(&paths).is_err());
        assert!(pending.save_pending(&paths).is_err());
        assert!(owned.transfer(&paths, "jackin-next").is_err());
        assert!(owned.retire(&paths).is_err());
        let fresh = SharedDockerLifetime::fresh(&daemon(), "jackin-role", true, true)?;
        fresh.save_pending(&paths)?;
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), "jackin-role")?,
            Some(fresh)
        );
        Ok(())
    }

    #[test]
    fn retained_history_has_no_fixed_generation_limit() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        let mut tombstone = SharedDockerLifetime::fresh(&daemon(), "jackin-retired", false, false)?;
        tombstone.disposition = LifetimeDisposition::Retired;
        let directory = store_dir(&paths, &daemon());
        std::fs::create_dir_all(&directory)?;
        for index in 0..4096 {
            tombstone.generation = format!("{index:032x}");
            std::fs::write(tombstone.path(&paths), serde_json::to_vec(&tombstone)?)?;
        }
        std::fs::write(directory.join(".staged-non-json"), b"temporary")?;
        let last = SharedDockerLifetime::fresh(&daemon(), "jackin-last", false, false)?;
        last.save_pending(&paths)?;
        let excess = SharedDockerLifetime::fresh(&daemon(), "jackin-excess", false, false)?;
        excess.save_pending(&paths)?;
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), excess.owner())?,
            Some(excess.clone())
        );
        assert_eq!(
            SharedDockerLifetime::load(&paths, &daemon(), last.owner())?,
            Some(last.clone())
        );
        last.save(&paths)?;
        let transferred = last.transfer(&paths, "jackin-transferred")?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), last.owner())?.is_none());
        transferred.retire(&paths)?;
        assert!(SharedDockerLifetime::load(&paths, &daemon(), transferred.owner())?.is_none());
        excess.save(&paths)?;
        excess.retire(&paths)?;
        Ok(())
    }

    #[test]
    fn inventory_covers_all_daemons_pending_owners_and_validates_tombstones() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let paths = JackinPaths::for_tests(temp.path());
        assert!(SharedDockerLifetime::inventory_all(&paths)?.is_empty());
        let pending = SharedDockerLifetime::fresh(&daemon(), "jackin-prewarm", true, true)?;
        pending.save_pending(&paths)?;
        let daemon_b = DaemonServerId::parse("test-daemon-B")?;
        let orphan = SharedDockerLifetime::fresh(&daemon_b, "jackin-orphan", true, false)?;
        orphan.save_pending(&paths)?;
        let retired = SharedDockerLifetime::fresh(&daemon(), "jackin-retired", false, false)?;
        retired.save_pending(&paths)?;
        retired.retire(&paths)?;
        let inventory = SharedDockerLifetime::inventory_all(&paths)?;
        assert_eq!(inventory.len(), 2);
        assert!(inventory.contains(&pending));
        assert!(inventory.contains(&orphan));
        let mut corrupt_tombstone = serde_json::to_value(
            SharedDockerLifetime::read_record(&retired.path(&paths), &daemon())?.unwrap(),
        )?;
        corrupt_tombstone["version"] = serde_json::json!(999);
        std::fs::write(
            retired.path(&paths),
            serde_json::to_vec(&corrupt_tombstone)?,
        )?;
        assert!(SharedDockerLifetime::inventory_all(&paths).is_err());
        Ok(())
    }

    #[test]
    fn inventory_rejects_unknown_paths_namespace_mismatch_and_corrupt_records() -> anyhow::Result<()>
    {
        for variant in [
            "namespace",
            "filename",
            "unknown_file",
            "unknown_directory",
            "corrupt",
            "duplicate",
        ] {
            let temp = tempfile::tempdir()?;
            let paths = JackinPaths::for_tests(temp.path());
            let lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", false, false)?;
            lifetime.save_pending(&paths)?;
            let directory = store_dir(&paths, &daemon());
            match variant {
                "namespace" => {
                    std::fs::rename(&directory, directory.parent().unwrap().join("0".repeat(64)))?;
                }
                "filename" => {
                    std::fs::rename(
                        lifetime.path(&paths),
                        directory.join(format!("{}.json", "0".repeat(32))),
                    )?;
                }
                "unknown_file" => {
                    std::fs::write(directory.join(".staged-unknown"), b"unknown")?;
                }
                "unknown_directory" => {
                    std::fs::create_dir(directory.parent().unwrap().join("unknown"))?;
                }
                "corrupt" => {
                    std::fs::write(lifetime.path(&paths), b"not JSON")?;
                }
                "duplicate" => {
                    let duplicate =
                        SharedDockerLifetime::fresh(&daemon(), lifetime.owner(), false, false)?;
                    std::fs::write(duplicate.path(&paths), serde_json::to_vec(&duplicate)?)?;
                }
                _ => unreachable!(),
            }
            assert!(
                SharedDockerLifetime::inventory_all(&paths).is_err(),
                "{variant}"
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn inventory_rejects_symlinks_at_every_storage_level() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;
        for variant in ["root", "namespace", "file"] {
            let temp = tempfile::tempdir()?;
            let paths = JackinPaths::for_tests(temp.path());
            let lifetime = SharedDockerLifetime::fresh(&daemon(), "jackin-role", false, false)?;
            lifetime.save_pending(&paths)?;
            let path = match variant {
                "root" => paths.jackin_home.join("shared-docker-lifetimes"),
                "namespace" => store_dir(&paths, &daemon()),
                "file" => lifetime.path(&paths),
                _ => unreachable!(),
            };
            let displaced = temp.path().join("displaced");
            std::fs::rename(&path, &displaced)?;
            symlink(displaced, path)?;
            assert!(
                SharedDockerLifetime::inventory_all(&paths).is_err(),
                "{variant}"
            );
        }
        Ok(())
    }
}
