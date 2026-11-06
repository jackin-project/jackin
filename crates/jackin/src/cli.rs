// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;
use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use jackin_core::{
    AccountName, BuilderName, EnvName, GatewayName, LaunchId, OpReference, OpenShellWorkspace,
    PlainEnvValue, Platform, Provider, RoleName, RoleRev, SandboxName, WorkspaceName,
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
    #[command(about = "Fetch the sandbox branches into the host repository")]
    Export(SandboxArg),
    #[command(about = "Stop a sandbox")]
    Stop(SandboxArg),
    #[command(about = "Start a stopped sandbox")]
    Start(SandboxArg),
    #[command(about = "Delete a sandbox; refuses unexported work without --force")]
    Purge(PurgeArgs),
    #[command(subcommand, about = "Manage accounts and their bindings")]
    Account(AccountCommand),
    #[command(about = "Check the host tools, the gateway, and the engine socket")]
    Doctor,
    #[command(about = "Build the role image and print its image ID")]
    Build(BuildArgs),
    #[command(about = "Build the image if needed, launch a sandbox, and attach to Codex")]
    Load(Box<LoadArgs>),
    #[command(about = "Reattach to the Codex terminal of a sandbox")]
    Attach(AttachArgs),
    #[command(subcommand, about = "Manage agent instances in a sandbox")]
    Agent(AgentCommand),
    #[command(about = "Merge each agent branch into the lead branch in the sandbox")]
    Integrate(SandboxArg),
    #[command(about = "Open a new shell in a sandbox")]
    Shell(SandboxArg),
    #[command(about = "List the jackin sandboxes")]
    Ls(JsonArg),
    #[command(about = "Show the limit windows of each account")]
    Usage(JsonArg),
    #[command(about = "Show a sandbox, its labels, providers, and policy status")]
    Inspect(InspectArgs),
    #[command(subcommand, about = "Manage role sources")]
    Role(RoleCommand),
    #[command(subcommand, about = "Manage workspaces")]
    Workspace(WorkspaceCommand),
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    #[command(about = "Restart one agent instance in a sandbox")]
    Restart(AgentRestartArgs),
}

#[derive(Debug, Args)]
pub struct AgentRestartArgs {
    #[arg(help = "Sandbox name")]
    pub sandbox: SandboxName,
    #[arg(help = "Agent name")]
    pub agent: String,
}

#[derive(Debug, Subcommand)]
pub enum GatewayCommand {
    #[command(about = "Add a gateway entry")]
    Add(GatewayAddArgs),
    #[command(about = "List the gateway entries")]
    Ls(JsonArg),
    #[command(about = "Remove a gateway entry")]
    Rm(GatewayRmArgs),
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    #[command(about = "Add an account")]
    Add(AccountAddArgs),
    #[command(about = "List the accounts")]
    Ls(JsonArg),
    #[command(about = "Remove an account")]
    Rm(AccountArg),
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
pub struct GatewayRmArgs {
    #[arg(help = "Gateway name from the jackin config")]
    pub name: GatewayName,
    #[arg(
        long,
        help = "Remove the entry without contacting the gateway; its OpenShell providers stay on it"
    )]
    pub offline: bool,
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

#[derive(Debug, Args)]
pub struct SandboxArg {
    #[arg(help = "Sandbox name")]
    pub sandbox: SandboxName,
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(help = "Sandbox name")]
    pub sandbox: SandboxName,
    #[arg(long, help = "Print JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    #[arg(help = "Role name; defaults to the workspace default role")]
    pub role: Option<RoleName>,
    #[arg(long, help = "Build the image again")]
    pub rebuild: bool,
    #[arg(long, help = "Print JSON")]
    pub json: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialArg {
    Env(EnvName),
    OnePassword(OpReference),
    File(PathBuf),
    OmpHost,
}

const MAX_ENV_NAME: usize = 128;

impl CredentialArg {
    pub fn parse(text: &str) -> Result<CredentialArg, CommandError> {
        const FORMS: &str =
            "--credential expects env:NAME, an op:// reference, file:PATH, or omp-host";
        let bad = || CommandError::Argument(FORMS.to_owned());
        if let Some(name) = text.strip_prefix("env:") {
            if name.len() > MAX_ENV_NAME {
                return Err(bad());
            }
            return EnvName::parse(name)
                .map(CredentialArg::Env)
                .map_err(|_| bad());
        }
        if text == "omp-host" {
            return Ok(CredentialArg::OmpHost);
        }
        if text.starts_with("op://") {
            return OpReference::parse(text)
                .map(CredentialArg::OnePassword)
                .map_err(|_| bad());
        }
        match text.strip_prefix("file:") {
            Some(path) if !path.is_empty() => Ok(CredentialArg::File(PathBuf::from(path))),
            _ => Err(bad()),
        }
    }
}

