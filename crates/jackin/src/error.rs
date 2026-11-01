// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::ConfigError;
use jackin_image::ImageError;
use jackin_openshell::{DiscoveryError, GatewayError};
use jackin_session::{
    AccountError, AttachError, ExportError, LaunchFailure, LifecycleError, PlanError, PurgeError,
    RestartError, RoleError,
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

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("{0}")]
    Argument(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Role(#[from] RoleError),
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
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("terminal error: {0}")]
    Terminal(std::io::Error),
    #[error(transparent)]
    Console(#[from] ConsoleError),
    #[error("a terminal is required")]
    NoTerminal,
}
