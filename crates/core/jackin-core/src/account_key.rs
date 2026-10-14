// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use sha2::{Digest, Sha256};

pub fn account_key_hash(provider: &str, account_label: &str) -> String {
    let digest = Sha256::digest(format!("{provider}\0{account_label}").as_bytes());
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests;
