// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe retry scheduling at bounded attempt boundaries.

use crate::{FieldSet, Rejection, emit_event, event};

/// Record that another bounded attempt will follow an unsuccessful attempt.
///
/// The fixed event name carries the semantic meaning. Dynamic delay, attempt,
/// target, and error values stay out of telemetry.
pub fn record_retry_scheduled() -> Result<(), Rejection> {
    emit_event(&event::RETRY_SCHEDULED, FieldSet::default())
}

#[cfg(test)]
mod tests;
