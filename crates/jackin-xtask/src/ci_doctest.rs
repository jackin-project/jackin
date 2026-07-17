use std::io::{self, Write};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub(crate) struct CiDoctestArgs {
    #[arg(long)]
    package: String,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    doctest: bool,
}

pub(crate) fn run(args: CiDoctestArgs) -> Result<()> {
    let mut metadata = Command::new("cargo");
    metadata.args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--locked",
        "--offline",
    ]);
    let metadata: Metadata = serde_json::from_slice(
        &cmd::output(&mut metadata).context("reading Cargo metadata for doctest ownership")?,
    )
    .context("parsing Cargo metadata for doctest ownership")?;
    let Some(package) = metadata
        .packages
        .iter()
        .find(|package| package.name == args.package)
    else {
        bail!("workspace package not found: {}", args.package);
    };
    if !package.targets.iter().any(|target| target.doctest) {
        writeln!(
            io::stdout().lock(),
            "skip: {} has no doctestable library target",
            args.package
        )?;
        return Ok(());
    }

    let mut command = Command::new("cargo");
    command.args([
        "test",
        "--doc",
        "-p",
        &args.package,
        "--locked",
        "--offline",
    ]);
    cmd::run_streaming(&mut command)
}
