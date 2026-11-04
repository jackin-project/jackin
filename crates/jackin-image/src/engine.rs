// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use async_trait::async_trait;
use jackin_core::{
    BuilderName, FileLock, FsError, GroupFault, ImageId, ImageTag, ProcessError, ProcessStatus,
    TerminalText, plain,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::ensure::ProgressSink;
use crate::plan::{BuildStage, PlannedBuild};

#[async_trait]
pub trait ImageEngine: Send + Sync + Debug {
    async fn find(
        &self,
        tag: &ImageTag,
        cancel: &CancellationToken,
    ) -> Result<Option<ImageId>, EngineError>;
    async fn check_builder(
        &self,
        builder: &BuilderName,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError>;
    async fn hold(
        &self,
        _key: &ImageTag,
        _cancel: &CancellationToken,
    ) -> Result<Option<FileLock>, EngineError> {
        Ok(None)
    }
    async fn build(
        &self,
        build: &PlannedBuild,
        progress: &ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ContextProblem {
    #[error("is a symbolic link")]
    Symlink,
    #[error("is not a regular file")]
    NotARegularFile,
    #[error("does not exist")]
    Missing,
    #[error("leaves the build context")]
    Escapes,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("builder {builder} does not exist")]
    BuilderMissing { builder: BuilderName },
    #[error("builder {builder} uses the {} driver; the docker driver is required", plain(.driver))]
    BuilderDriver {
        builder: BuilderName,
        driver: String,
    },
    #[error("the {stage} build failed with {status}:\n{}", .tail.as_str())]
    BuildFailed {
        stage: BuildStage,
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("cannot read the driver of builder {builder}; its inspect command ended with {status}:\n{}", .output.as_str())]
    BuilderUnreadable {
        builder: BuilderName,
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error("the build context cannot hold {}: it {problem}", plain(.path))]
    ContextEntry {
        path: String,
        problem: ContextProblem,
    },
    #[error("cannot read the image ID of {tag}:\n{}", .output.as_str())]
    DigestUnreadable { tag: ImageTag, output: TerminalText },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("gave up waiting for another process to finish building {key}")]
    LockTimeout { key: ImageTag },
    #[error("cancelled{}", GroupFault::note(.group.as_ref()))]
    Cancelled { group: Option<GroupFault> },
}
