// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use async_trait::async_trait;
use jackin_core::{
    BuilderName, FsError, ImageId, ImageTag, ProcessError, ProcessStatus, TerminalText,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::ensure::ProgressSink;
use crate::plan::{BuildStage, PlannedBuild};

#[async_trait]
pub trait ImageEngine: Send + Sync + Debug {
    async fn find(&self, tag: &ImageTag) -> Result<Option<ImageId>, EngineError>;
    async fn check_builder(&self, builder: &BuilderName) -> Result<(), EngineError>;
    async fn build(
        &self,
        build: &PlannedBuild,
        progress: &ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError>;
}

#[derive(Clone, Debug)]
pub struct BuildxEngine {}

fn describe(status: ProcessStatus) -> String {
    match status {
        ProcessStatus::Exited(code) => format!("exit code {code}"),
        ProcessStatus::Signaled(signal) => format!("signal {signal}"),
    }
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("builder {builder} does not exist")]
    BuilderMissing { builder: BuilderName },
    #[error("builder {builder} uses the {driver} driver; the docker driver is required")]
    BuilderDriver {
        builder: BuilderName,
        driver: String,
    },
    #[error("the {stage} build failed with {}:\n{}", describe(*.status), .tail.as_str())]
    BuildFailed {
        stage: BuildStage,
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("cannot read the image ID of {tag}:\n{}", .output.as_str())]
    DigestUnreadable { tag: ImageTag, output: TerminalText },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("cancelled")]
    Cancelled,
}
