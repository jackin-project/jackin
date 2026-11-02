// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod app;
mod cli;
mod commands;
mod error;
mod output;
mod paths;
mod report;
mod system;
mod terminal;
mod views;

use std::sync::Arc;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use jackin_core::ConfigStore;

pub use app::*;
pub use cli::*;
pub use error::*;
pub use system::*;
pub use terminal::*;
pub use views::*;

use output::Io;

pub async fn run(env: AppEnv, deps: Deps) -> ExitClass {
    let AppEnv {
        args,
        stdout,
        stderr,
        cwd,
        config_path,
        cache_dir,
        ..
    } = env;
    let mut io = Io::new(stdout, stderr);
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => return usage(&mut io, &error),
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
        cwd,
        cache_dir,
    };
    let result = match cli.command {
        Some(command) => commands::dispatch(&mut app, command).await,
        None => app.io.data(&Cli::command().render_help().to_string()),
    };
    match result {
        Ok(()) => ExitClass::Ok,
        Err(error) => {
            app.io.note(&report::report(&error));
            error.class()
        }
    }
}

fn usage(io: &mut Io, error: &clap::Error) -> ExitClass {
    let text = error.render().to_string();
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            if io.data(&text).is_ok() {
                ExitClass::Ok
            } else {
                ExitClass::Failure
            }
        }
        _ => {
            io.note(text.trim_end());
            ExitClass::Usage
        }
    }
}
