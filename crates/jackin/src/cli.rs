// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;
use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use jackin_core::{
    AccountName, BuilderName, GatewayName, OpenShellWorkspace, Platform, RoleName, RoleRev,
    WorkspaceName,
};

use crate::error::CommandError;

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
    #[command(subcommand, about = "Manage the jackin gateway entries")]
    Gateway(GatewayCommand),
    #[command(subcommand, about = "Manage accounts and their bindings")]
    Account(AccountCommand),
    #[command(subcommand, about = "Manage role sources")]
    Role(RoleCommand),
    #[command(subcommand, about = "Manage workspaces")]
    Workspace(WorkspaceCommand),
}

#[derive(Debug, Subcommand)]
pub enum GatewayCommand {
    #[command(about = "Add a gateway entry")]
    Add(GatewayAddArgs),
    #[command(about = "List the gateway entries")]
    Ls(JsonArg),
    #[command(about = "Remove a gateway entry")]
    Rm(GatewayNameArg),
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    #[command(about = "List the accounts")]
    Ls(JsonArg),
    #[command(about = "Make an account the default of its provider")]
    Default(AccountArg),
    #[command(about = "Bind an account to a role or a workspace")]
    Bind(BindArgs),
}

#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    #[command(about = "Add a role source")]
    Add(RoleAddArgs),
    #[command(about = "List the role sources")]
    Ls(JsonArg),
    #[command(about = "Remove a role source")]
    Rm(RoleArg),
    #[command(about = "Validate a role manifest, its Dockerfile, and its policy file")]
    Check(RoleCheckArgs),
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    #[command(about = "Add a workspace")]
    Add(WorkspaceAddArgs),
    #[command(about = "List the workspaces")]
    Ls(JsonArg),
    #[command(about = "Remove a workspace")]
    Rm(WorkspaceArg),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PlatformArg {
    #[value(name = "linux/arm64")]
    LinuxArm64,
    #[value(name = "linux/amd64")]
    LinuxAmd64,
}

impl From<PlatformArg> for Platform {
    fn from(arg: PlatformArg) -> Platform {
        match arg {
            PlatformArg::LinuxArm64 => Platform::LinuxArm64,
            PlatformArg::LinuxAmd64 => Platform::LinuxAmd64,
        }
    }
}

#[derive(Debug, Args)]
pub struct JsonArg {
    #[arg(long, help = "Print JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct GatewayNameArg {
    #[arg(help = "Gateway name from the jackin config")]
    pub name: GatewayName,
}

#[derive(Debug, Args)]
pub struct GatewayAddArgs {
    #[arg(help = "Name of the gateway as registered with openshell gateway add")]
    pub name: GatewayName,
    #[arg(
        long,
        default_value = "default",
        help = "OpenShell workspace that jackin uses"
    )]
    pub workspace: OpenShellWorkspace,
    #[arg(
        long,
        default_value = "default",
        help = "Docker buildx builder with the docker driver"
    )]
    pub builder: BuilderName,
    #[arg(
        long,
        value_enum,
        help = "Image platform; defaults to the host architecture"
    )]
    pub platform: Option<PlatformArg>,
    #[arg(
        long,
        requires = "engine_certs",
        help = "Port of the dedicated Docker engine on host.openshell.internal"
    )]
    pub engine_port: Option<NonZeroU16>,
    #[arg(
        long,
        requires = "engine_port",
        help = "Directory with the engine client ca.pem, cert.pem, and key.pem"
    )]
    pub engine_certs: Option<PathBuf>,
    #[arg(long, help = "Make this the default gateway")]
    pub default: bool,
}

#[derive(Debug, Args)]
pub struct AccountArg {
    #[arg(help = "Account name")]
    pub account: AccountName,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("scope").required(true).multiple(false).args(["role", "workspace"])))]
pub struct BindArgs {
    #[arg(help = "Account name")]
    pub account: AccountName,
    #[arg(long, help = "Role that gets the account as its default")]
    pub role: Option<RoleName>,
    #[arg(long, help = "Workspace that gets the account for its provider")]
    pub workspace: Option<WorkspaceName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindScope {
    Role(RoleName),
    Workspace(WorkspaceName),
}

impl BindArgs {
    pub fn scope(&self) -> Result<BindScope, CommandError> {
        match (&self.role, &self.workspace) {
            (Some(role), None) => Ok(BindScope::Role(role.clone())),
            (None, Some(workspace)) => Ok(BindScope::Workspace(workspace.clone())),
            _ => Err(CommandError::Argument(
                "pass exactly one of --role and --workspace".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Args)]
pub struct RoleArg {
    #[arg(help = "Role name")]
    pub name: RoleName,
}

#[derive(Debug, Args)]
pub struct RoleAddArgs {
    #[arg(help = "Role name")]
    pub name: RoleName,
    #[arg(help = "Role directory, or the URL of a Git repository")]
    pub source: String,
    #[arg(long, help = "Branch, tag, or commit of a Git source")]
    pub rev: Option<RoleRev>,
}

#[derive(Debug, Args)]
pub struct RoleCheckArgs {
    #[arg(help = "Role repository directory")]
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct WorkspaceArg {
    #[arg(help = "Workspace name")]
    pub name: WorkspaceName,
}

#[derive(Debug, Args)]
pub struct WorkspaceAddArgs {
    #[arg(help = "Workspace name")]
    pub name: WorkspaceName,
    #[arg(help = "Project directory")]
    pub path: PathBuf,
    #[arg(long, help = "Enable the Docker capability for this workspace")]
    pub docker: bool,
}
