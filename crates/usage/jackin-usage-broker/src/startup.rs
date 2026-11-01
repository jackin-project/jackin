// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use std::fs;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};

use crate::{BrokerLeaseOwner, BrokerSocketIdentity, cleanup_owned_files, renew_lease};

pub(crate) struct BrokerStartupCleanup {
    lease_path: PathBuf,
    socket_path: PathBuf,
    lease: Option<BrokerLeaseOwner>,
    socket_identity: Option<BrokerSocketIdentity>,
}

impl BrokerStartupCleanup {
    pub(crate) fn new(lease_path: PathBuf, socket_path: PathBuf, lease: BrokerLeaseOwner) -> Self {
        Self {
            lease_path,
            socket_path,
            lease: Some(lease),
            socket_identity: None,
        }
    }

    pub(crate) fn reclaimed_stale_lease(&self) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.stale_lease_reclaimed)
    }

    pub(crate) fn record_socket_identity(&mut self) -> Result<(), ()> {
        let metadata = fs::symlink_metadata(&self.socket_path).map_err(|_| ())?;
        if !metadata.file_type().is_socket() {
            return Err(());
        }
        self.socket_identity = Some(BrokerSocketIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        });
        Ok(())
    }

    pub(crate) fn renew(&mut self) -> bool {
        self.lease.as_mut().is_some_and(renew_lease)
    }
}

impl Drop for BrokerStartupCleanup {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.as_mut() {
            let _ignored = cleanup_owned_files(
                &self.lease_path,
                &self.socket_path,
                self.socket_identity,
                lease,
            );
        }
    }
}
