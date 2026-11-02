// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Provisioning,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Completed { exit_code: i32 },
    Error { exit_code: Option<i32> },
    Deleting,
    Suspending,
    Suspended,
    Unknown,
}

impl Phase {
    pub fn accepts_exec(self) -> bool {
        matches!(self, Phase::Ready)
    }

    pub fn main_process_ended(self) -> bool {
        matches!(self, Phase::Completed { .. } | Phase::Error { .. })
    }

    pub fn exit_code(self) -> Option<i32> {
        match self {
            Phase::Completed { exit_code } => Some(exit_code),
            Phase::Error { exit_code } => exit_code,
            Phase::Provisioning
            | Phase::Starting
            | Phase::Ready
            | Phase::Stopping
            | Phase::Stopped
            | Phase::Deleting
            | Phase::Suspending
            | Phase::Suspended
            | Phase::Unknown => None,
        }
    }

    pub fn is_settled(self) -> bool {
        match self {
            Phase::Ready
            | Phase::Stopped
            | Phase::Completed { .. }
            | Phase::Error { .. }
            | Phase::Suspended => true,
            Phase::Provisioning
            | Phase::Starting
            | Phase::Stopping
            | Phase::Deleting
            | Phase::Suspending
            | Phase::Unknown => false,
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Phase::Provisioning => f.write_str("provisioning"),
            Phase::Starting => f.write_str("starting"),
            Phase::Ready => f.write_str("ready"),
            Phase::Stopping => f.write_str("stopping"),
            Phase::Stopped => f.write_str("stopped"),
            Phase::Completed { exit_code } => write!(f, "completed with exit code {exit_code}"),
            Phase::Error {
                exit_code: Some(code),
            } => write!(f, "error with exit code {code}"),
            Phase::Error { exit_code: None } => f.write_str("error without an exit code"),
            Phase::Deleting => f.write_str("deleting"),
            Phase::Suspending => f.write_str("suspending"),
            Phase::Suspended => f.write_str("suspended"),
            Phase::Unknown => f.write_str("unknown"),
        }
    }
}
