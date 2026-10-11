// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Publisher errors.

use jackin_protocol::usage_broker::{
    UsageCoordinationError, UsageCoordinationErrorKind, UsageIssueRecoverabilityV1,
};

use jackin_usage_coordinator::StateStoreError;

pub fn publisher_unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage projection publisher is unavailable".to_owned(),
    }
}

pub fn projection_store_error(error: StateStoreError) -> UsageCoordinationError {
    match error {
        StateStoreError::Unavailable => publisher_unavailable(),
        StateStoreError::SchemaMigrationRequired { .. } => publisher_unavailable(),
        StateStoreError::Corrupt => UsageCoordinationError {
            kind: UsageCoordinationErrorKind::CorruptState,
            message: "usage broker projection state is corrupt".to_owned(),
        },
    }
}

pub fn first_publisher_rollback_error(
    projection: Result<(), StateStoreError>,
    coordinator: Result<(), UsageCoordinationError>,
) -> Result<(), UsageCoordinationError> {
    match (projection, coordinator) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) => Err(projection_store_error(error)),
        (Ok(()), Err(error)) => Err(error),
    }
}

pub fn preserve_publisher_error(
    primary: UsageCoordinationError,
    rollback: Result<(), UsageCoordinationError>,
) -> UsageCoordinationError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => UsageCoordinationError {
            kind: primary.kind,
            message: format!(
                "{}; publication rollback failed: {}",
                primary.message, rollback.message
            ),
        },
    }
}

pub fn publisher_corrupt_state() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CorruptState,
        message: "usage broker catalog is invalid".to_owned(),
    }
}

pub fn catalog_revision_conflict() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
        message: "usage broker catalog publication lease is stale".to_owned(),
    }
}

pub const fn issue_recoverability(kind: UsageCoordinationErrorKind) -> UsageIssueRecoverabilityV1 {
    match kind {
        UsageCoordinationErrorKind::NeedsSecret
        | UsageCoordinationErrorKind::Unauthorized
        | UsageCoordinationErrorKind::BrokerConflict => UsageIssueRecoverabilityV1::ActionRequired,
        UsageCoordinationErrorKind::ProtocolMismatch
        | UsageCoordinationErrorKind::CorruptState
        | UsageCoordinationErrorKind::OwnerLost
        | UsageCoordinationErrorKind::CatalogRevoked
        | UsageCoordinationErrorKind::CatalogRevisionConflict => {
            UsageIssueRecoverabilityV1::Terminal
        }
        UsageCoordinationErrorKind::Unavailable
        | UsageCoordinationErrorKind::WaitTimeout
        | UsageCoordinationErrorKind::ProviderTimeout
        | UsageCoordinationErrorKind::ProviderUnavailable
        | UsageCoordinationErrorKind::RateLimited => UsageIssueRecoverabilityV1::Retryable,
    }
}
