// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `GrantValidationError` and its impls.

use super::{VALID_CAPABILITIES, format_bytes};

/// Errors produced by [`validate_grants`] before any container is started.
#[derive(Debug)]
pub enum GrantValidationError {
    /// `user = "root"` and `sudo = true` are mutually exclusive.
    RootAndSudo,
    /// An entry in `capabilities_add` is not a recognized Linux capability.
    UnknownCapability(String),
    /// `memory_reservation` exceeds `memory` (both provided).
    MemoryReservationExceedsMemory { reservation: u64, memory: u64 },
    /// A size string could not be parsed.
    UnparsableSize { field: &'static str, value: String },
    /// A numeric field is outside its valid range (e.g. `pids <= 0`, memory > `i64::MAX`).
    ValueOutOfRange {
        field: &'static str,
        reason: &'static str,
    },
}

impl std::fmt::Display for GrantValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootAndSudo => write!(
                f,
                "grants.user = \"root\" and grants.sudo = true are mutually exclusive: \
                 root does not need sudo escalation; remove one of the two grants"
            ),
            Self::UnknownCapability(cap) => {
                write!(
                    f,
                    "unknown Linux capability {cap:?} in grants.capabilities_add — \
                     valid values: {}",
                    VALID_CAPABILITIES.join(", ")
                )
            }
            Self::MemoryReservationExceedsMemory {
                reservation,
                memory,
            } => write!(
                f,
                "grants.memory_reservation ({}) must be ≤ grants.memory ({})",
                format_bytes(*reservation),
                format_bytes(*memory),
            ),
            Self::UnparsableSize { field, value } => write!(
                f,
                "cannot parse {value:?} as a size for grants.{field} — \
                 use format \"512M\", \"4G\", \"32G\""
            ),
            Self::ValueOutOfRange { field, reason } => {
                write!(f, "grants.{field} is out of range: {reason}")
            }
        }
    }
}

impl std::error::Error for GrantValidationError {}
