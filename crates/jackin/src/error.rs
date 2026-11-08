// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    CompatibilityError, ConfigError, EntropyError, EnvName, EnvValueError, FsError, GroupFault,
    HostPath, PathError, ProcessError, plain,
};
use jackin_image::{EngineError, ImageError};
use jackin_openshell::GatewayError;
use jackin_session::{
    AccountEditError, AccountError, AttachError, BuildError, ConnectError, CredentialError,
    ExportError, LaunchError, LaunchFailure, LifecycleError, LockError, ManagedIdentityError,
    PlanError, PurgeError, RestartError, RoleError, WorkspaceGuardError,
};
use jackin_tui::ConsoleError;
use jackin_usage::UsageError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExitClass {
    Ok,
    Failure,
    Usage,
    Refused,
    Cancelled,
}

impl ExitClass {
    pub fn code(self) -> u8 {
        match self {
            ExitClass::Ok => 0,
            ExitClass::Failure => 1,
            ExitClass::Usage => 2,
            ExitClass::Refused => 3,
            ExitClass::Cancelled => 130,
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("{}", plain(.0))]
    Argument(String),
    #[error("--env {name} is refused")]
    EnvValue {
        name: EnvName,
        #[source]
        source: EnvValueError,
    },
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Role(#[from] RoleError),
    #[error("the role is invalid")]
    RoleFindings(Vec<RoleError>),
    #[error("launch failed at step {}: {}", .0.step, .0.error)]
    Launch(LaunchFailure),
    #[error(transparent)]
    Attach(#[from] AttachError),
    #[error(transparent)]
    Restart(#[from] RestartError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Purge(#[from] PurgeError),
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Accounts(AccountEditError),
    #[error(transparent)]
    WorkspaceInUse(#[from] WorkspaceGuardError),
    #[error("cannot import the ChatGPT login from {}", .home)]
    Login {
        home: HostPath,
        source: CredentialError,
    },
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Connect(ConnectError),
    #[error(transparent)]
    Entropy(#[from] EntropyError),
    #[error("terminal error: {0}")]
    Terminal(std::io::Error),
    #[error("cannot write the output: {0}")]
    Output(std::io::Error),
    #[error("cannot encode the output: {0}")]
    Json(serde_json::Error),
    #[error(transparent)]
    Console(#[from] ConsoleError),
    #[error(transparent)]
    Usage(#[from] UsageError),
    #[error("a terminal is required")]
    NoTerminal,
    #[error("{}", plain(.0))]
    Session(String),
    #[error("{}", plain(.0))]
    Cancelled(String),
    #[error("the environment is incomplete: {}", plain(.0))]
    Environment(String),
    #[error("the checks did not pass")]
    Unhealthy,
}

impl From<AccountEditError> for CommandError {
    fn from(error: AccountEditError) -> CommandError {
        match error {
            AccountEditError::Config(error) => CommandError::Config(error),
            AccountEditError::Account(error) => CommandError::Account(error),
            AccountEditError::Connect(error) => CommandError::from(error),
            other => CommandError::Accounts(other),
        }
    }
}

impl From<BuildError> for CommandError {
    fn from(error: BuildError) -> CommandError {
        match error {
            BuildError::Config(error) => CommandError::Config(error),
            BuildError::Role(error) => CommandError::Role(error),
            BuildError::Plan(error) => CommandError::Plan(error),
            BuildError::Image(error) => CommandError::Image(error),
        }
    }
}

impl From<ConnectError> for CommandError {
    fn from(error: ConnectError) -> CommandError {
        let cancelled = error.cancel().map(|group| {
            format!(
                "cancelled while connecting to the gateway{}",
                GroupFault::note(group)
            )
        });
        match cancelled {
            Some(text) => CommandError::Cancelled(text),
            None => CommandError::Connect(error),
        }
    }
}

fn image_cancelled(error: &ImageError) -> bool {
    matches!(
        error,
        ImageError::Cancelled { .. } | ImageError::Engine(EngineError::Cancelled { .. })
    )
}

fn account_cancelled(error: &AccountError) -> bool {
    matches!(
        error,
        AccountError::Lock(LockError::Cancelled)
            | AccountError::Config(ConfigError::Cancelled)
            | AccountError::Gateway(GatewayError::Cancelled { .. })
    )
}

fn lifecycle_cancelled(error: &LifecycleError) -> bool {
    matches!(
        error,
        LifecycleError::Cancelled
            | LifecycleError::Identity(ManagedIdentityError::Cancelled)
            | LifecycleError::Gateway(GatewayError::Cancelled { .. })
    )
}

fn launch_cancelled(error: &LaunchError) -> bool {
    match error {
        LaunchError::Image(error) => image_cancelled(error),
        LaunchError::Gateway(GatewayError::Cancelled { .. })
        | LaunchError::Config(ConfigError::Cancelled)
        | LaunchError::Process(ProcessError::Cancelled { .. }) => true,
        LaunchError::ProviderUnavailable { source, .. } => account_cancelled(source),
        _ => false,
    }
}

impl CommandError {
    fn is_cancelled(&self) -> bool {
        match self {
            CommandError::Cancelled(_)
            | CommandError::Usage(UsageError::Cancelled)
            | CommandError::Config(ConfigError::Cancelled)
            | CommandError::Console(ConsoleError::Config(ConfigError::Cancelled))
            | CommandError::Plan(PlanError::Config(ConfigError::Cancelled))
            | CommandError::Role(RoleError::Cancelled)
            | CommandError::Attach(
                AttachError::Gateway(GatewayError::Cancelled { .. })
                | AttachError::Identity(ManagedIdentityError::Cancelled),
            )
            | CommandError::Restart(RestartError::Gateway(GatewayError::Cancelled { .. }))
            | CommandError::Export(
                ExportError::Cancelled { .. }
                | ExportError::Gateway(GatewayError::Cancelled { .. })
                | ExportError::Process(ProcessError::Cancelled { .. }),
            )
            | CommandError::Purge(
                PurgeError::Cancelled
                | PurgeError::Identity(ManagedIdentityError::Cancelled)
                | PurgeError::Gateway(GatewayError::Cancelled { .. }),
            )
            | CommandError::WorkspaceInUse(WorkspaceGuardError::Lock(LockError::Cancelled))
            | CommandError::Accounts(
                AccountEditError::Config(ConfigError::Cancelled)
                | AccountEditError::OnGateway {
                    source: GatewayError::Cancelled { .. },
                    ..
                },
            ) => true,
            CommandError::Image(error) | CommandError::Plan(PlanError::Image(error)) => {
                image_cancelled(error)
            }
            CommandError::Account(error)
            | CommandError::Accounts(AccountEditError::Account(error)) => account_cancelled(error),
            CommandError::Lifecycle(error)
            | CommandError::WorkspaceInUse(WorkspaceGuardError::Listing {
                source: error, ..
            }) => lifecycle_cancelled(error),
            CommandError::Connect(error)
            | CommandError::Accounts(AccountEditError::Connect(error)) => error.is_cancelled(),
            CommandError::RoleFindings(findings) => {
                !findings.is_empty()
                    && findings
                        .iter()
                        .all(|error| matches!(error, RoleError::Cancelled))
            }
            CommandError::Launch(failure) => launch_cancelled(&failure.error),
            CommandError::Argument(_)
            | CommandError::EnvValue { .. }
            | CommandError::Config(_)
            | CommandError::Path(_)
            | CommandError::Fs(_)
            | CommandError::Compatibility(_)
            | CommandError::Plan(_)
            | CommandError::Role(_)
            | CommandError::Attach(_)
            | CommandError::Restart(_)
            | CommandError::Export(_)
            | CommandError::Purge(_)
            | CommandError::Accounts(_)
            | CommandError::WorkspaceInUse(_)
            | CommandError::Login { .. }
            | CommandError::Entropy(_)
            | CommandError::Terminal(_)
            | CommandError::Output(_)
            | CommandError::Json(_)
            | CommandError::Console(_)
            | CommandError::Usage(_)
            | CommandError::NoTerminal
            | CommandError::Session(_)
            | CommandError::Environment(_)
            | CommandError::Unhealthy => false,
        }
    }

    pub fn class(&self) -> ExitClass {
        if self.is_cancelled() {
            return ExitClass::Cancelled;
        }
        match self {
            CommandError::Argument(_) | CommandError::EnvValue { .. } => ExitClass::Usage,
            CommandError::Plan(PlanError::EnvNeeded(_))
            | CommandError::Restart(RestartError::AlreadyRunning { .. })
            | CommandError::Purge(
                PurgeError::WorkNotExported { .. }
                | PurgeError::Replaced { .. }
                | PurgeError::Identity(_),
            ) => ExitClass::Refused,
            _ => ExitClass::Failure,
        }
    }
}
