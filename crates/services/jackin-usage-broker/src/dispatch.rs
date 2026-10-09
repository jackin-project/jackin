// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker request dispatch and publication join.

use std::io::{BufRead, BufReader, Read};

use std::os::unix::net::UnixStream;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation,
    UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageProjectionRefreshStateV1, UsageRelayCapabilityMappingV1,
    UsageRelayCapabilityResolutionV1, UsageRelayForwardedSourcesV1,
};
use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, MonitorOperation, MonitorReply,
};

use crate::{
    BROKER_ACTIVATION_ATTEMPTS, BrokerCatalogRefresh, ForwardedUsageSources, MonitorStore,
    forwarded_usage_capabilities, protocol_error, publish, unavailable,
    usage_capability_for_selected_account_with_sources,
};
use jackin_usage_coordinator::UsageCoordinator;

pub(crate) fn dispatch(
    coordinator: &UsageCoordinator,
    request: UsageBrokerRequest,
    build_id: &str,
    publisher: &publish::ProjectionPublisher,
    monitor_store: &MonitorStore,
    shutdown: &AtomicBool,
    catalog_refresh: Option<&BrokerCatalogRefresh>,
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
    if let Some(response) = dispatch_monitor_operation(&operation, monitor_store, shutdown) {
        return response;
    }
    if let Some(response) =
        dispatch_projection_operation(coordinator, &operation, publisher, catalog_refresh)
    {
        return response;
    }
    let now = chrono::Utc::now().timestamp();
    let result = dispatch_capability_operation(
        coordinator,
        operation,
        launch_credential_scope.as_ref(),
        publisher,
        now,
    );
    match result {
        Ok(state) => UsageBrokerResponse::State {
            state: Box::new(state),
        },
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

fn dispatch_monitor_operation(
    operation: &UsageBrokerOperation,
    monitor_store: &MonitorStore,
    shutdown: &AtomicBool,
) -> Option<UsageBrokerResponse> {
    let UsageBrokerOperation::Monitor { request } = operation else {
        return None;
    };
    let now = chrono::Utc::now().timestamp();
    if matches!(request, MonitorOperation::PrepareAuth { .. }) {
        return Some(monitor_error(
            MonitorIssueCode::InteractionRequired,
            "interactive authentication preparation requires an explicit TTY operator command",
            None,
        ));
    }
    let stopping = matches!(request, MonitorOperation::ServiceStop);
    let reply = match monitor_store.operate(request.clone(), now) {
        Ok(reply) => reply,
        Err(issue) => return Some(UsageBrokerResponse::MonitorError { issue }),
    };
    if stopping && matches!(reply, MonitorReply::ServiceStopped) {
        shutdown.store(true, Ordering::Release);
    }
    Some(UsageBrokerResponse::Monitor { reply })
}

fn dispatch_projection_operation(
    coordinator: &UsageCoordinator,
    operation: &UsageBrokerOperation,
    publisher: &publish::ProjectionPublisher,
    catalog_refresh: Option<&BrokerCatalogRefresh>,
) -> Option<UsageBrokerResponse> {
    match operation {
        UsageBrokerOperation::ResolveRelayCapabilities {
            scope_label,
            forwarded_sources,
        } => Some(resolve_relay_capabilities(
            publisher,
            catalog_refresh,
            scope_label,
            forwarded_sources,
        )),
        UsageBrokerOperation::CurrentProjection => Some(read_projection(publisher)),
        UsageBrokerOperation::RequestRefresh { force, .. } => Some(
            refresh_projection_with_catalog(coordinator, publisher, *force, catalog_refresh),
        ),
        UsageBrokerOperation::JoinPublication {
            projection_id,
            timeout_ms,
        } => Some(join_publication(publisher, projection_id, *timeout_ms)),
        UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => {
            Some(UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unauthorized,
                    message: "scoped projection operation requires a container relay".to_owned(),
                },
            })
        }
        _ => None,
    }
}

fn dispatch_capability_operation(
    coordinator: &UsageCoordinator,
    operation: UsageBrokerOperation,
    launch_credential_scope: Option<&jackin_protocol::usage_broker::UsageCredentialScope>,
    publisher: &publish::ProjectionPublisher,
    now: i64,
) -> Result<UsageGenerationView, UsageCoordinationError> {
    match operation {
        UsageBrokerOperation::CurrentForCapability { .. }
        | UsageBrokerOperation::RefreshForCapability { .. }
        | UsageBrokerOperation::JoinForCapability { .. } => Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "scoped usage operation requires a container relay".to_owned(),
        }),
        UsageBrokerOperation::Current { capability } => {
            if let Some(scope) = launch_credential_scope
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return Err(error);
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
            if let Some(scope) = launch_credential_scope
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return Err(error);
            }
            publisher.observe(&capability);
            let result = match launch_credential_scope {
                Some(scope) => coordinator.request_refresh_scoped(
                    &capability,
                    observed_generation,
                    force,
                    now,
                    scope.clone(),
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
            if let Some(scope) = launch_credential_scope
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return Err(error);
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
        _ => Err(protocol_error()),
    }
}

