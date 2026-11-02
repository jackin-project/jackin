// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use jackin_core::GatewayName;

#[derive(Debug, Parser)]
#[command(name = "jackin", about = "Run Codex in OpenShell sandboxes", version)]
pub struct Cli {
    #[arg(long, global = true, help = "Gateway name from the jackin config")]
    pub gateway: Option<GatewayName>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(subcommand, about = "Manage role sources")]
    Role(RoleCommand),
}

#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    #[command(about = "Validate a role manifest, its Dockerfile, and its policy file")]
    Check(RoleCheckArgs),
}

#[derive(Debug, Args)]
pub struct RoleCheckArgs {
    #[arg(help = "Role repository directory")]
    pub path: PathBuf,
}
