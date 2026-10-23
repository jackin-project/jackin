// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use sha2::{Digest, Sha256};

use super::error::ProfileCredentialV2Error;
use super::json::JsonNode;

pub(super) fn hash_frame(hash: &mut Sha256, bytes: &[u8]) -> Result<(), ProfileCredentialV2Error> {
    hash_length(hash, bytes.len())?;
    hash.update(bytes);
    Ok(())
}

pub(super) fn hash_length(
    hash: &mut Sha256,
    length: usize,
) -> Result<(), ProfileCredentialV2Error> {
    let length = u64::try_from(length).map_err(|_| ProfileCredentialV2Error::InvalidProof)?;
    hash.update(length.to_be_bytes());
    Ok(())
}

pub(super) fn hash_json_node(
    hash: &mut Sha256,
    node: &JsonNode<'_>,
) -> Result<(), ProfileCredentialV2Error> {
    match node {
        JsonNode::Null => hash.update([0]),
        JsonNode::Bool(value) => hash.update([1, u8::from(*value)]),
        JsonNode::Number(token) => {
            hash.update([2]);
            hash_frame(hash, token)?;
        }
        JsonNode::String(value) => {
            hash.update([3]);
            hash_frame(hash, value.as_bytes())?;
        }
        JsonNode::Array(values) => {
            hash.update([4]);
            hash_length(hash, values.len())?;
            for value in values {
                hash_json_node(hash, value)?;
            }
        }
        JsonNode::Object(entries) => {
            hash.update([5]);
            hash_length(hash, entries.len())?;
            for (key, value) in entries {
                hash_frame(hash, key.as_bytes())?;
                hash_json_node(hash, value)?;
            }
        }
    }
    Ok(())
}

pub(super) fn hash_amp_payload(
    hash: &mut Sha256,
    token: &str,
) -> Result<(), ProfileCredentialV2Error> {
    hash.update([5]);
    hash_length(hash, 1)?;
    hash_frame(hash, b"apiKey@https://ampcode.com/")?;
    hash.update([3]);
    hash_frame(hash, token.as_bytes())
}

pub(super) fn finish_fingerprint(
    hash: Sha256,
) -> Result<super::digest::ProfileCredentialFingerprintV2, ProfileCredentialV2Error> {
    let encoded = hex::encode(hash.finalize());
    super::digest::ProfileCredentialFingerprintV2::parse_hex(&encoded)
}
