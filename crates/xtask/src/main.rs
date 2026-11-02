// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jackin_xtask::{CiArgs, CiFailure, Repo, run_ci};

#[derive(Parser)]
#[command(name = "xtask", about = "Repository gates for jackin")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Run the repository gates in order and stop at the first failure")]
    Ci(CiArgs),
    #[command(about = "Run the live journey against a gateway (not built yet)")]
    E2e {
        #[arg(hide = true, num_args = 0.., trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Ci(args) => ci(args),
        Command::E2e { .. } => fail("e2e is not built yet; PLAN P8.1 builds it"),
    }
}

fn ci(args: CiArgs) -> ExitCode {
    let root = match std::env::current_dir() {
        Ok(root) => root,
        Err(error) => return fail_chain(&error),
    };
    let repo = match Repo::open(root) {
        Ok(repo) => repo,
        Err(error) => return fail_chain(&error),
    };
    match run_ci(&args.steps(), &repo) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CiFailure::Violations { step, violations }) => {
            let mut stderr = io::stderr().lock();
            for violation in &violations {
                if writeln!(stderr, "{violation}").is_err() {
                    break;
                }
            }
            drop(stderr);
            fail(format!("{step} reported {} violations", violations.len()))
        }
        Err(error) => fail_chain(&error),
    }
}

fn fail_chain(error: &dyn Error) -> ExitCode {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        message.push_str(": ");
        message.push_str(&next.to_string());
        cause = next.source();
    }
    fail(message)
}

fn fail(message: impl Display) -> ExitCode {
    match writeln!(io::stderr().lock(), "xtask: {message}") {
        Ok(()) | Err(_) => ExitCode::FAILURE,
    }
}
