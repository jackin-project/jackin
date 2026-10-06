// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub(crate) struct CiCargoAuditArgs {}

pub(crate) fn run(_: CiCargoAuditArgs) -> Result<()> {
    let database = advisory_database()?;
    let arguments = audit_arguments(database.join(".git").is_dir());
    cmd::run(cmd::command("cargo").args(arguments))
}

fn advisory_database() -> Result<PathBuf> {
    if let Some(cargo_home) = env::var_os("CARGO_HOME") {
        return Ok(PathBuf::from(cargo_home).join("advisory-db"));
    }
    let home = env::var_os("HOME").context("HOME or CARGO_HOME must be set")?;
    Ok(PathBuf::from(home).join(".cargo/advisory-db"))
}

fn audit_arguments(restored_database: bool) -> Vec<&'static str> {
    let mut arguments = vec!["audit", "--no-yanked"];
    if restored_database {
        arguments.extend(["--no-fetch", "--stale"]);
    }
    arguments
}
