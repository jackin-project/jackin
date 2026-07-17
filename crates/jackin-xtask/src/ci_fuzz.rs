//! Run the bounded fuzz contract for one workspace crate without workflow scripting.

use std::env;
use std::process::Command;

use anyhow::{Context, Result, bail};
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
    let contract = contract_for(&args.package)?;
    let cargo_fuzz = env::var_os("CI_CARGO_FUZZ").context("CI_CARGO_FUZZ must be set")?;
    cmd::run(Command::new("cargo").current_dir(contract.directory).args([
        "fetch",
        "--locked",
        "--offline",
    ]))?;
    for target in contract.targets {
        cmd::run_streaming(
            Command::new(&cargo_fuzz)
                .current_dir(contract.directory)
                .args([
                    "fuzz",
                    "run",
                    "--sanitizer",
                    "none",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    target,
                    "--",
                    &format!("-max_total_time={}", args.max_total_time),
                ]),
        )
        .with_context(|| format!("running fuzz target {target} for {}", args.package))?;
    }
    Ok(())
}

struct FuzzContract {
    directory: &'static str,
    targets: &'static [&'static str],
}

fn contract_for(package: &str) -> Result<FuzzContract> {
    let (directory, targets): (&str, &[&str]) = match package {
        "jackin-config" => (
            "crates/jackin-config",
            &["config_migrate", "workspace_migrate"],
        ),
        "jackin-env" => ("crates/jackin-env", &["env_resolve"]),
        "jackin-manifest" => (
            "crates/jackin-manifest",
            &["manifest_migrate", "manifest_validate"],
        ),
        "jackin-protocol" => ("crates/jackin-protocol", &["decode_frames"]),
        "jackin-term" => ("crates/jackin-term", &["damage_grid_process"]),
        _ => bail!("crate `{package}` does not own a bounded CI fuzz contract"),
    };
    Ok(FuzzContract { directory, targets })
}
