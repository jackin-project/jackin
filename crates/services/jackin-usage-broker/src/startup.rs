// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker startup cleanup guard.

use std::path::PathBuf;

use crate::{BrokerLeaseOwner, cleanup_owned_files, renew_lease};
use std::time::Duration;

/// Owns the startup lease and socket until the serve loop takes over. Drop
/// removes the socket while the descriptor-bound lease is still locked, then
/// removes that exact lease inode. A successor cannot claim the lease between
/// those operations and therefore cannot have its socket removed by stale
/// startup cleanup.
pub(crate) struct BrokerStartupCleanup {
    lease_path: PathBuf,
    socket_path: PathBuf,
    lease: Option<BrokerLeaseOwner>,
}

impl BrokerStartupCleanup {
    pub(crate) fn new(lease_path: PathBuf, socket_path: PathBuf, lease: BrokerLeaseOwner) -> Self {
        Self {
            lease_path,
            socket_path,
            lease: Some(lease),
        }
    }

    pub(crate) fn renew(&mut self, lease_duration: Duration) -> bool {
        self.lease
            .as_mut()
            .is_some_and(|lease| renew_lease(lease, lease_duration))
    }
}

impl Drop for BrokerStartupCleanup {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.as_mut() {
            let _ignored = cleanup_owned_files(&self.lease_path, &self.socket_path, lease);
        }
    }
}
