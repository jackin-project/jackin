// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub const BROKER_DIR: &str = "usage-broker";
pub const BROKER_RUN_DIR: &str = "run";
pub const BROKER_SOCKET: &str = "usage-broker.sock";
pub const BROKER_LEADER: &str = "leader.pid";

pub const BROKER_ACTIVATION_ATTEMPTS: u32 = 3;
pub const BROKER_LEASE_DURATION: Duration = Duration::from_secs(30);
pub const BROKER_LEASE_RENEWAL: Duration = Duration::from_secs(10);
pub const BROKER_IDLE_EXIT: Duration = Duration::from_mins(10);
pub const CONNECT_RETRY: Duration = Duration::from_secs(10);
pub const CONNECT_RETRY_STEP: Duration = Duration::from_millis(20);
pub const BROKER_CONNECTION_WORKERS: usize = 4;
pub const BROKER_CONNECTION_QUEUE: usize = 128;
pub const PUBLISH_TICK: Duration = Duration::from_millis(200);

#[cfg(target_os = "macos")]
pub const UNIX_SOCKET_PATH_LIMIT: usize = 104;

#[cfg(not(target_os = "macos"))]
pub const UNIX_SOCKET_PATH_LIMIT: usize = 108;

pub const BROKER_SOCKET_ALIAS_DIR_PREFIX: &str = "jk-ub-";
