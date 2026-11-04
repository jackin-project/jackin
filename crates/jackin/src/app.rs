// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::io::Write;
use std::sync::Arc;

use jackin_core::{Config, ConfigStore, EnvSnapshot, GatewayName, HostPath};
use jackin_image::ImageEngine;
use jackin_openshell::Gateway;
use jackin_session::{
    AccountDesk, GatewayConnector, Gateways, HostPorts, LaunchLimits, RoleCatalog, Session,
    SessionDeps,
};
use jackin_usage::ProviderEndpoints;
use tokio_util::sync::CancellationToken;

use crate::error::CommandError;
use crate::output::Io;
use crate::terminal::Terminal;

#[derive(Clone, Debug)]
pub struct Deps {
    pub host: HostPorts,
    pub engine: Arc<dyn ImageEngine>,
    pub connect: Arc<dyn GatewayConnector>,
    pub limits: LaunchLimits,
    pub usage_endpoints: ProviderEndpoints,
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
    pub(crate) store: ConfigStore,
    pub(crate) terminal: Option<Box<dyn Terminal>>,
    pub(crate) cwd: HostPath,
    pub(crate) cache_dir: HostPath,
    pub(crate) env: EnvSnapshot,
    pub(crate) cancel: CancellationToken,
    pub(crate) gateway: Option<GatewayName>,
}

impl App {
    pub(crate) fn catalog(&self) -> RoleCatalog {
        RoleCatalog::new(
            Arc::clone(&self.deps.host.fs),
            Arc::clone(&self.deps.host.runner),
            Arc::clone(&self.deps.host.clock),
            self.cache_dir.clone(),
        )
    }
}

impl App {
    pub(crate) fn desk(&self) -> AccountDesk {
        AccountDesk::new(
            self.deps.host.clone(),
            self.cache_dir.clone(),
            self.store.clone(),
        )
    }

    pub(crate) async fn session(&self, config: &Config) -> Result<Session, CommandError> {
        let (name, entry) = config.resolve_gateway(self.gateway.as_ref())?;
        let gateway = self.deps.connect.connect(name, entry, &self.cancel).await?;
        Ok(self.session_of(gateway))
    }

    pub(crate) fn session_of(&self, gateway: Arc<dyn Gateway>) -> Session {
        Session::new(SessionDeps {
            gateway,
            engine: Arc::clone(&self.deps.engine),
            host: self.deps.host.clone(),
            cache_dir: self.cache_dir.clone(),
            config: self.store.clone(),
            limits: self.deps.limits,
        })
    }

    pub(crate) async fn gateways(&self, config: &Config) -> Result<Gateways, CommandError> {
        let home = self.session(config).await?;
        Ok(Gateways::with_peers(home, config, self.deps.connect.as_ref(), &self.cancel).await?)
    }
}
