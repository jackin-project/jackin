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
use jackin_session::HostPorts;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct Deps {
    pub host: HostPorts,
    pub engine: Arc<dyn ImageEngine>,
    pub connect: Arc<dyn GatewayConnector>,
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
    pub stdin_is_tty: bool,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    pub config_path: HostPath,
    pub env: EnvSnapshot,
    pub cancel: CancellationToken,
}

impl Debug for AppEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppEnv")
            .field("args", &self.args)
            .field("stdin_is_tty", &self.stdin_is_tty)
            .field("config_path", &self.config_path)
            .field("env", &self.env)
            .field("cancel", &self.cancel)
            .finish_non_exhaustive()
    }
}
