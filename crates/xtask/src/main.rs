// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jackin_xtask::{
    Check, CiArgs, CiFailure, E2eArgs, GhCompare, ProductCheck, Repo, baseline_branch, e2e_command,
    production, run_ci,
};

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
        name = "baseline-branch",
        about = "Check that the pinned baseline rev is on feature/openshell-native (needs gh and the network)"
    )]
    BaselineBranch,
    #[command(
        name = "product-check",
        about = "Build the release binary of jackin and prove that it holds no demo (several minutes)"
    )]
    ProductCheck,
    #[command(
        about = "Run the sentinel journey J00 to J17 against a gateway",
        after_help = E2E_HELP
    )]
    E2e(Box<E2eArgs>),
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Ci(args) => ci(args),
        Command::BaselineBranch => branch(),
        Command::ProductCheck => product_check(),
        Command::E2e(args) => e2e(&args).await,
    }
}

async fn e2e(args: &E2eArgs) -> ExitCode {
    ExitCode::from(e2e_command(args, production(args)).await)
}

fn open_repo() -> Result<Repo, ExitCode> {
    let root = std::env::current_dir().map_err(|error| fail_chain(&error))?;
    Repo::open(root).map_err(|error| fail_chain(&error))
}

fn ci(args: CiArgs) -> ExitCode {
    let repo = match open_repo() {
        Ok(repo) => repo,
        Err(code) => return code,
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

fn product_check() -> ExitCode {
    let repo = match open_repo() {
        Ok(repo) => repo,
        Err(code) => return code,
    };
    let mut stderr = io::stderr().lock();
    if writeln!(stderr, "xtask: product-check").is_err() {
        return ExitCode::FAILURE;
    }
    drop(stderr);
    let violations = ProductCheck::production().run(&repo);
    if violations.is_empty() {
        return ExitCode::SUCCESS;
    }
    let mut stderr = io::stderr().lock();
    for violation in &violations {
        if writeln!(stderr, "{violation}").is_err() {
            break;
        }
    }
    drop(stderr);
    fail(format!(
        "product-check reported {} violations",
        violations.len()
    ))
}

fn branch() -> ExitCode {
    let repo = match open_repo() {
        Ok(repo) => repo,
        Err(code) => return code,
    };
    match baseline_branch(&repo, &GhCompare) {
        Ok(pins) => {
            let mut stdout = io::stdout().lock();
            for pin in pins {
                if writeln!(stdout, "{} {}", pin.rev, pin.status).is_err() {
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
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