/// Resolve a launch from fresh discovery inside the broker, then reconcile
/// that same generation before returning any authority. A stale catalog scan
/// cannot win after a newer publication because every reconciliation is CAS
/// fenced and conflicts trigger a fresh scan.
fn resolve_relay_capabilities(
    publisher: &publish::ProjectionPublisher,
    catalog_refresh: Option<&BrokerCatalogRefresh>,
    scope_label: &str,
    forwarded_sources: &UsageRelayForwardedSourcesV1,
) -> UsageBrokerResponse {
    if forwarded_sources.profile_surface_ids.is_empty()
        && forwarded_sources.env_keys.is_empty()
        && forwarded_sources.credential_scope.sources.is_empty()
    {
        return UsageBrokerResponse::RelayCapabilities {
            resolution: Box::default(),
        };
    }
    let Some(catalog_refresh) = catalog_refresh else {
        return UsageBrokerResponse::Error {
            error: unavailable(),
        };
    };

    let forwarded_sources = ForwardedUsageSources::from(forwarded_sources.clone());
    match retry_catalog_revision_conflict(|| {
        catalog_refresh.with_discovery(|discovery| {
            let catalog_revision = discovery
                .config_generation
                .clone()
                .unwrap_or_else(|| "empty".to_owned());
            let entries = crate::usage_catalog_entries(&discovery);
            let diagnostics = crate::catalog_diagnostics::from_discovery(&discovery);
            let now = chrono::Utc::now().timestamp();
            let current_projection = publisher.current_projection()?;
            publisher.reconcile_catalog_if_projection_with_diagnostics(
                Some(&current_projection.projection_id),
                catalog_revision,
                entries,
                diagnostics,
                now,
            )?;
            {
                let capabilities =
                    forwarded_usage_capabilities(&discovery, scope_label, &forwarded_sources);
                let selected_accounts = forwarded_sources
                    .selected_account_surfaces
                    .iter()
                    .filter(|(account_id, _)| {
                        forwarded_sources.selected_account_ids.contains(*account_id)
                    })
                    .filter_map(|(account_id, surface_id)| {
                        let capability = usage_capability_for_selected_account_with_sources(
                            &discovery,
                            account_id,
                            surface_id,
                            Some(&forwarded_sources),
                        )?;
                        capabilities
                            .contains(&capability)
                            .then(|| UsageRelayCapabilityMappingV1 {
                                account_id: account_id.clone(),
                                surface_id: surface_id.clone(),
                                capability,
                            })
                    })
                    .collect();
                Ok(UsageBrokerResponse::RelayCapabilities {
                    resolution: Box::new(UsageRelayCapabilityResolutionV1 {
                        capabilities,
                        selected_accounts,
                    }),
                })
            }
        })
    }) {
        Ok(response) => response,
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

fn refresh_projection_with_catalog(
    coordinator: &UsageCoordinator,
    publisher: &publish::ProjectionPublisher,
    force: bool,
    catalog_refresh: Option<&BrokerCatalogRefresh>,
) -> UsageBrokerResponse {
    if force && let Some(catalog_refresh) = catalog_refresh {
        let reconciled = retry_catalog_revision_conflict(|| {
            catalog_refresh.with_discovery(|discovery| {
                let catalog_revision = discovery
                    .config_generation
                    .clone()
                    .unwrap_or_else(|| "empty".to_owned());
                let entries = crate::usage_catalog_entries(&discovery);
                let diagnostics = crate::catalog_diagnostics::from_discovery(&discovery);
                let now = chrono::Utc::now().timestamp();
                let current_projection = publisher.current_projection()?;
                publisher.reconcile_catalog_if_projection_with_diagnostics(
                    Some(&current_projection.projection_id),
                    catalog_revision,
                    entries,
                    diagnostics,
                    now,
                )?;
                Ok(())
            })
        });
        if let Err(error) = reconciled {
            return UsageBrokerResponse::Error { error };
        }
    }
    refresh_projection(coordinator, publisher, force)
}

/// Retry only catalog lease conflicts. The supplied attempt performs a fresh
/// broker-owned scan on every call; all other failures stay fail-closed.
pub(crate) fn retry_catalog_revision_conflict<T>(
    mut attempt: impl FnMut() -> Result<T, UsageCoordinationError>,
) -> Result<T, UsageCoordinationError> {
    let mut last_conflict = None;
    for _ in 0..BROKER_ACTIVATION_ATTEMPTS {
        match attempt() {
            Err(error) if error.kind == UsageCoordinationErrorKind::CatalogRevisionConflict => {
                last_conflict = Some(error);
            }
            result => return result,
        }
    }
    Err(last_conflict.unwrap_or_else(unavailable))
}

fn monitor_error(
    code: MonitorIssueCode,
    message: &str,
    retry_at_epoch: Option<i64>,
) -> UsageBrokerResponse {
    UsageBrokerResponse::MonitorError {
        issue: MonitorIssue {
            code,
            message: message.to_owned(),
            retry_at_epoch,
        },
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
