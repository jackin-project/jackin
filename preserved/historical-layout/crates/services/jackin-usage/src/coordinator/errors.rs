// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Coordination errors.

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};

use super::StateStoreError;

pub(crate) fn state_error(error: StateStoreError) -> UsageCoordinationError {
    match error {
        StateStoreError::Unavailable => unavailable_error(),
        StateStoreError::Corrupt => coordination_error(
            UsageCoordinationErrorKind::CorruptState,
            "usage coordinator state is corrupt",
        ),
    }
}

pub(crate) fn unavailable_error() -> UsageCoordinationError {
    coordination_error(
        UsageCoordinationErrorKind::Unavailable,
        "usage coordinator is unavailable",
    )
}

pub(crate) fn catalog_revoked_error() -> UsageCoordinationError {
    coordination_error(
        UsageCoordinationErrorKind::CatalogRevoked,
        "usage account capability was removed from the current broker catalog",
    )
}

pub(crate) fn coordination_error(
    kind: UsageCoordinationErrorKind,
    message: impl AsRef<str>,
) -> UsageCoordinationError {
    UsageCoordinationError {
        kind,
        message: message
            .as_ref()
            .chars()
            .filter(|character| !character.is_control())
            .take(256)
            .collect(),
    }
}
