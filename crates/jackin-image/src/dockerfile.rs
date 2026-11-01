// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DockerfileError {
    #[error("the Dockerfile has no stages")]
    NoStages,
    #[error("line {line}: the final stage must start FROM construct, found {found:?}")]
    FinalStageNotConstruct { line: usize, found: String },
    #[error("line {line}: cannot read the FROM instruction {text:?}")]
    UnreadableFrom { line: usize, text: String },
    #[error("the Dockerfile is not valid UTF-8")]
    NotUtf8,
}
