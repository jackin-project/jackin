// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Config, GatewayName};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::connect::{ConnectError, GatewayConnector};
use crate::service::{Session, SessionDeps};

#[derive(Clone, Debug, Error)]
#[error("cannot reach gateway {gateway}")]
pub struct UnreachableGateway {
    gateway: GatewayName,
    source: Arc<ConnectError>,
}

impl UnreachableGateway {
    pub fn gateway(&self) -> &GatewayName {
        &self.gateway
    }
}

#[derive(Clone, Debug)]
enum Peer {
    Up(Session),
    Down(UnreachableGateway),
}

#[derive(Clone, Debug)]
pub struct Gateways {
    home: Session,
    peers: Vec<Peer>,
}

impl Gateways {
    pub fn single(home: Session) -> Gateways {
        Gateways {
            home,
            peers: Vec::new(),
        }
    }

    pub async fn with_peers(
        home: Session,
        config: &Config,
        connector: &dyn GatewayConnector,
        cancel: &CancellationToken,
    ) -> Result<Gateways, ConnectError> {
        let mut peers = Vec::new();
        for (name, entry) in config.gateways() {
            if name == home.gateway_name() {
                continue;
            }
            match connector.connect(name, entry, cancel).await {
                Ok(gateway) => peers.push(Peer::Up(Session::new(SessionDeps {
                    gateway,
                    ..home.deps.clone()
                }))),
                Err(error) if error.is_cancelled() => return Err(error),
                Err(error) => peers.push(Peer::Down(UnreachableGateway {
                    gateway: name.clone(),
                    source: Arc::new(error),
                })),
            }
        }
        Ok(Gateways { home, peers })
    }

    pub fn home(&self) -> &Session {
        &self.home
    }

    pub fn names(&self) -> Vec<&GatewayName> {
        let peers = self.peers.iter().map(|peer| match peer {
            Peer::Up(session) => session.gateway_name(),
            Peer::Down(down) => down.gateway(),
        });
        std::iter::once(self.home.gateway_name())
            .chain(peers)
            .collect()
    }
}
