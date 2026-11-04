// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{Config, ConfigError, GatewayName, HostPath};
use jackin_tui::{
    ConsoleDeps, ConsoleExit, GatewaySessions, HostClipboard, SessionSet, SystemWall, system_theme,
};
use jackin_usage::{HostUsage, UsageDeps};

use super::usage::STATE_DIR;
use crate::app::App;
use crate::error::CommandError;
use crate::system::{console_limits, engine_socket, home};

fn default_gateway(
    config: &Config,
    requested: Option<&GatewayName>,
) -> Result<Option<GatewayName>, CommandError> {
    match config.resolve_gateway(requested) {
        Ok((name, _)) => Ok(Some(name.clone())),
        Err(error @ ConfigError::UnknownGateway(_)) => Err(error.into()),
        Err(_) => Ok(None),
    }
}

pub(crate) async fn run(app: &mut App) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let default = default_gateway(&config, app.gateway.as_ref())?;
    let limits = console_limits();
    let connect = Arc::new(GatewaySessions::new(
        Arc::clone(&app.deps.connect),
        app.session_maker(),
    ));
    let sessions = SessionSet::connect(
        &config,
        default,
        connect.as_ref(),
        limits.connect_deadline,
        &app.cancel,
    )
    .await;
    let usage = HostUsage::new(UsageDeps {
        clock: Arc::clone(&app.deps.host.clock),
        fs: Arc::clone(&app.deps.host.fs),
        runner: Arc::clone(&app.deps.host.runner),
        env: app.env.clone(),
        endpoints: app.deps.usage_endpoints.clone(),
        state_dir: HostPath::resolve(Path::new(STATE_DIR), &app.cache_dir)?,
    });
    let deps = ConsoleDeps {
        config: app.store.clone(),
        desk: app.desk(),
        usage: Arc::new(usage),
        sessions,
        connect,
        roles: Arc::new(app.catalog()),
        host: app.deps.host.clone(),
        wall: Arc::new(SystemWall::new(Arc::clone(&app.deps.host.clock))),
        env: app.env.clone(),
        cwd: app.cwd.clone(),
        home: home(&app.env)?,
        engine_socket: engine_socket(&app.env)?,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        theme: system_theme(),
        handle: tokio::runtime::Handle::current(),
        limits,
        clipboard: Arc::new(HostClipboard::detect(&app.env)),
    };
    let ConsoleExit::Quit = app.deps.console.run(deps).await?;
    Ok(())
}
