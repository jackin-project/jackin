// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Grant validation and capability normalization.

use super::{DockerGrants, GrantValidationError, VALID_CAPABILITIES, parse_size_field};

/// Validate explicit grants, returning all errors found (not just the first).
///
/// Called at launch time before any container is started. A non-empty error
/// list aborts the launch with clear, actionable messages.
pub fn validate_grants(grants: &DockerGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();

    // user = "root" + sudo = true is mutually exclusive.
    if grants.user.as_deref() == Some("root") && grants.sudo == Some(true) {
        errors.push(GrantValidationError::RootAndSudo);
    }

    // Validate capability names — reuse normalize_cap to strip CAP_ prefix.
    for cap in &grants.capabilities_add {
        let normalized = normalize_cap(cap);
        if !VALID_CAPABILITIES.contains(&normalized.as_str()) {
            errors.push(GrantValidationError::UnknownCapability(cap.clone()));
        }
    }

    // Parse + range-check the two memory size fields (identical rules: a parse
    // failure records UnparsableSize; a value over i64::MAX records ValueOutOfRange
    // for the Bollard/Docker API boundary). Returns the parsed value either way so
    // the reservation-vs-memory comparison below still runs.
    let memory_bytes = parse_size_field(&mut errors, "memory", grants.memory.as_deref());
    let reservation_bytes = parse_size_field(
        &mut errors,
        "memory_reservation",
        grants.memory_reservation.as_deref(),
    );

    if let (Some(res), Some(mem)) = (reservation_bytes, memory_bytes)
        && res > mem
    {
        errors.push(GrantValidationError::MemoryReservationExceedsMemory {
            reservation: res,
            memory: mem,
        });
    }

    // pids must be positive. Docker uses -1 as "unlimited", but that would
    // disable the limit that hardened/locked profiles are designed to enforce.
    if let Some(pids) = grants.pids
        && pids <= 0
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "pids",
            reason: "must be > 0; omit the field to remove the limit",
        });
    }

    // cpus must be finite and positive. A non-finite (NaN/inf) or non-positive
    // value survives raise_to_max (NaN fails every `>=` compare, so it is kept)
    // and reaches `--cpus <value>`, failing opaquely at `docker run` instead of
    // at this launch-time gate.
    if let Some(cpus) = grants.cpus
        && (!cpus.is_finite() || cpus <= 0.0)
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "cpus",
            reason: "must be a finite value > 0; omit the field to remove the limit",
        });
    }

    // nofile = 0 emits `--ulimit nofile=0:0`, forbidding the container from
    // opening any file descriptor — a launch that cannot function. Reject it.
    if grants.nofile == Some(0) {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "nofile",
            reason: "must be > 0; omit the field to remove the limit",
        });
    }

    errors
}

/// Normalize a capability name to uppercase without `CAP_` prefix.
pub(crate) fn normalize_cap(cap: &str) -> String {
    let upper = cap.to_ascii_uppercase();
    upper.strip_prefix("CAP_").unwrap_or(&upper).to_owned()
}
