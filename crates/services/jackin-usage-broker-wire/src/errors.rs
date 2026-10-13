// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker wire error constructors.

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};

pub fn unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage broker is unavailable".to_owned(),
    }
}

pub fn credential_scope_mismatch() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unauthorized,
        message: "launch credential source no longer matches staged material".to_owned(),
    }
}

pub fn protocol_error() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::ProtocolMismatch,
        message: "usage broker protocol mismatch".to_owned(),
    }
}

pub fn catalog_discovery_mismatch() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
        message: "usage broker catalog does not match current discovery".to_owned(),
    }
}
