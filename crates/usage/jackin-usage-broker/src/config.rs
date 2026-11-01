// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use std::time::Duration;

use jackin_usage_coordinator::UsageCoordinatorConfig;

use crate::{
    BROKER_IDLE_EXIT, BROKER_LEASE_DURATION, BROKER_LEASE_RENEWAL, UsageBrokerClient,
    socket_alias::socket_path_for_data_dir,
};

#[derive(Debug, Clone)]
pub struct UsageBrokerConfig {
    pub data_dir: PathBuf,
    pub build_id: String,
    pub coordinator: UsageCoordinatorConfig,
    pub idle_exit: Duration,
    pub lease_duration: Duration,
    pub lease_renewal: Duration,
    pub service_executable: Option<PathBuf>,

    pub socket_alias_root: Option<PathBuf>,
}

impl UsageBrokerConfig {
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
            socket_alias_root: None,
        }
    }

    pub(crate) fn prepare_socket_path(
        &self,
    ) -> Result<PathBuf, jackin_protocol::usage_broker::UsageCoordinationError> {
        socket_path_for_data_dir(&self.data_dir, self.socket_alias_root.as_deref())
    }

    #[must_use]
    pub fn client(&self) -> UsageBrokerClient {
        let full = self
            .data_dir
            .join(crate::BROKER_DIR)
            .join(crate::BROKER_RUN_DIR)
            .join(crate::BROKER_SOCKET);
        match self.prepare_socket_path() {
            Ok(socket_path) => UsageBrokerClient::at_host(
                socket_path,
                self.data_dir.clone(),
                self.build_id.clone(),
            ),
            Err(error) => UsageBrokerClient::at_host_with_initialization_error(
                full,
                self.data_dir.clone(),
                self.build_id.clone(),
                error,
            ),
        }
    }
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
