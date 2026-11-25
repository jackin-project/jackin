// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, ConfigStore, FileSystem, GatewayName, HostPath, ProcessRunner, SandboxName,
};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError, GatewayOp, SandboxQuery, SandboxView};
use tokio_util::sync::CancellationToken;

use crate::plan::LaunchLimits;

pub(crate) const POLL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub struct HostPorts {
    pub clock: Arc<dyn Clock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<dyn ProcessRunner>,
    pub home: HostPath,
}

#[derive(Clone, Debug)]
pub struct SessionDeps {
    pub gateway: Arc<dyn Gateway>,
    pub engine: Arc<dyn ImageEngine>,
    pub host: HostPorts,
    pub cache_dir: HostPath,
    pub config: ConfigStore,
    pub limits: LaunchLimits,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub(crate) deps: SessionDeps,
}

impl Session {
    pub fn new(deps: SessionDeps) -> Session {
        Session { deps }
    }

    pub fn gateway_name(&self) -> &GatewayName {
        self.deps.gateway.name()
    }

    pub fn gateway(&self) -> &dyn Gateway {
        self.deps.gateway.as_ref()
    }

    pub fn engine(&self) -> &dyn ImageEngine {
        self.deps.engine.as_ref()
    }
}

pub(crate) async fn read_until_cancelled<T, E>(
    cancel: &CancellationToken,
    op: GatewayOp,
    call: impl Future<Output = Result<T, E>>,
) -> Result<T, E>
where
    E: From<GatewayError>,
{
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(GatewayError::Cancelled { op, group: None }.into()),
        result = call => result,
    }
}

pub(crate) enum Located {
    Found(Box<SandboxView>),
    Unmanaged,
    Absent,
}

impl Located {
    pub(crate) fn require<E>(
        self,
        absent: impl FnOnce() -> E,
        unmanaged: impl FnOnce() -> E,
    ) -> Result<SandboxView, E> {
        match self {
            Located::Found(view) => Ok(*view),
            Located::Absent => Err(absent()),
            Located::Unmanaged => Err(unmanaged()),
        }
    }
}

impl Session {
    pub(crate) async fn locate(
        &self,
        name: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<Located, GatewayError> {
        read_until_cancelled(cancel, GatewayOp::Find, self.listed(name)).await
    }

    async fn listed(&self, name: &SandboxName) -> Result<Located, GatewayError> {
        let found = self
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await?;
        let mut named = found.into_iter().filter(|view| view.name == *name);
        let first = named.next();
        if named.next().is_some() {
            return Err(GatewayError::Protocol {
                message: format!("the gateway listed several sandboxes named {name}"),
            });
        }
        Ok(match first {
            None => Located::Absent,
            Some(view) if view.labels.is_managed() => Located::Found(Box::new(view)),
            Some(_) => Located::Unmanaged,
        })
    }
}
