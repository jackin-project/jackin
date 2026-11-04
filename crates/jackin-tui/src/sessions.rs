// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use jackin_core::{Config, GatewayEntry, GatewayName};
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::{
    ConnectError, GatewayAccess, GatewayConnector, Gateways, Session, UnreachableGateway,
};
use tokio_util::sync::CancellationToken;

use crate::fleet::{TimedOut, Waited, within};

#[derive(Debug)]
pub enum GatewayState {
    Connected(Box<Session>),
    Failed(GatewayError),
}

#[async_trait]
pub trait SessionConnector: Send + Sync + fmt::Debug {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Session, GatewayError>;
}

type SessionMaker = dyn Fn(Arc<dyn Gateway>) -> Session + Send + Sync;

pub struct GatewaySessions {
    gateways: Arc<dyn GatewayConnector>,
    make: Box<SessionMaker>,
}

impl GatewaySessions {
    pub fn new(
        gateways: Arc<dyn GatewayConnector>,
        make: impl Fn(Arc<dyn Gateway>) -> Session + Send + Sync + 'static,
    ) -> GatewaySessions {
        GatewaySessions {
            gateways,
            make: Box::new(make),
        }
    }
}

impl fmt::Debug for GatewaySessions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewaySessions")
            .field("gateways", &self.gateways)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl SessionConnector for GatewaySessions {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Session, GatewayError> {
        self.gateways
            .connect(name, entry, cancel)
            .await
            .map(|gateway| (self.make)(gateway))
            .map_err(|error| error.into_failure(name))
    }
}

#[derive(Debug)]
enum Slot {
    Up(Box<Session>),
    Down(Arc<GatewayError>),
}

impl From<GatewayState> for Slot {
    fn from(state: GatewayState) -> Slot {
        match state {
            GatewayState::Connected(session) => Slot::Up(session),
            GatewayState::Failed(error) => Slot::Down(Arc::new(error)),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Reached {
    Up(Box<Session>),
    Down(Arc<GatewayError>),
}

#[derive(Clone, Debug)]
pub(crate) enum Access {
    NotChosen,
    Up(Box<Session>),
    Down {
        name: GatewayName,
        error: Arc<GatewayError>,
    },
}

impl Access {
    pub(crate) fn borrowed(&self) -> GatewayAccess<'_> {
        match self {
            Access::NotChosen => GatewayAccess::NotChosen,
            Access::Up(session) => GatewayAccess::Connected(session.gateway()),
            Access::Down { name, error } => GatewayAccess::Failed { name, error },
        }
    }
}

#[derive(Debug)]
pub(crate) enum Home {
    NotChosen,
    Down(Arc<GatewayError>),
    Ready(Box<Gateways>),
}

#[derive(Debug)]
pub struct SessionSet {
    default: GatewayName,
    slots: Mutex<BTreeMap<GatewayName, Slot>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SessionSet {
    pub fn new(default: GatewayName, entries: BTreeMap<GatewayName, GatewayState>) -> SessionSet {
        SessionSet {
            default,
            slots: Mutex::new(
                entries
                    .into_iter()
                    .map(|(n, s)| (n, Slot::from(s)))
                    .collect(),
            ),
        }
    }

    pub async fn connect(
        config: &Config,
        default: GatewayName,
        connector: &dyn SessionConnector,
        deadline: Duration,
        cancel: &CancellationToken,
    ) -> SessionSet {
        let attempts = config.gateways().map(|(name, entry)| async move {
            let state = match within(deadline, connector.connect(name, entry, cancel)).await {
                Some(Ok(session)) => GatewayState::Connected(Box::new(session)),
                Some(Err(error)) => GatewayState::Failed(error),
                None => GatewayState::Failed(GatewayError::Unreachable {
                    endpoint: name.to_string(),
                    message: TimedOut(Waited(deadline)).to_string(),
                }),
            };
            (name.clone(), state)
        });
        SessionSet::new(default, join_all(attempts).await.into_iter().collect())
    }

    pub fn get(&self, name: &GatewayName) -> Option<Session> {
        match locked(&self.slots).get(name)? {
            Slot::Up(session) => Some(Session::clone(session)),
            Slot::Down(_) => None,
        }
    }

    pub fn default_name(&self) -> &GatewayName {
        &self.default
    }

    pub(crate) fn reached(&self) -> Vec<(GatewayName, Reached)> {
        locked(&self.slots)
            .iter()
            .map(|(name, slot)| {
                let reached = match slot {
                    Slot::Up(session) => Reached::Up(session.clone()),
                    Slot::Down(error) => Reached::Down(Arc::clone(error)),
                };
                (name.clone(), reached)
            })
            .collect()
    }

    pub(crate) async fn home(
        &self,
        config: &Config,
        connector: &dyn SessionConnector,
        cancel: &CancellationToken,
    ) -> Home {
        let (home, known) = {
            let slots = locked(&self.slots);
            let home = match slots.get(&self.default) {
                None => return Home::NotChosen,
                Some(Slot::Down(error)) => return Home::Down(Arc::clone(error)),
                Some(Slot::Up(session)) => Session::clone(session),
            };
            let known: BTreeMap<GatewayName, Reached> = slots
                .iter()
                .map(|(name, slot)| {
                    let reached = match slot {
                        Slot::Up(session) => Reached::Up(session.clone()),
                        Slot::Down(error) => Reached::Down(Arc::clone(error)),
                    };
                    (name.clone(), reached)
                })
                .collect();
            (home, known)
        };
        let unreachable = |name: &GatewayName, error: &GatewayError| {
            UnreachableGateway::new(
                name.clone(),
                ConnectError::Gateway(GatewayError::Unreachable {
                    endpoint: name.to_string(),
                    message: error.to_string(),
                }),
            )
        };
        let mut peers = Vec::new();
        for (name, entry) in config.gateways().filter(|(name, _)| **name != self.default) {
            peers.push(match known.get(name) {
                Some(Reached::Up(session)) => Ok(Session::clone(session)),
                Some(Reached::Down(error)) => Err(unreachable(name, error)),
                None => connector
                    .connect(name, entry, cancel)
                    .await
                    .map_err(|error| unreachable(name, &error)),
            });
        }
        Home::Ready(Box::new(Gateways::assemble(home, peers)))
    }

    pub(crate) fn access(&self, name: &GatewayName) -> Access {
        match locked(&self.slots).get(name) {
            None => Access::NotChosen,
            Some(Slot::Up(session)) => Access::Up(session.clone()),
            Some(Slot::Down(error)) => Access::Down {
                name: name.clone(),
                error: Arc::clone(error),
            },
        }
    }

    pub(crate) async fn reconnect(
        &self,
        connector: &dyn SessionConnector,
        config: &Config,
        cancel: &CancellationToken,
    ) {
        let failed: Vec<GatewayName> = locked(&self.slots)
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::Down(_)))
            .map(|(name, _)| name.clone())
            .collect();
        let attempts = failed.iter().filter_map(|name| {
            let entry = config.gateway(name)?;
            Some(async move { (name, connector.connect(name, entry, cancel).await) })
        });
        let outcomes = join_all(attempts).await;
        let mut slots = locked(&self.slots);
        for (name, outcome) in outcomes {
            let slot = match outcome {
                Ok(session) => Slot::Up(Box::new(session)),
                Err(error) => Slot::Down(Arc::new(error)),
            };
            slots.insert(name.clone(), slot);
        }
    }
}
