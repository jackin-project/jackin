// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    CompatibilityError, ConfigError, EntropyError, FsError, HostPath, PathError, ProcessError,
};
use jackin_image::ImageError;
use jackin_openshell::{DiscoveryError, GatewayError};
use jackin_session::{
    AccountError, AttachError, CredentialError, ExportError, LaunchError, LaunchFailure,
    LifecycleError, PlanError, PurgeError, RestartError, RoleError,
};
use jackin_tui::ConsoleError;
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
    #[error("{0}")]
    Argument(String),
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
    #[error("launch failed at step {:?}: {}", .0.step, .0.error)]
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
    #[error("cannot import the ChatGPT login from {home:?}: {source}")]
    Login {
        home: HostPath,
        source: CredentialError,
    },
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
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
    #[error("a terminal is required")]
    NoTerminal,
    #[error("{0}")]
    Session(String),
    #[error("{0}")]
    Cancelled(String),
    #[error("the environment is incomplete: {0}")]
    Environment(String),
    #[error("the checks did not pass")]
    Unhealthy,
}

fn gateway_cancelled(error: &GatewayError) -> bool {
    matches!(error, GatewayError::Cancelled { .. })
        || matches!(error, GatewayError::Process(ProcessError::Cancelled { .. }))
}

fn launch_cancelled(error: &LaunchError) -> bool {
    match error {
        LaunchError::Image(ImageError::Cancelled) => true,
        LaunchError::Gateway(error) => gateway_cancelled(error),
        LaunchError::Process(ProcessError::Cancelled { .. }) => true,
        _ => false,
    }
}

impl CommandError {
    pub fn class(&self) -> ExitClass {
        match self {
            CommandError::Argument(_) => ExitClass::Usage,
            CommandError::Plan(PlanError::EnvNeeded(_))
            | CommandError::Restart(RestartError::AlreadyRunning { .. })
            | CommandError::Purge(PurgeError::WorkNotExported { .. }) => ExitClass::Refused,
            CommandError::Cancelled(_)
            | CommandError::Image(ImageError::Cancelled)
            | CommandError::Export(ExportError::Cancelled) => ExitClass::Cancelled,
            CommandError::Gateway(error)
            | CommandError::Export(ExportError::Gateway(error))
            | CommandError::Restart(RestartError::Gateway(error))
            | CommandError::Account(AccountError::Gateway(error))
                if gateway_cancelled(error) =>
            {
                ExitClass::Cancelled
            }
            CommandError::Launch(failure) if launch_cancelled(&failure.error) => {
                ExitClass::Cancelled
            }
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
            | CommandError::Lifecycle(_)
            | CommandError::Export(_)
            | CommandError::Purge(_)
            | CommandError::Account(_)
            | CommandError::Login { .. }
            | CommandError::Image(_)
            | CommandError::Gateway(_)
            | CommandError::Discovery(_)
            | CommandError::Entropy(_)
            | CommandError::Terminal(_)
            | CommandError::Output(_)
            | CommandError::Json(_)
            | CommandError::Console(_)
            | CommandError::NoTerminal
            | CommandError::Session(_)
            | CommandError::Environment(_)
            | CommandError::Unhealthy => ExitClass::Failure,
        }
    }
}