pub fn parse_agent(text: &str) -> Result<jackin_core::Agent, CommandError> {
    jackin_core::Agent::ALL
        .into_iter()
        .find(|agent| agent.as_str() == text)
        .ok_or_else(|| {
            let names: Vec<&str> = jackin_core::Agent::ALL
                .into_iter()
                .map(jackin_core::Agent::as_str)
                .collect();
            CommandError::Argument(format!(
                "the agent is unknown; expected one of {}",
                names.join(", ")
            ))
        })
}

pub fn parse_provider(text: &str) -> Result<Provider, CommandError> {
    Provider::ALL
        .into_iter()
        .find(|provider| provider.as_str() == text)
        .ok_or_else(|| {
            let names: Vec<&str> = Provider::ALL.into_iter().map(Provider::as_str).collect();
            CommandError::Argument(format!(
                "the provider is unknown; expected one of {}",
                names.join(", ")
            ))
        })
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("source")
        .required(true)
        .multiple(false)
        .args(["chatgpt_from", "credential"])
))]
pub struct AccountAddArgs {
    #[arg(help = "Provider of the account")]
    pub provider: String,
    #[arg(long, help = "Account name")]
    pub name: AccountName,
    #[arg(
        long,
        help = "Agent to sync this account for; defaults to the provider native agent, or omp for omp-only providers"
    )]
    pub agent: Option<String>,
    #[arg(
        long = "chatgpt-from",
        value_name = "CODEX_HOME",
        help = "Import a Codex ChatGPT login"
    )]
    pub chatgpt_from: Option<PathBuf>,
    #[arg(
        long,
        value_name = "SOURCE",
        help = "Credential as env:NAME, an op:// reference, file:PATH, or omp-host"
    )]
    pub credential: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvAssignment {
    pub name: EnvName,
    pub value: PlainEnvValue,
}

impl EnvAssignment {
    pub fn parse(text: &str) -> Result<EnvAssignment, CommandError> {
        let bad = || CommandError::Argument("--env expects NAME=VALUE".to_owned());
        let (name, value) = text.split_once('=').ok_or_else(bad)?;
        let name = EnvName::parse(name).map_err(|_| bad())?;
        let value = PlainEnvValue::parse(value).map_err(|source| CommandError::EnvValue {
            name: name.clone(),
            source,
        })?;
        Ok(EnvAssignment { name, value })
    }
}

#[derive(Debug, Args)]
pub struct LoadArgs {
    #[arg(help = "Role name; defaults to the workspace default role")]
    pub role: Option<RoleName>,
    #[arg(default_value = ".", help = "Project directory")]
    pub path: PathBuf,
    #[arg(long, help = "Account to use for this launch")]
    pub account: Option<AccountName>,
    #[arg(long, help = "Agent to run for this launch; repeat for more instances")]
    pub agent: Vec<String>,
    #[arg(long, help = "Provider to use for this launch")]
    pub provider: Option<String>,
    #[arg(
        long = "provider-for",
        value_name = "AGENT=PROVIDER",
        help = "Provider for one instance"
    )]
    pub provider_for: Vec<String>,
    #[arg(
        long = "account-for",
        value_name = "AGENT=ACCOUNT",
        help = "Account for one instance"
    )]
    pub account_for: Vec<String>,
    #[arg(
        long = "model-for",
        value_name = "AGENT=MODEL",
        help = "Model for one instance"
    )]
    pub model_for: Vec<String>,
    #[arg(long, help = "Model to use for this launch")]
    pub model: Option<String>,
    #[arg(long, help = "Task text for the instances")]
    pub task: Option<String>,
    #[arg(long = "task-file", help = "Host file that holds the task text")]
    pub task_file: Option<PathBuf>,
    #[arg(long, help = "GitHub issue URL that holds the task")]
    pub issue: Option<String>,
    #[arg(long, help = "Build the image again")]
    pub rebuild: bool,
    #[arg(long, help = "Continue an earlier launch")]
    pub launch_id: Option<LaunchId>,
    #[arg(
        long = "env",
        value_name = "NAME=VALUE",
        help = "Value for a role env entry"
    )]
    pub env: Vec<String>,
}

#[derive(Debug, Args)]
pub struct AttachArgs {
    #[arg(help = "Sandbox name")]
    pub sandbox: SandboxName,
    #[arg(long, help = "Run preflight and start Codex again after it exited")]
    pub restart: bool,
    #[arg(long, help = "Attach to this agent instance instead of the lead")]
    pub agent: Option<String>,
}

#[derive(Debug, Args)]
pub struct PurgeArgs {
    #[arg(help = "Sandbox name")]
    pub sandbox: SandboxName,
    #[arg(long, help = "Delete the sandbox even when its work is not exported")]
    pub force: bool,
}
