// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{ConfigError, ConfigStore, EnvSnapshot, HostPath};
use jackin_openshell::GatewayError;
use jackin_session::{HostPorts, LaunchRequest, RoleCatalog, Session};
use thiserror::Error;

#[derive(Debug)]
pub enum GatewayState {
    Connected(Box<Session>),
    Failed(GatewayError),
}

#[derive(Debug)]
pub struct SessionSet {}

#[derive(Debug)]
pub struct ConsoleDeps {
    pub config: ConfigStore,
    pub sessions: SessionSet,
    pub roles: Arc<RoleCatalog>,
    pub host: HostPorts,
    pub env: EnvSnapshot,
    pub cwd: HostPath,
    pub engine_socket: HostPath,
}

#[derive(Debug)]
pub enum ConsoleStart {
    Home,
    Launch(LaunchRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleExit {
    Quit,
}

#[derive(Debug)]
pub struct Console {}

#[derive(Debug, Error)]
pub enum ConsoleError {
    #[error("terminal error: {0}")]
    Terminal(std::io::Error),
    #[error(transparent)]
    Config(#[from] ConfigError),
}
