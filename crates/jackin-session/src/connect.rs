// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{GatewayEntry, GatewayName, ProcessError};
use jackin_openshell::{DiscoveryError, Gateway, GatewayError};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("cannot discover gateway {gateway}")]
    Discovery {
        gateway: GatewayName,
        source: DiscoveryError,
    },
    #[error(transparent)]
    Gateway(#[from] GatewayError),
}

impl ConnectError {
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self,
            ConnectError::Discovery {
                source: DiscoveryError::Process(ProcessError::Cancelled { .. }),
                ..
            } | ConnectError::Gateway(GatewayError::Cancelled { .. })
        )
    }
}

#[async_trait]
pub trait GatewayConnector: Send + Sync + Debug {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, ConnectError>;
}
