// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod app;
mod cli;
mod commands;
mod error;
mod output;
mod parse_error;
mod paths;
mod report;
mod signals;
mod system;
mod term;
mod terminal;
mod views;

use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use jackin_core::ConfigStore;

pub use app::*;
pub use cli::*;
pub use error::*;
pub use signals::*;
pub use system::*;
pub use terminal::*;
pub use views::*;

use output::Io;

pub async fn run(env: AppEnv, deps: Deps) -> ExitClass {
    let AppEnv {
        args,
        stdout,
        stderr,
        terminal,
        cwd,
        config_path,
        cache_dir,
        env,
        cancel,
        abandon,
    } = env;
    let mut io = Io::new(
        stdout,
        stderr,
        output::wants_color(&env, terminal.is_some()),
    );
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => return usage(&mut io, &error, &args),
    };
    let store = ConfigStore::new(
        Arc::clone(&deps.host.fs),
        Arc::clone(&deps.host.clock),
        config_path,
    );
    let mut app = App {
        io,
        deps,
        store,
        terminal,
        cwd,
        cache_dir,
        env,
        cancel,
        gateway: cli.gateway,
        unsettled: Mutex::new(None),
    };
    let finished = {
        let command = async {
            match cli.command {
                Some(command) => commands::dispatch(&mut app, command).await,
                None if app.terminal.is_some() => commands::console::run(&mut app).await,
                None => app.io.data(&Cli::command().render_help().to_string()),
            }
        };
        tokio::pin!(command);
        tokio::select! {
            biased;
            result = &mut command => Some(result),
            () = abandon.cancelled() => None,
        }
    };
    let Some(result) = finished else {
        app.io.note(&report::abandoned(app.take_unsettled()));
        return ExitClass::Cancelled;
    };
    match result {
        Ok(()) => ExitClass::Ok,
        Err(error) => {
            app.io.note(&report::report(&error));
            error.class()
        }
    }
}

fn usage(io: &mut Io, error: &clap::Error, args: &[OsString]) -> ExitClass {
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            if io.data(&error.render().to_string()).is_ok() {
                ExitClass::Ok
            } else {
                ExitClass::Failure
            }
        }
        _ => {
            io.note(&parse_error::render(error, args));
            ExitClass::Usage
        }
    }
}
