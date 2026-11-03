// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jackin_xtask::{CiArgs, CiFailure, E2eArgs, Repo, production, run_ci, run_e2e};

const E2E_HELP: &str = "J15 and J16 need the operator's D24 test engine: a rootless dockerd at \
host.openshell.internal:2376 (localhost:2376 on the Mac) with client certificates in \
~/.local/share/jackin-e2e-engine/client/. Start and stop it yourself; the runner never does. \
Pass --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client.\n\n\
Exit codes: 0 every step PASS, 1 a FAIL (CLEAN FAIL included), 2 the run could not start, \
3 no FAIL and a SKIP, 130 interrupted.";

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
    #[command(
        about = "Run the sentinel journey J00 to J17 against a gateway",
        after_help = E2E_HELP
    )]
    E2e(E2eArgs),
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Ci(args) => ci(args),
        Command::E2e(args) => e2e(&args).await,
    }
}

async fn e2e(args: &E2eArgs) -> ExitCode {
    match production(args).await {
        Ok(boundaries) => ExitCode::from(run_e2e(args, boundaries).await.code()),
        Err(error) => {
            fail(&error);
            ExitCode::from(2)
        }
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
