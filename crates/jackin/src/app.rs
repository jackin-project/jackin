// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::io::Write;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{EnvSnapshot, GatewayEntry, GatewayName, HostPath};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::{HostPorts, LaunchLimits, RoleCatalog};
use tokio_util::sync::CancellationToken;

use crate::output::Io;
use crate::terminal::Terminal;

#[derive(Clone, Debug)]
pub struct Deps {
    pub host: HostPorts,
    pub engine: Arc<dyn ImageEngine>,
    pub connect: Arc<dyn GatewayConnector>,
    pub limits: LaunchLimits,
}

#[async_trait]
pub trait GatewayConnector: Send + Sync + Debug {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
    ) -> Result<Arc<dyn Gateway>, GatewayError>;
}

pub struct AppEnv {
    pub args: Vec<OsString>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    pub terminal: Option<Box<dyn Terminal>>,
    pub cwd: HostPath,
    pub config_path: HostPath,
    pub cache_dir: HostPath,
    pub env: EnvSnapshot,
    pub cancel: CancellationToken,
}

impl Debug for AppEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppEnv")
            .field("args", &self.args)
            .field("terminal", &self.terminal)
            .field("cwd", &self.cwd)
            .field("config_path", &self.config_path)
            .field("cache_dir", &self.cache_dir)
            .field("env", &self.env)
            .field("cancel", &self.cancel)
            .finish_non_exhaustive()
    }
}

pub(crate) struct App {
    pub(crate) io: Io,
    pub(crate) deps: Deps,
    pub(crate) cwd: HostPath,
    pub(crate) cache_dir: HostPath,
}

impl App {
    pub(crate) fn catalog(&self) -> RoleCatalog {
        RoleCatalog::new(
            Arc::clone(&self.deps.host.fs),
            Arc::clone(&self.deps.host.runner),
            self.cache_dir.clone(),
        )
    }
}
