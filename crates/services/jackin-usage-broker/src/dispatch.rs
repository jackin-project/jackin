// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker request dispatch and publication join.

use std::io::{BufRead, BufReader, Read};

use std::os::unix::net::UnixStream;

use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation,
    UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageProjectionRefreshStateV1,
};

use crate::{protocol_error, publish, unavailable};
use jackin_usage_coordinator::UsageCoordinator;

pub(crate) fn dispatch(
    coordinator: &UsageCoordinator,
    request: UsageBrokerRequest,
    build_id: &str,
    publisher: &publish::ProjectionPublisher,
) -> UsageBrokerResponse {
    let UsageBrokerRequest {
        protocol_version,
        build_id: request_build_id,
        operation,
        launch_credential_scope,
    } = request;
    if protocol_version != USAGE_BROKER_PROTOCOL_VERSION || request_build_id != build_id {
        return UsageBrokerResponse::Error {
            error: protocol_error(),
        };
    }
    if launch_credential_scope.is_some()
        && !matches!(
            &operation,
            UsageBrokerOperation::Current { .. }
                | UsageBrokerOperation::Refresh { .. }
                | UsageBrokerOperation::Join { .. }
        )
    {
        return UsageBrokerResponse::Error {
            error: UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "launch credential scope requires an account operation".to_owned(),
            },
        };
    }
    match &operation {
        UsageBrokerOperation::ReconcileCatalog {
            expected_projection_id,
            catalog_revision,
            entries,
        } => {
            return match publisher.reconcile_catalog_if_projection(
                expected_projection_id.as_deref(),
                catalog_revision.clone(),
                entries.clone(),
                chrono::Utc::now().timestamp(),
            ) {
                Ok(projection) => UsageBrokerResponse::Projection {
                    projection: Box::new(projection),
                },
                Err(error) => UsageBrokerResponse::Error { error },
            };
        }
        UsageBrokerOperation::CurrentProjection => return read_projection(publisher),
        UsageBrokerOperation::RequestRefresh {
            force,
            observed_projection_id: _,
        } => return refresh_projection(coordinator, publisher, *force),
        UsageBrokerOperation::JoinPublication {
            projection_id,
            timeout_ms,
        } => return join_publication(publisher, projection_id, *timeout_ms),
        UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => {
            return UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unauthorized,
                    message: "scoped projection operation requires a container relay".to_owned(),
                },
            };
        }
        _ => {}
    }
    let now = chrono::Utc::now().timestamp();
    let result = match operation {
        UsageBrokerOperation::ReconcileCatalog { .. } => Err(protocol_error()),
        UsageBrokerOperation::CurrentProjection
        | UsageBrokerOperation::RequestRefresh { .. }
        | UsageBrokerOperation::JoinPublication { .. }
        | UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => Err(protocol_error()),
        UsageBrokerOperation::CurrentForCapability { .. }
        | UsageBrokerOperation::RefreshForCapability { .. }
        | UsageBrokerOperation::JoinForCapability { .. } => Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "scoped usage operation requires a container relay".to_owned(),
        }),
        UsageBrokerOperation::Current { capability } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = coordinator.current(&capability, now);
            publisher.publish_due(now);
            result
        }
        UsageBrokerOperation::Refresh {
            capability,
            observed_generation,
            force,
        } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = match launch_credential_scope {
                Some(scope) => coordinator.request_refresh_scoped(
                    &capability,
                    observed_generation,
                    force,
                    now,
                    scope,
                ),
                None => coordinator.request_refresh(&capability, observed_generation, force, now),
            };
            publisher.publish_due(now);
            result
        }
        UsageBrokerOperation::Join {
            capability,
            generation,
            timeout_ms,
        } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = coordinator.join_generation(
                &capability,
                generation,
                Duration::from_millis(timeout_ms.min(30_000)),
                now,
            );
            publisher.publish_due(now);
            result
        }
    };
    match result {
        Ok(state) => UsageBrokerResponse::State {
            state: Box::new(state),
        },
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

pub(crate) fn read_projection(publisher: &publish::ProjectionPublisher) -> UsageBrokerResponse {
    match publisher.current_projection() {
        Ok(projection) => UsageBrokerResponse::Projection {
            projection: Box::new(projection),
        },
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

/// Request due observations for every observed account and return the latest
/// publication. Each account runs its normal due check: still-fresh data is
/// reused, active work is joined, and retry deadlines always win. `force` is
/// honored only as the coordinator honors it — an explicit operator refresh
/// bypasses the success cooldown but never retry or rate-limit deadlines.
pub(crate) fn refresh_projection(
    coordinator: &UsageCoordinator,
    publisher: &publish::ProjectionPublisher,
    force: bool,
) -> UsageBrokerResponse {
    let now = chrono::Utc::now().timestamp();
    let known = publisher.known_capabilities();
    if !known.is_empty() {
        let mut requests = Vec::with_capacity(known.len());
        for capability in &known {
            // A read failure resolves to generation 0, which can only adopt
            // the current winner — never force a duplicate generation.
            let observed = coordinator
                .current(capability, now)
                .map_or(0, |view| view.generation);
            requests.push((capability.clone(), observed));
        }
        let _ignored = coordinator.request_refresh_all(requests, force, now);
        publisher.publish_due(now);
    }
    read_projection(publisher)
}

/// Wait until one named publication settles or is superseded.
///
/// A newer publication means the requested one is terminal history and is
/// returned immediately. Waiting only happens while the requested publication
/// is current and still refreshing. Expiry reports `WaitTimeout` without
/// touching broker ownership: generations always run to terminal.
pub(crate) fn join_publication(
    publisher: &publish::ProjectionPublisher,
    projection_id: &str,
    timeout_ms: u64,
) -> UsageBrokerResponse {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(30_000));
    loop {
        let current = match publisher.current_projection() {
            Ok(current) => current,
            Err(error) => return UsageBrokerResponse::Error { error },
        };
        if current.projection_id != projection_id
            || current.refresh_state != UsageProjectionRefreshStateV1::Refreshing
        {
            return UsageBrokerResponse::Projection {
                projection: Box::new(current),
            };
        }
        publisher.publish_due(chrono::Utc::now().timestamp());
        if Instant::now() >= deadline {
            return UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::WaitTimeout,
                    message: "usage projection publication is still refreshing".to_owned(),
                },
            };
        }
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

pub(crate) fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T, UsageCoordinationError> {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    let read = reader
        .by_ref()
        .take(u64::try_from(USAGE_BROKER_MAX_FRAME_BYTES).unwrap_or(u64::MAX) + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| unavailable())?;
    if read == 0 || read > USAGE_BROKER_MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        return Err(protocol_error());
    }
    bytes.pop();
    serde_json::from_slice(&bytes).map_err(|_| protocol_error())
}
