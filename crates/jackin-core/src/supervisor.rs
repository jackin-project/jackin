// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use thiserror::Error;

use crate::ids::{AttemptId, IdError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum SupervisorState {
    Waiting,
    Running { attempt: AttemptId },
    Exited { attempt: AttemptId, code: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct StartRequest(pub AttemptId);

#[derive(Debug, Error)]
pub enum SupervisorStateError {
    #[error("supervisor state is empty")]
    Empty,
    #[error("unknown supervisor state word {0:?}")]
    UnknownWord(String),
    #[error("invalid attempt ID")]
    BadAttempt(IdError),
    #[error("invalid exit code {0:?}")]
    BadExitCode(String),
    #[error("trailing text {0:?}")]
    TrailingText(String),
}
