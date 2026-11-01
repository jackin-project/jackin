// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::LabelError;
use jackin_core::{Phase, SandboxName};
use jackin_openshell::GatewayError;
use thiserror::Error;

use crate::export::WorkState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PurgeMode {
    Guarded,
    Forced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurgeOutcome {
    Deleted {
        sandbox: SandboxName,
        was: WorkState,
    },
    AlreadyGone {
        sandbox: SandboxName,
    },
}

#[derive(Debug, Error)]
pub enum PurgeError {
    #[error("the work in {sandbox:?} is not exported: {state:?}")]
    WorkNotExported {
        sandbox: SandboxName,
        state: WorkState,
    },
    #[error(transparent)]
    Gateway(#[from] GatewayError),
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("the labels of {sandbox:?} are invalid")]
    Labels {
        sandbox: SandboxName,
        source: LabelError,
    },
    #[error("sandbox {0:?} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox:?} did not reach {wanted:?} within {after:?}")]
    PowerTimeout {
        sandbox: SandboxName,
        wanted: Phase,
        after: Duration,
    },
    #[error("the main process of {sandbox:?} has ended: {phase:?}")]
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
}
