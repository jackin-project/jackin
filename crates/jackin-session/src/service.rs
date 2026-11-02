// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Clock, FileSystem, GatewayName, HostPath, ProcessRunner};
use jackin_image::ImageEngine;
use jackin_openshell::Gateway;

use crate::plan::LaunchLimits;

#[derive(Clone, Debug)]
pub struct HostPorts {
    pub clock: Arc<dyn Clock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<dyn ProcessRunner>,
}

#[derive(Clone, Debug)]
pub struct SessionDeps {
    pub gateway: Arc<dyn Gateway>,
    pub engine: Arc<dyn ImageEngine>,
    pub host: HostPorts,
    pub cache_dir: HostPath,
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
}
