// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, ConfigStore, FileSystem, GatewayName, HostPath, ProcessRunner, SandboxName,
};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError, SandboxQuery, SandboxView};

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

impl Session {
    pub(crate) async fn find_managed(
        &self,
        name: &SandboxName,
    ) -> Result<Option<SandboxView>, GatewayError> {
        let found = self
            .deps
            .gateway
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await?;
        Ok(found
            .into_iter()
            .find(|view| view.name == *name && view.labels.is_managed()))
    }
}
