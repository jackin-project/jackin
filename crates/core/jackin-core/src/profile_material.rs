// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure identities and semantic revisions for profile credential proofs.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::Agent;

/// Opaque identity of a profile credential source, safe to carry in a proof.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCredentialSourceIdentity {
    /// Agent whose profile owns the credential source.
    pub agent: Agent,
    /// Domain-separated SHA-256 of the complete source descriptor.
    pub descriptor_fingerprint: String,
}

/// A secret-free proof binding a profile source to its credential revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCredentialSourceMaterial {
    /// Opaque identity of the profile credential source.
    pub source: ProfileCredentialSourceIdentity,
    /// Semantic SHA-256 revision of the source's credential payload.
    pub material_revision: String,
}

/// Select Amp's canonical server credential and discard unrelated credentials.
///
/// Accepts exactly one of the canonical server's slash and non-slash keys.
/// The returned object always uses the slash key and retains the exact token.
/// Foreign server and MCP OAuth entries never become Amp profile credentials.
///
/// # Errors
/// Rejects absent, ambiguous, non-string, or blank canonical credentials with
/// static messages that contain no credential contents.
pub fn amp_profile_credential_payload(value: &Value) -> Result<Value, &'static str> {
    const CANONICAL: &str = "apiKey@https://ampcode.com/";
    const ALIAS: &str = "apiKey@https://ampcode.com";

    let object = value
        .as_object()
        .ok_or("Amp profile credential payload is not an object")?;
    let token = match (object.get(CANONICAL), object.get(ALIAS)) {
        (Some(_), Some(_)) => return Err("Amp profile canonical credential is ambiguous"),
        (None, None) => return Err("Amp profile canonical credential is missing"),
        (Some(value), None) | (None, Some(value)) => value
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .ok_or("Amp profile canonical credential is not a usable string")?,
    };
    let mut payload = serde_json::Map::new();
    payload.insert(CANONICAL.to_owned(), Value::String(token.to_owned()));
    Ok(Value::Object(payload))
}

/// Identify a provider, effective directory, and optional full profile selector.
///
/// Directory bytes and exact provider and selector strings are hashed without
/// normalization. Object keys are sorted; array order is preserved. The result
/// carries neither the directory nor selector contents.
pub fn profile_credential_source_identity(
    agent: Agent,
    provider_slug: &str,
    effective_directory: &Path,
    selector: Option<&Value>,
) -> ProfileCredentialSourceIdentity {
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"jackin.profile-credential-source.v1");
    hash_field(&mut hash, agent.slug().as_bytes());
    hash_field(&mut hash, provider_slug.as_bytes());
    hash_field(
        &mut hash,
        effective_directory.as_os_str().as_encoded_bytes(),
    );
    match selector {
        None => hash.update([0]),
        Some(value) => {
            hash.update([1]);
            hash_json(&mut hash, value);
        }
    }
    ProfileCredentialSourceIdentity {
        agent,
        descriptor_fingerprint: hex::encode(hash.finalize()),
    }
}

/// Compute the semantic revision of a profile credential payload.
///
/// Omp payloads are opaque binary. All other agents require strict JSON:
/// formatting and object key order do not affect the revision, while exact
/// string contents, array order, and JSON value types do. Agent and payload
/// encoding domains are included in the hash. Amp revisions bind only its
/// unambiguous canonical server credential, excluding foreign and MCP entries.
///
/// # Errors
/// Returns a JSON parse error for invalid JSON or invalid UTF-8 in JSON payloads,
/// or an invalid-data error when Amp has no unambiguous canonical credential.
pub fn profile_credential_material_revision(
    agent: Agent,
    raw: &[u8],
) -> Result<String, serde_json::Error> {
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"jackin.profile-credential-material.v1");
    hash_field(&mut hash, agent.slug().as_bytes());
    if agent == Agent::Omp {
        hash_field(&mut hash, b"binary");
        hash_field(&mut hash, raw);
    } else {
        let value: Value = serde_json::from_slice(raw)?;
        let value = if agent == Agent::Amp {
            amp_profile_credential_payload(&value).map_err(|message| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    message,
                ))
            })?
        } else {
            value
        };
        hash_field(&mut hash, b"json");
        hash_json(&mut hash, &value);
    }
    Ok(hex::encode(hash.finalize()))
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

fn hash_json(hash: &mut Sha256, value: &Value) {
    match value {
        Value::Null => hash.update([0]),
        Value::Bool(value) => hash.update([1, u8::from(*value)]),
        Value::Number(value) => {
            hash.update([2]);
            hash_field(hash, value.to_string().as_bytes());
        }
        Value::String(value) => {
            hash.update([3]);
            hash_field(hash, value.as_bytes());
        }
        Value::Array(values) => {
            hash.update([4]);
            hash.update((values.len() as u64).to_be_bytes());
            for value in values {
                hash_json(hash, value);
            }
        }
        Value::Object(values) => {
            hash.update([5]);
            hash.update((values.len() as u64).to_be_bytes());
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by_key(|(key, _)| *key);
            for (key, value) in entries {
                hash_field(hash, key.as_bytes());
                hash_json(hash, value);
            }
        }
    }
}

#[cfg(test)]
mod tests;
