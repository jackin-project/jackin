// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exhaustive typed mapping from the live clap tree to governed telemetry names.

use jackin_telemetry::schema::enums::CliCommandName;

use super::{
    AuthCommand, CoauthorTrailerCommand, Command, ConfigCommand, DcoCommand, DiagnosticsCommand,
    EnvCommand, GitCommand, MountCommand, PruneCommand, TrustCommand, WorkspaceClaudeTokenCommand,
    WorkspaceCommand, WorkspaceEnvCommand,
};

#[cfg(unix)]
use super::DaemonCommand;
use super::role::RoleCommand;
use super::usage::UsageScope;

#[must_use]
pub const fn command_name(command: &Command) -> CliCommandName {
    match command {
        Command::Load(_) => CliCommandName::Load,
        Command::Hardline(_) => CliCommandName::Hardline,
        Command::Eject(_) => CliCommandName::Eject,
        Command::Exile => CliCommandName::Exile,
        Command::Purge(_) => CliCommandName::Purge,
        Command::Prewarm(_) => CliCommandName::Prewarm,
        Command::Prune(command) => match command {
            PruneCommand::Roles => CliCommandName::PruneRoles,
            PruneCommand::Cache => CliCommandName::PruneCache,
            PruneCommand::Images => CliCommandName::PruneImages,
            PruneCommand::Instances(_) => CliCommandName::PruneInstances,
            PruneCommand::System(_) => CliCommandName::PruneSystem,
        },
        Command::Console(_) => CliCommandName::Console,
        Command::Role(command) => role_command_name(command),
        Command::Workspace(command) => workspace_command_name(command),
        Command::Config(command) => config_command_name(command),
        #[cfg(unix)]
        Command::Daemon(command) => match command {
            DaemonCommand::Serve => CliCommandName::DaemonServe,
            DaemonCommand::Install => CliCommandName::DaemonInstall,
            DaemonCommand::Uninstall => CliCommandName::DaemonUninstall,
            DaemonCommand::Start => CliCommandName::DaemonStart,
            DaemonCommand::Stop => CliCommandName::DaemonStop,
            DaemonCommand::Restart => CliCommandName::DaemonRestart,
            DaemonCommand::Status => CliCommandName::DaemonStatus,
        },
        Command::Doctor(_) => CliCommandName::Doctor,
        Command::Diagnostics(command) => match command {
            DiagnosticsCommand::Validate => CliCommandName::DiagnosticsValidate,
        },
        Command::Status(_) => CliCommandName::Status,
        Command::Usage(args) => match args.scope {
            UsageScope::Accounts(_) => CliCommandName::UsageAccounts,
            UsageScope::Verify => CliCommandName::UsageVerify,
            UsageScope::Snapshot(_) => CliCommandName::UsageSnapshot,
        },
        Command::Help { .. } => CliCommandName::Help,
    }
}

#[must_use]
pub const fn role_command_name(command: &RoleCommand) -> CliCommandName {
    match command {
        RoleCommand::Validate(_) => CliCommandName::RoleValidate,
        RoleCommand::Migrate(_) => CliCommandName::RoleMigrate,
        RoleCommand::Create(_) => CliCommandName::RoleCreate,
        RoleCommand::ConstructVersion(_) => CliCommandName::RoleConstructVersion,
        RoleCommand::PublishedImage(_) => CliCommandName::RolePublishedImage,
        RoleCommand::PublishedImageRepository(_) => CliCommandName::RolePublishedImageRepository,
        RoleCommand::PublishLabels(_) => CliCommandName::RolePublishLabels,
    }
}

const fn workspace_command_name(command: &WorkspaceCommand) -> CliCommandName {
    match command {
        WorkspaceCommand::Create { .. } => CliCommandName::WorkspaceCreate,
        WorkspaceCommand::List(_) => CliCommandName::WorkspaceList,
        WorkspaceCommand::Show(_) => CliCommandName::WorkspaceShow,
        WorkspaceCommand::Edit { .. } => CliCommandName::WorkspaceEdit,
        WorkspaceCommand::Prune { .. } => CliCommandName::WorkspacePrune,
        WorkspaceCommand::Remove { .. } => CliCommandName::WorkspaceRemove,
        WorkspaceCommand::Env(command) => match command {
            WorkspaceEnvCommand::Set { .. } => CliCommandName::WorkspaceEnvSet,
            WorkspaceEnvCommand::Unset { .. } => CliCommandName::WorkspaceEnvUnset,
            WorkspaceEnvCommand::List { .. } => CliCommandName::WorkspaceEnvList,
        },
        WorkspaceCommand::ClaudeToken(command) => match command {
            WorkspaceClaudeTokenCommand::Setup { .. } => CliCommandName::WorkspaceClaudeTokenSetup,
            WorkspaceClaudeTokenCommand::Rotate { .. } => {
                CliCommandName::WorkspaceClaudeTokenRotate
            }
            WorkspaceClaudeTokenCommand::Revoke { .. } => {
                CliCommandName::WorkspaceClaudeTokenRevoke
            }
            WorkspaceClaudeTokenCommand::Doctor { .. } => {
                CliCommandName::WorkspaceClaudeTokenDoctor
            }
        },
    }
}

const fn config_command_name(command: &ConfigCommand) -> CliCommandName {
    match command {
        ConfigCommand::Mount(command) => match command {
            MountCommand::Add { .. } => CliCommandName::ConfigMountAdd,
            MountCommand::Remove { .. } => CliCommandName::ConfigMountRemove,
            MountCommand::List => CliCommandName::ConfigMountList,
        },
        ConfigCommand::Trust(command) => match command {
            TrustCommand::Grant { .. } => CliCommandName::ConfigTrustGrant,
            TrustCommand::Revoke { .. } => CliCommandName::ConfigTrustRevoke,
            TrustCommand::List => CliCommandName::ConfigTrustList,
        },
        ConfigCommand::Auth(command) => match command {
            AuthCommand::Set { .. } => CliCommandName::ConfigAuthSet,
            AuthCommand::Show => CliCommandName::ConfigAuthShow,
        },
        ConfigCommand::Env(command) => match command {
            EnvCommand::Set { .. } => CliCommandName::ConfigEnvSet,
            EnvCommand::Unset { .. } => CliCommandName::ConfigEnvUnset,
            EnvCommand::List { .. } => CliCommandName::ConfigEnvList,
        },
        ConfigCommand::Git(command) => match command {
            GitCommand::CoauthorTrailer(command) => match command {
                CoauthorTrailerCommand::Enable => CliCommandName::ConfigGitCoauthorTrailerEnable,
                CoauthorTrailerCommand::Disable => CliCommandName::ConfigGitCoauthorTrailerDisable,
            },
            GitCommand::Dco(command) => match command {
                DcoCommand::Enable => CliCommandName::ConfigGitDcoEnable,
                DcoCommand::Disable => CliCommandName::ConfigGitDcoDisable,
            },
        },
    }
}
