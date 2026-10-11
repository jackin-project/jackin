// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker configuration and socket aliases.

use std::fs::{self};
use std::io::ErrorKind;

use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

use std::path::{Path, PathBuf};

use std::time::Duration;

use nix::unistd::geteuid;
use sha2::{Digest as _, Sha256};

use jackin_usage_coordinator::UsageCoordinatorConfig;

use crate::{
    BROKER_DIR, BROKER_IDLE_EXIT, BROKER_LEASE_DURATION, BROKER_LEASE_RENEWAL, BROKER_RUN_DIR,
    BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX, UNIX_SOCKET_PATH_LIMIT, UsageBrokerClient,
    unavailable,
};
/// Host broker filesystem and handshake configuration.
#[derive(Debug, Clone)]
pub struct UsageBrokerConfig {
    /// Host-only jackin data directory. Never mounted into a Capsule.
    pub data_dir: PathBuf,
    /// Exact caller build identifier.
    pub build_id: String,
    /// Bounded refresh scheduling policy.
    pub coordinator: UsageCoordinatorConfig,
    /// Idle lifetime after the last client while no generation is active.
    pub idle_exit: Duration,
    /// Lease lifetime used by the process-independent authority.
    pub lease_duration: Duration,
    /// Lease renewal cadence.
    pub lease_renewal: Duration,
    /// Optional sibling broker executable used by process activation.
    pub service_executable: Option<PathBuf>,
}

impl UsageBrokerConfig {
    /// Build production defaults for one host data directory.
    #[must_use]
    pub fn for_data_dir(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            coordinator: UsageCoordinatorConfig::default(),
            idle_exit: BROKER_IDLE_EXIT,
            lease_duration: BROKER_LEASE_DURATION,
            lease_renewal: BROKER_LEASE_RENEWAL,
            service_executable: default_service_executable(),
        }
    }

    pub(crate) fn socket_path(&self) -> PathBuf {
        let full = self
            .data_dir
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .join(BROKER_SOCKET);
        short_socket_alias_path(&full).unwrap_or(full)
    }

    /// Prepare the host socket rendezvous for a service bind. Passive clients
    /// use `client()` and never create or chmod directories.
    pub(crate) fn prepare_socket_path(
        &self,
    ) -> Result<PathBuf, jackin_protocol::usage_broker::UsageCoordinationError> {
        let full = self
            .data_dir
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .join(BROKER_SOCKET);
        if full.as_os_str().len() < UNIX_SOCKET_PATH_LIMIT {
            return Ok(self.socket_path());
        }
        short_socket_alias(&full).ok_or_else(unavailable)
    }

    /// Construct a fail-closed client even when broker startup is unavailable.
    #[must_use]
    pub fn client(&self) -> UsageBrokerClient {
        let full = self
            .data_dir
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .join(BROKER_SOCKET);
        UsageBrokerClient::at_host(
            short_socket_alias_path(&full).unwrap_or(full),
            self.data_dir.clone(),
            self.build_id.clone(),
        )
    }
}

/// Deterministic short alias for a broker socket path that exceeds the
/// platform `sun_path` limit (deep test tempdirs, long `$HOME`).
///
/// Returns `None` when the full path fits or the alias directory cannot be
/// provisioned. Client and server derive the same alias from the same
/// `data_dir`, so no rendezvous state is needed. The alias directory is
/// per-uid, `0700`, and ownership-validated like the run directory; the
/// socket file itself keeps the existing `0600` + ownership checks at bind
/// time. Distinct data directories map to distinct alias names via the
/// 64-bit SHA-256 prefix of the full path.
pub fn short_socket_alias(full: &Path) -> Option<PathBuf> {
    let alias = short_socket_alias_path(full)?;
    let dir = alias.parent()?;
    validate_trusted_ancestors(dir.parent()?)?;
    match fs::create_dir(dir) {
        Ok(()) => fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).ok()?,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(_) => return None,
    }
    let metadata = fs::symlink_metadata(dir).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return None;
    }
    Some(alias)
}

fn validate_trusted_ancestors(path: &Path) -> Option<()> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::RootDir => prefix.push(component.as_os_str()),
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => return None,
            std::path::Component::Normal(_) | std::path::Component::Prefix(_) => {
                prefix.push(component.as_os_str());
            }
        }
        let link_metadata = fs::symlink_metadata(&prefix).ok()?;
        let (owner, mode, is_dir) = if link_metadata.file_type().is_symlink() {
            if link_metadata.uid() != 0 && link_metadata.uid() != geteuid().as_raw() {
                return None;
            }
            let target = fs::metadata(&prefix).ok()?;
            (target.uid(), target.mode(), target.is_dir())
        } else {
            (
                link_metadata.uid(),
                link_metadata.mode(),
                link_metadata.is_dir(),
            )
        };
        let root_sticky = owner == 0 && mode & 0o1000 != 0;
        if !is_dir
            || (owner != 0 && owner != geteuid().as_raw())
            || (mode & 0o022 != 0 && !root_sticky)
        {
            return None;
        }
    }
    Some(())
}

/// Derive the deterministic alias path without creating or changing anything.
fn short_socket_alias_path(full: &Path) -> Option<PathBuf> {
    if full.as_os_str().len() < UNIX_SOCKET_PATH_LIMIT {
        return None;
    }
    let digest = Sha256::digest(full.as_os_str().as_encoded_bytes());
    let mut name = String::with_capacity(21);
    name.push_str("jk-");
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _written = write!(name, "{byte:02x}");
    }
    name.push_str(".sock");
    let dir = std::env::temp_dir().join(format!(
        "{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}",
        geteuid().as_raw()
    ));
    let alias = dir.join(name);
    if alias.as_os_str().len() >= UNIX_SOCKET_PATH_LIMIT {
        return None;
    }
    Some(alias)
}

pub(crate) fn default_service_executable() -> Option<PathBuf> {
    std::env::var_os("JACKIN_USAGE_BROKER_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("jackin-usage-broker"))
            })
        })
}
