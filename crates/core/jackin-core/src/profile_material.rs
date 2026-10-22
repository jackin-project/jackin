// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::{collections::HashSet, fmt};

use serde::de::{DeserializeSeed, Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::Agent;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCredentialSourceIdentity {
    pub agent: Agent,
    #[serde(deserialize_with = "deserialize_sha256_fingerprint")]
    pub descriptor_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCredentialSourceMaterial {
    pub source: ProfileCredentialSourceIdentity,
    #[serde(deserialize_with = "deserialize_sha256_fingerprint")]
    pub material_revision: String,
}

fn deserialize_sha256_fingerprint<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let fingerprint = String::deserialize(deserializer)?;
    if is_sha256_fingerprint(&fingerprint) {
        Ok(fingerprint)
    } else {
        Err(D::Error::custom(
            "credential fingerprint must be 64 lowercase hexadecimal characters",
        ))
    }
}

fn is_sha256_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

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

/// Compute a profile credential's v1 semantic revision.
///
/// Omp payloads remain opaque bytes. Other agents require JSON with unique
/// decoded object keys. The JSON parser keeps its existing recursion limit,
/// and accepted JSON retains the existing v1 normalization and hash encoding.
///
/// # Errors
///
/// Returns a JSON error for invalid JSON or an Amp payload without one
/// unambiguous canonical credential.
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
        validate_profile_json(raw)?;
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

fn validate_profile_json(raw: &[u8]) -> Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(raw);
    UniqueJsonSeed.deserialize(&mut deserializer)?;
    deserializer.end()
}

struct UniqueJsonSeed;

impl<'de> DeserializeSeed<'de> for UniqueJsonSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value with unique object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(UniqueJsonSeed)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(A::Error::custom(
                    "profile credential JSON contains duplicate object keys",
                ));
            }
            map.next_value_seed(UniqueJsonSeed)?;
        }
        Ok(())
    }
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
