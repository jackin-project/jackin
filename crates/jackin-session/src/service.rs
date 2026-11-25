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
    pub(crate) fn managed(self) -> Option<SandboxView> {
        match self {
            Located::Found(view) => Some(*view),
            Located::Unmanaged | Located::Absent => None,
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

    pub(crate) async fn find_managed_until(
        &self,
        name: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<Option<SandboxView>, GatewayError> {
        Ok(self.locate(name, cancel).await?.managed())
    }

    async fn listed(&self, name: &SandboxName) -> Result<Located, GatewayError> {
        let found = self
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await?;
        Ok(found
            .into_iter()
            .find(|view| view.name == *name)
            .map_or(Located::Absent, |view| {
                if view.labels.is_managed() {
                    Located::Found(Box::new(view))
                } else {
                    Located::Unmanaged
                }
            }))
    }
}
