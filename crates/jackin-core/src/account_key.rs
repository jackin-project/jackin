// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical account-key hashing shared by the host CLI and the in-capsule
//! telemetry store. Both sides derive the same `account_key_hash` from a
//! `(provider, account_label)` pair so host- and container-recorded usage
//! rows correlate; a single definition keeps them from silently drifting.

use sha2::{Digest, Sha256};

/// Stable, opaque key correlating usage rows for a provider account across the
/// host and the container. Format: `sha256:<lowercase hex>` over
/// `"{provider}\0{account_label}"`.
pub fn account_key_hash(provider: &str, account_label: &str) -> String {
    let digest = Sha256::digest(format!("{provider}\0{account_label}").as_bytes());
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests;
