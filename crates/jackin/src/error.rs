// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    CompatibilityError, ConfigError, EntropyError, EnvName, EnvValueError, FsError, GroupFault,
    HostPath, PathError, plain,
};
use jackin_image::ImageError;
use jackin_openshell::GatewayError;
use jackin_session::{
    AccountEditError, AccountError, AttachError, BuildError, ConnectError, CredentialError,
    ExportError, IntegrateError, LaunchFailure, LifecycleError, LockError, PlanError, PurgeError,
    RestartError, RoleError, WorkspaceGuardError,
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
    #[error("provider {} with an API key needs --agent omp", .provider.as_str())]
    ApiKeyNeedsAgent { provider: jackin_core::Provider },
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
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Integrate(#[from] IntegrateError),
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

impl CommandError {
    pub fn class(&self) -> ExitClass {
        match self {
            CommandError::Argument(_)
            | CommandError::ApiKeyNeedsAgent { .. }
            | CommandError::EnvValue { .. }
            | CommandError::Integrate(IntegrateError::UnknownAgent { .. }) => ExitClass::Usage,
            CommandError::Plan(PlanError::EnvNeeded(_))
            | CommandError::Restart(RestartError::AlreadyRunning { .. })
            | CommandError::Purge(
                PurgeError::WorkNotExported { .. } | PurgeError::Replaced { .. },
            ) => ExitClass::Refused,
            CommandError::Cancelled(_)
            | CommandError::Usage(UsageError::Cancelled)
            | CommandError::Role(RoleError::Cancelled)
            | CommandError::Lifecycle(LifecycleError::Cancelled)
            | CommandError::Image(ImageError::Cancelled { .. })
            | CommandError::Export(ExportError::Cancelled { .. })
            | CommandError::Account(AccountError::Lock(LockError::Cancelled))
            | CommandError::Restart(RestartError::Gateway(GatewayError::Cancelled { .. }))
            | CommandError::Gateway(GatewayError::Cancelled { .. })
            | CommandError::Integrate(IntegrateError::Gateway(GatewayError::Cancelled {
                ..
            })) => ExitClass::Cancelled,
            CommandError::Config(_)
            | CommandError::Path(_)
            | CommandError::Fs(_)
            | CommandError::Compatibility(_)
            | CommandError::Plan(_)
            | CommandError::Role(_)
            | CommandError::RoleFindings(_)
            | CommandError::Launch(_)
            | CommandError::Attach(_)
            | CommandError::Restart(_)
            | CommandError::Gateway(_)
            | CommandError::Integrate(_)
            | CommandError::Lifecycle(_)
            | CommandError::Export(_)
            | CommandError::Purge(_)
            | CommandError::Account(_)
            | CommandError::Accounts(_)
            | CommandError::WorkspaceInUse(_)
            | CommandError::Login { .. }
            | CommandError::Image(_)
            | CommandError::Connect(_)
            | CommandError::Entropy(_)
            | CommandError::Terminal(_)
            | CommandError::Output(_)
            | CommandError::Json(_)
            | CommandError::Console(_)
            | CommandError::Usage(_)
            | CommandError::NoTerminal
            | CommandError::Session(_)
            | CommandError::Environment(_)
            | CommandError::Unhealthy => ExitClass::Failure,
        }
    }
}
