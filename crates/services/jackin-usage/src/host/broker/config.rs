// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker configuration and socket aliases.

use std::fs::{self};

use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

use std::path::{Path, PathBuf};

use std::time::Duration;

use nix::unistd::geteuid;
use sha2::{Digest as _, Sha256};

use crate::coordinator::UsageCoordinatorConfig;

use super::{
    BROKER_DIR, BROKER_IDLE_EXIT, BROKER_LEASE_DURATION, BROKER_LEASE_RENEWAL, BROKER_RUN_DIR,
    BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX, UNIX_SOCKET_PATH_LIMIT, UsageBrokerClient,
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
        short_socket_alias(&full).unwrap_or(full)
    }

    /// Construct a fail-closed client even when broker startup is unavailable.
    #[must_use]
    pub fn client(&self) -> UsageBrokerClient {
        UsageBrokerClient::at(self.socket_path(), self.build_id.clone())
    }
}

/// Deterministic short alias for a broker socket path that exceeds the
/// platform `sun_path` limit (deep test tempdirs, long `$HOME`).
///
/// Returns `None` when the full path fits or the alias directory cannot be
/// provisioned; callers then use the full path and fail closed exactly as
/// before. Client and server derive the same alias from the same
/// `data_dir`, so no rendezvous state is needed. The alias directory is
/// per-uid, `0700`, and ownership-validated like the run directory; the
/// socket file itself keeps the existing `0600` + ownership checks at bind
/// time. Distinct data directories map to distinct alias names via the
/// 64-bit SHA-256 prefix of the full path.
pub(crate) fn short_socket_alias(full: &Path) -> Option<PathBuf> {
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
    fs::create_dir_all(&dir).ok()?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).ok()?;
    let metadata = fs::symlink_metadata(&dir).ok()?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return None;
    }
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
