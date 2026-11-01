// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Clock, FileSystem, HostPath, ProcessRunner};
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
pub struct Session {}
