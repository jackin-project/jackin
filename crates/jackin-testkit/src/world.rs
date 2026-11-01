// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{ConfigStore, FileSystem};
use jackin_session::{RoleCatalog, Session};

use crate::engine::FakeEngine;
use crate::gateway::FakeGateway;
use crate::host::{FakeClock, TestRunner};

#[derive(Debug)]
pub struct World {
    pub gateway: Arc<FakeGateway>,
    pub engine: Arc<FakeEngine>,
    pub clock: Arc<FakeClock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<TestRunner>,
    pub session: Session,
    pub roles: Arc<RoleCatalog>,
    pub config: ConfigStore,
}

#[derive(Debug)]
pub struct WorldBuilder {}
