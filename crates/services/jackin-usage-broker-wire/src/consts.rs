// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker directory, lease, and socket constants.

use std::time::Duration;

pub const BROKER_DIR: &str = "usage-broker";
pub const BROKER_RUN_DIR: &str = "run";
pub const BROKER_SOCKET: &str = "usage-broker.sock";
pub const BROKER_LEADER: &str = "leader.pid";
/// Broker catalog attempts per explicit scan: one initial reconcile plus
/// bounded CAS-conflict retries with fresh discovery. Conflicting scans fail
/// closed with `CatalogRevisionConflict` after this bound.
pub const BROKER_ACTIVATION_ATTEMPTS: u32 = 3;
pub const BROKER_LEASE_DURATION: Duration = Duration::from_secs(30);
pub const BROKER_LEASE_RENEWAL: Duration = Duration::from_secs(10);
pub const BROKER_IDLE_EXIT: Duration = Duration::from_mins(10);
pub const CONNECT_RETRY: Duration = Duration::from_secs(10);
pub const CONNECT_RETRY_STEP: Duration = Duration::from_millis(20);
pub const BROKER_CONNECTION_WORKERS: usize = 4;
pub const BROKER_CONNECTION_QUEUE: usize = 128;
pub const PUBLISH_TICK: Duration = Duration::from_millis(200);
/// Maximum `sun_path` bytes including the trailing NUL. `bind`/`connect`
/// fail beyond this, so over-long broker socket paths fall back to a
/// deterministic short alias (macOS allows 104, Linux 108).
#[cfg(target_os = "macos")]
pub const UNIX_SOCKET_PATH_LIMIT: usize = 104;
/// Maximum `sun_path` bytes including the trailing NUL. `bind`/`connect`
/// fail beyond this, so over-long broker socket paths fall back to a
/// deterministic short alias (macOS allows 104, Linux 108).
#[cfg(not(target_os = "macos"))]
pub const UNIX_SOCKET_PATH_LIMIT: usize = 108;
/// Alias directory prefix under the system temp dir, suffixed with the
/// effective uid so alias sockets stay per-user.
pub const BROKER_SOCKET_ALIAS_DIR_PREFIX: &str = "jk-ub-";
