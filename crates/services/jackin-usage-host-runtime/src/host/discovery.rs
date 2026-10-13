// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Rust-owned host account-source discovery.
//!
//! Account-registry authority, path roots, provider ownership, and deduplication stay in
//! this crate. Native clients receive only sanitized descriptors/diagnostics.

mod stage;

pub(crate) use jackin_usage_discovery::ValidatedCredentialBinding;
#[cfg(test)]
pub(crate) use jackin_usage_discovery::ValidatedCredentialSource;
