// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker startup cleanup guard.

use std::path::PathBuf;

use crate::{BrokerLeaseOwner, cleanup_owned_files, renew_lease};

/// Owns the lease and socket for the broker lifetime. Drop removes the socket
/// while the descriptor-bound lease is still locked, then removes that exact
/// lease inode. The serve loop drops this guard only after its workers and
/// ticker have drained.
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

    pub(crate) fn renew(&mut self) -> bool {
        self.lease.as_mut().is_some_and(renew_lease)
    }
}

impl Drop for BrokerStartupCleanup {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.as_mut() {
            let _ignored = cleanup_owned_files(&self.lease_path, &self.socket_path, lease);
        }
    }
}
