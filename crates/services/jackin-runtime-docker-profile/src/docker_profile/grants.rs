// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DockerGrants, GrantValidationError, VALID_CAPABILITIES, parse_size_field};

pub fn validate_grants(grants: &DockerGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();

    if grants.user.as_deref() == Some("root") && grants.sudo == Some(true) {
        errors.push(GrantValidationError::RootAndSudo);
    }

    for cap in &grants.capabilities_add {
        let normalized = normalize_cap(cap);
        if !VALID_CAPABILITIES.contains(&normalized.as_str()) {
            errors.push(GrantValidationError::UnknownCapability(cap.clone()));
        }
    }

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

    if let Some(pids) = grants.pids
        && pids <= 0
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "pids",
            reason: "must be > 0; omit the field to remove the limit",
        });
    }

    if let Some(cpus) = grants.cpus
        && (!cpus.is_finite() || cpus <= 0.0)
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "cpus",
            reason: "must be a finite value > 0; omit the field to remove the limit",
        });
    }

    if grants.nofile == Some(0) {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "nofile",
            reason: "must be > 0; omit the field to remove the limit",
        });
    }

    errors
}

pub(crate) fn normalize_cap(cap: &str) -> String {
    let upper = cap.to_ascii_uppercase();
    upper.strip_prefix("CAP_").unwrap_or(&upper).to_owned()
}
