// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{FieldSet, Rejection, emit_event, event};

pub fn record_retry_scheduled() -> Result<(), Rejection> {
    emit_event(&event::RETRY_SCHEDULED, FieldSet::default())
}

#[cfg(test)]
mod tests;
