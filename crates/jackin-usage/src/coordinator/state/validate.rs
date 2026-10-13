// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Envelope validation.

use std::fs::File;

use std::os::unix::fs::MetadataExt as _;

use jackin_protocol::usage_broker::UsageAccountCapability;

use nix::unistd::geteuid;

use super::{
    ACCOUNT_STATE_SCHEMA_VERSION, AccountStateEnvelope, MAX_CLOCK_SKEW_SECS, StateStoreError,
    sanitize_text, sanitize_usage_view,
};

pub(crate) fn validate_owned_mode(file: &File, expected: u32) -> Result<(), StateStoreError> {
    let metadata = file.metadata().map_err(|_| StateStoreError::Unavailable)?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o777 != expected {
        return Err(StateStoreError::Unavailable);
    }
    Ok(())
}

pub(crate) fn validate_capability(
    capability: &UsageAccountCapability,
) -> Result<(), StateStoreError> {
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
    };
    if !valid(&capability.account_id) || !valid(&capability.surface_id) {
        return Err(StateStoreError::Corrupt);
    }
    Ok(())
}

pub(crate) fn state_filename(capability: &UsageAccountCapability) -> String {
    format!("{}-{}.json", capability.surface_id, capability.account_id)
}

pub(crate) fn validate_envelope(
    envelope: AccountStateEnvelope,
    expected: &UsageAccountCapability,
    now_epoch: i64,
) -> Result<AccountStateEnvelope, StateStoreError> {
    if envelope.schema_version != ACCOUNT_STATE_SCHEMA_VERSION || &envelope.capability != expected {
        return Err(StateStoreError::Corrupt);
    }
    let future_limit = now_epoch.saturating_add(MAX_CLOCK_SKEW_SECS);
    if [
        envelope.provider_invoked_at_epoch,
        envelope.started_at_epoch,
        envelope.completed_at_epoch,
        envelope
            .terminal_result
            .as_ref()
            .map(|view| view.fetched_at_epoch),
        envelope
            .last_good
            .as_ref()
            .map(|view| view.fetched_at_epoch),
    ]
    .into_iter()
    .flatten()
    .any(|timestamp| timestamp > future_limit)
    {
        return Err(StateStoreError::Corrupt);
    }
    Ok(sanitize_envelope(envelope))
}

pub(crate) fn sanitize_envelope(mut envelope: AccountStateEnvelope) -> AccountStateEnvelope {
    envelope.terminal_result = envelope.terminal_result.map(sanitize_usage_view);
    envelope.last_good = envelope.last_good.map(sanitize_usage_view);
    if let Some(error) = &mut envelope.terminal_error {
        error.message = sanitize_text(&error.message);
    }
    envelope
}
