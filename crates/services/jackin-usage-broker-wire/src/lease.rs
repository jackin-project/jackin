// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker lease types and ownership.

use std::fs::File;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jackin_protocol::usage_broker::USAGE_BROKER_PROTOCOL_VERSION;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrokerLease {
    pub instance_id: String,
    pub process_id: u32,
    pub protocol_version: String,
    pub build_id: String,
    pub renewed_at_epoch: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ServePolicy {
    pub idle_exit: Duration,
    pub lease_duration: Duration,
    pub lease_renewal: Duration,
}

impl BrokerLease {
    pub fn new(build_id: &str) -> Self {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let instance_id = jackin_core::account_key_hash(
            "usage-broker-instance-v1",
            &format!("{}:{now_nanos}", std::process::id()),
        );
        Self {
            instance_id,
            process_id: std::process::id(),
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: build_id.to_owned(),
            renewed_at_epoch: chrono::Utc::now().timestamp(),
        }
    }
}

/// Descriptor-bound broker authority.
///
/// The lease file is never replaced while an owner is alive. The owner keeps
/// this descriptor locked for its lifetime, and renew/cleanup verify the
/// instance before updating or removing only the inode it opened. A stale
/// process holding an old descriptor therefore cannot renew or unlink a
/// replacement lease at the same path.
pub struct BrokerLeaseOwner {
    pub lease: BrokerLease,
    pub file: File,
}

impl std::fmt::Debug for BrokerLeaseOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerLeaseOwner")
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}
