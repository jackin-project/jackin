// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use jackin_core::{Config, ConfigError, GatewayEntry, GatewayName};
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

async fn dial(
    name: &GatewayName,
    entry: &GatewayEntry,
    connector: &dyn SessionConnector,
    deadline: Duration,
    cancel: &CancellationToken,
) -> Dialed {
    let state = match within(deadline, connector.connect(name, entry, cancel)).await {
        Some(Ok(session)) => GatewayState::Connected(Box::new(session)),
        Some(Err(error)) => GatewayState::Failed(error),
        None => GatewayState::Failed(GatewayError::Unreachable {
            endpoint: name.to_string(),
            message: TimedOut(Waited(deadline)).to_string(),
        }),
    };
    (name.clone(), entry.clone(), state)
}

type Dialed = (GatewayName, GatewayEntry, GatewayState);

#[derive(Clone, Debug)]
pub enum DefaultGateway {
    Pinned(GatewayName),
    Followed(Option<GatewayName>),
}

impl DefaultGateway {
    fn choose(
        config: &Config,
        requested: Option<GatewayName>,
    ) -> Result<DefaultGateway, ConfigError> {
        match requested {
            Some(name) => {
                config.resolve_gateway(Some(&name))?;
                Ok(DefaultGateway::Pinned(name))
            }
            None => Ok(DefaultGateway::follow(config)),
        }
    }

    fn follow(config: &Config) -> DefaultGateway {
        DefaultGateway::Followed(
            config
                .resolve_gateway(None)
                .ok()
                .map(|(name, _)| name.clone()),
        )
    }

    fn settle(&self, config: &Config) -> DefaultGateway {
        match self {
            DefaultGateway::Pinned(name) if config.gateway(name).is_some() => self.clone(),
            DefaultGateway::Pinned(_) | DefaultGateway::Followed(_) => {
                DefaultGateway::follow(config)
            }
        }
    }

    fn name(&self) -> Option<&GatewayName> {
        match self {
            DefaultGateway::Pinned(name) => Some(name),
            DefaultGateway::Followed(name) => name.as_ref(),
        }
    }
}

#[derive(Debug)]
struct State {
    default: DefaultGateway,
    slots: BTreeMap<GatewayName, Slot>,
    built: BTreeMap<GatewayName, GatewayEntry>,
}

impl State {
    fn access(&self, name: &GatewayName) -> Access {
        match self.slots.get(name) {
            None => Access::NotChosen,
            Some(Slot::Up(session)) => Access::Up(session.clone()),
            Some(Slot::Down(error)) => Access::Down {
                name: name.clone(),
                error: Arc::clone(error),
            },
        }
    }
}

#[derive(Debug)]
pub struct SessionSet {
    state: Mutex<State>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SessionSet {
    pub fn new(
        config: &Config,
        default: DefaultGateway,
        entries: BTreeMap<GatewayName, GatewayState>,
    ) -> SessionSet {
        let built = entries
            .keys()
            .filter_map(|name| Some((name.clone(), config.gateway(name)?.clone())))
            .collect();
        SessionSet {
            state: Mutex::new(State {
                default,
                slots: entries
                    .into_iter()
                    .map(|(n, s)| (n, Slot::from(s)))
                    .collect(),
                built,
            }),
        }
    }

    pub async fn connect(
        config: &Config,
        requested: Option<GatewayName>,
        connector: &dyn SessionConnector,
        deadline: Duration,
        cancel: &CancellationToken,
    ) -> Result<SessionSet, ConfigError> {
        let default = DefaultGateway::choose(config, requested)?;
        let attempts = config
            .gateways()
            .map(|(name, entry)| dial(name, entry, connector, deadline, cancel));
        let dialed = join_all(attempts).await;
        let built = dialed
            .iter()
            .map(|(name, entry, _)| (name.clone(), entry.clone()))
            .collect();
        let slots = dialed
            .into_iter()
            .map(|(name, _, state)| (name, Slot::from(state)))
            .collect();
        Ok(SessionSet {
            state: Mutex::new(State {
                default,
                slots,
                built,
            }),
        })
    }

    pub(crate) async fn reconcile(
        &self,
        config: &Config,
        connector: &dyn SessionConnector,
        deadline: Duration,
        cancel: &CancellationToken,
    ) {
        let missing: Vec<(&GatewayName, &GatewayEntry)> = {
            let mut guard = locked(&self.state);
            let State { slots, built, .. } = &mut *guard;
            slots.retain(|name, _| {
                config.gateway(name).is_some() && built.get(name) == config.gateway(name)
            });
            built.retain(|name, _| slots.contains_key(name));
            config
                .gateways()
                .filter(|(name, _)| !slots.contains_key(*name))
                .collect()
        };
        let dialed = join_all(
            missing
                .into_iter()
                .map(|(name, entry)| dial(name, entry, connector, deadline, cancel)),
        )
        .await;
        let mut state = locked(&self.state);
        for (name, entry, gateway) in dialed {
            if let std::collections::btree_map::Entry::Vacant(vacant) =
                state.slots.entry(name.clone())
            {
                vacant.insert(Slot::from(gateway));
                state.built.insert(name, entry);
            }
        }
        state.default = state.default.settle(config);
    }

    pub fn get(&self, name: &GatewayName) -> Option<Session> {
        match locked(&self.state).slots.get(name)? {
            Slot::Up(session) => Some(Session::clone(session)),
            Slot::Down(_) => None,
        }
    }

    pub fn default_name(&self) -> Option<GatewayName> {
        locked(&self.state).default.name().cloned()
    }

    pub(crate) fn default_target(&self) -> Option<(GatewayName, Access)> {
        let state = locked(&self.state);
        let name = state.default.name()?;
        Some((name.clone(), state.access(name)))
    }

    pub(crate) fn reached(&self) -> Vec<(GatewayName, Reached)> {
        locked(&self.state)
            .slots
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
        let (default, home, known) = {
            let state = locked(&self.state);
            let Some(default) = state.default.name().cloned() else {
                return Home::NotChosen;
            };
            let home = match state.slots.get(&default) {
                None => return Home::NotChosen,
                Some(Slot::Down(error)) => return Home::Down(Arc::clone(error)),
                Some(Slot::Up(session)) => Session::clone(session),
            };
            let known: BTreeMap<GatewayName, Reached> = state
                .slots
                .iter()
                .map(|(name, slot)| {
                    let reached = match slot {
                        Slot::Up(session) => Reached::Up(session.clone()),
                        Slot::Down(error) => Reached::Down(Arc::clone(error)),
                    };
                    (name.clone(), reached)
                })
                .collect();
            (default, home, known)
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
        for (name, entry) in config.gateways().filter(|(name, _)| **name != default) {
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
        locked(&self.state).access(name)
    }

    pub(crate) async fn reconnect(
        &self,
        connector: &dyn SessionConnector,
        config: &Config,
        cancel: &CancellationToken,
    ) {
        let failed: Vec<GatewayName> = locked(&self.state)
            .slots
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::Down(_)))
            .map(|(name, _)| name.clone())
            .collect();
        let attempts = failed.iter().filter_map(|name| {
            let entry = config.gateway(name)?;
            Some(async move { (name, entry, connector.connect(name, entry, cancel).await) })
        });
        let outcomes = join_all(attempts).await;
        let mut state = locked(&self.state);
        for (name, entry, outcome) in outcomes {
            let slot = match outcome {
                Ok(session) => Slot::Up(Box::new(session)),
                Err(error) => Slot::Down(Arc::new(error)),
            };
            state.slots.insert(name.clone(), slot);
            state.built.insert(name.clone(), entry.clone());
        }
    }
}
