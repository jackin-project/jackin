// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Atomic host-only refresh-generation persistence.

mod envelope;
mod sanitize;
mod stores;
mod validate;

#[cfg(test)]
use nix::unistd::geteuid;
#[cfg(test)]
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

pub(crate) use envelope::{
    ACCOUNT_STATE_SCHEMA_VERSION, LEGACY_ACCOUNT_STATE_SCHEMA_VERSION, MAX_ACCOUNT_STATE_BYTES,
    MAX_CLOCK_SKEW_SECS, MAX_DISPLAY_CHARS, PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION,
    PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION, PROJECTION_STATE_SCHEMA_VERSION,
    STATE_QUARANTINE_COUNTER, STATE_TMP_COUNTER,
};
pub use envelope::{AccountStateEnvelope, StateStoreError};
pub(crate) use sanitize::sanitize_text;
pub(crate) use sanitize::sanitize_usage_view;
pub use stores::{
    AccountStateStore, FileAccountStateStore, FileProjectionStateStore, ProjectionAlias,
    ProjectionStateEnvelope,
};
pub(crate) use validate::{
    sanitize_envelope, state_filename, validate_capability, validate_envelope, validate_owned_mode,
};

#[cfg(test)]
mod tests;
