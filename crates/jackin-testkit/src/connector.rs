// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jackin_core::{GatewayEntry, GatewayName};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{ConnectError, GatewayConnector};
use tokio_util::sync::CancellationToken;

use crate::sync::locked;

#[derive(Debug)]
struct Scripted {
    gateway: Arc<dyn Gateway>,
    script: VecDeque<Option<GatewayError>>,
    calls: u32,
}

#[derive(Debug, Default)]
pub struct ScriptedConnector {
    gateways: Mutex<BTreeMap<GatewayName, Scripted>>,
}

impl ScriptedConnector {
    pub fn new() -> ScriptedConnector {
        ScriptedConnector::default()
    }

    #[must_use]
    pub fn gateway(
        self,
        gateway: Arc<dyn Gateway>,
        script: Vec<Option<GatewayError>>,
    ) -> ScriptedConnector {
        locked(&self.gateways).insert(
            gateway.name().clone(),
            Scripted {
                gateway,
                script: script.into(),
                calls: 0,
            },
        );
        self
    }

    pub fn calls(&self, name: &GatewayName) -> u32 {
        locked(&self.gateways)
            .get(name)
            .map_or(0, |scripted| scripted.calls)
    }
}

#[async_trait]
impl GatewayConnector for ScriptedConnector {
    async fn connect(
        &self,
        name: &GatewayName,
        _entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, ConnectError> {
        if cancel.is_cancelled() {
            return Err(ConnectError::Gateway(GatewayError::Cancelled {
                op: GatewayOp::Health,
            }));
        }
        let mut gateways = locked(&self.gateways);
        let Some(scripted) = gateways.get_mut(name) else {
            return Err(ConnectError::Gateway(GatewayError::Unreachable {
                endpoint: name.to_string(),
                message: "the scripted connector knows no such gateway".to_owned(),
            }));
        };
        scripted.calls += 1;
        match scripted.script.pop_front().flatten() {
            Some(error) => Err(ConnectError::Gateway(error)),
            None => Ok(Arc::clone(&scripted.gateway)),
        }
    }
}
