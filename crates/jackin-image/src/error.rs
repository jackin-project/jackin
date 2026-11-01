// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Platform;
use thiserror::Error;

use crate::dockerfile::DockerfileError;
use crate::engine::EngineError;

#[derive(Debug, Error)]
pub enum ImageError {
    #[error(transparent)]
    Dockerfile(#[from] DockerfileError),
    #[error("the agent has no install target for {platform:?}")]
    MissingInstallTarget { platform: Platform },
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("cancelled")]
    Cancelled,
}
