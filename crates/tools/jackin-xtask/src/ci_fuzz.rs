// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Run the bounded fuzz contract for one workspace crate without workflow scripting.

use std::env;
use std::io::{self, Write};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub(crate) struct CiFuzzArgs {
    #[arg(long)]
    package: String,
    #[arg(long, default_value_t = 5)]
    max_total_time: u64,
}

pub(crate) fn run(args: CiFuzzArgs) -> Result<()> {
    let Some(contract) = contract_for(&args.package) else {
        writeln!(
            io::stdout().lock(),
            "crate `{}` has no bounded fuzz contract",
            args.package
        )?;
        return Ok(());
    };
    let cargo_fuzz = env::var_os("CI_CARGO_FUZZ").context("CI_CARGO_FUZZ must be set")?;
    cmd::run(Command::new("cargo").current_dir(contract.directory).args([
        "fetch",
        "--locked",
        "--offline",
    ]))?;
    // cargo-fuzz resolves the parent crate manifest from the invocation
    // root, so run here (repo root) with an explicit --fuzz-dir instead of
    // entering the auxiliary workspace (which has no parent manifest).
    for target in contract.targets {
        cmd::run_streaming(Command::new(&cargo_fuzz).args([
            "fuzz",
            "run",
            "--fuzz-dir",
            contract.directory,
            "--sanitizer",
            "none",
            "--target",
            "x86_64-unknown-linux-gnu",
            target,
            "--",
            &format!("-max_total_time={}", args.max_total_time),
        ]))
        .with_context(|| format!("running fuzz target {target} for {}", args.package))?;
    }
    Ok(())
}

struct FuzzContract {
    directory: &'static str,
    targets: &'static [&'static str],
}

fn contract_for(package: &str) -> Option<FuzzContract> {
    let (directory, targets): (&str, &[&str]) = match package {
        "jackin-config" => (
            "fuzz/jackin-config",
            &["config_migrate", "workspace_migrate"],
        ),
        "jackin-env" => ("fuzz/jackin-env", &["env_resolve"]),
        "jackin-manifest" => (
            "fuzz/jackin-manifest",
            &["manifest_migrate", "manifest_validate"],
        ),
        "jackin-protocol" => ("fuzz/jackin-protocol", &["decode_frames"]),
        _ => return None,
    };
    Some(FuzzContract { directory, targets })
}

pub(crate) fn target_names(package: &str) -> Option<&'static [&'static str]> {
    contract_for(package).map(|contract| contract.targets)
}
