// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use jackin::{
    AppEnv, Deps, ExitClass, GrpcConnector, SystemTerminal, Terminal, launch_limits, locations,
};
use jackin_core::{EnvSnapshot, HostPath, StdFileSystem};
use jackin_image::BuildxEngine;
use jackin_session::HostPorts;
use tokio_util::sync::CancellationToken;

fn fail(message: &str) -> ExitCode {
    drop(writeln!(std::io::stderr(), "error: {message}"));
    ExitCode::from(ExitClass::Failure.code())
}

async fn interrupted(cancel: &CancellationToken) {
    if tokio::signal::ctrl_c().await.is_ok() {
        cancel.cancel();
        if tokio::signal::ctrl_c().await.is_ok() {
            return;
        }
    }
    std::future::pending::<()>().await;
}

fn main() -> ExitCode {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(start()),
        Err(error) => fail(&error.to_string()),
    }
}

async fn start() -> ExitCode {
    let env =
        EnvSnapshot::from_pairs(std::env::vars_os().filter_map(|(name, value)| {
            Some((name.into_string().ok()?, value.into_string().ok()?))
        }));
    let places = match locations(&env) {
        Ok(places) => places,
        Err(error) => return fail(&error.to_string()),
    };
    let cwd = match std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|dir| HostPath::parse(&dir).map_err(|error| error.to_string()))
    {
        Ok(cwd) => cwd,
        Err(error) => return fail(&error),
    };
    let terminal: Option<Box<dyn Terminal>> = match SystemTerminal::open() {
        Ok(Some(terminal)) => Some(Box::new(terminal)),
        Ok(None) => None,
        Err(error) => return fail(&error.to_string()),
    };
    let cancel = CancellationToken::new();
    let host = HostPorts::system(Arc::new(StdFileSystem::default()));
    let build_dir = match HostPath::resolve(std::path::Path::new("build"), &places.cache_dir) {
        Ok(dir) => dir,
        Err(error) => return fail(&error.to_string()),
    };
    let deps = Deps {
        engine: Arc::new(BuildxEngine::new(
            Arc::clone(&host.runner),
            Arc::clone(&host.fs),
            build_dir,
        )),
        connect: Arc::new(GrpcConnector::new(
            Arc::clone(&host.runner),
            Arc::clone(&host.fs),
            places.openshell_config,
        )),
        host,
        limits: launch_limits(),
    };
    let run = jackin::run(
        AppEnv {
            args: std::env::args_os().collect(),
            stdout: Box::new(std::io::stdout()),
            stderr: Box::new(std::io::stderr()),
            terminal,
            cwd,
            config_path: places.config_path,
            cache_dir: places.cache_dir,
            env,
            cancel: cancel.clone(),
        },
        deps,
    );
    let class = tokio::select! {
        class = run => class,
        () = interrupted(&cancel) => ExitClass::Cancelled,
    };
    ExitCode::from(class.code())
}
