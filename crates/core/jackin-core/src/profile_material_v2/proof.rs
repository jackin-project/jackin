// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::{fmt, fmt::Debug};

use serde::de::Error as DeError;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::Agent;

use super::digest::ProfileCredentialFingerprintV2;
use super::error::ProfileCredentialV2Error;
use super::hash::{finish_fingerprint, hash_amp_payload, hash_frame, hash_json_node};
use super::json::{JsonNode, parse_json};

const SOURCE_DOMAIN_V2: &[u8] = b"jackin.profile-credential-source.v2";
const MATERIAL_DOMAIN_V2: &[u8] = b"jackin.profile-credential-material.v2";
const AMP_CANONICAL_KEY: &str = "apiKey@https://ampcode.com/";
const AMP_ALIAS_KEY: &str = "apiKey@https://ampcode.com";

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
enum SourceIdentitySchemeV2 {
    #[serde(rename = "jackin.profile-credential-source.v2")]
    V2,
}

pub struct ProfileCredentialSourceIdentityV2 {
    agent: Agent,
    descriptor_fingerprint: ProfileCredentialFingerprintV2,
    scheme: SourceIdentitySchemeV2,
}

impl Debug for ProfileCredentialSourceIdentityV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileCredentialSourceIdentityV2")
            .field("agent", &self.agent)
            .field("descriptor_fingerprint", &self.descriptor_fingerprint)
            .finish_non_exhaustive()
    }
}

impl ProfileCredentialSourceIdentityV2 {
    pub fn agent(&self) -> Agent {
        self.agent
    }

    pub fn descriptor_fingerprint(&self) -> &ProfileCredentialFingerprintV2 {
        &self.descriptor_fingerprint
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIdentityWire {
    scheme: SourceIdentitySchemeV2,
    agent: Agent,
    descriptor_fingerprint: ProfileCredentialFingerprintV2,
}

impl Serialize for ProfileCredentialSourceIdentityV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ProfileCredentialSourceIdentityV2", 3)?;
        state.serialize_field("scheme", &self.scheme)?;
        state.serialize_field("agent", &self.agent)?;
        state.serialize_field("descriptor_fingerprint", &self.descriptor_fingerprint)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ProfileCredentialSourceIdentityV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SourceIdentityWire::deserialize(deserializer)
            .map_err(|_| D::Error::custom("invalid profile credential source identity"))?;
        Ok(Self {
            agent: wire.agent,
            descriptor_fingerprint: wire.descriptor_fingerprint,
            scheme: wire.scheme,
        })
    }
}

pub fn profile_credential_source_identity_v2(
    agent: Agent,
    provider_slug: &str,
    effective_directory: &Path,
    selector_json: Option<&[u8]>,
) -> Result<ProfileCredentialSourceIdentityV2, ProfileCredentialV2Error> {
    let selector = selector_json.map(parse_json).transpose()?;

    let mut hash = Sha256::new();
    hash_frame(&mut hash, SOURCE_DOMAIN_V2)?;
    hash_frame(&mut hash, agent.slug().as_bytes())?;
    hash_frame(&mut hash, provider_slug.as_bytes())?;
    hash_frame(
        &mut hash,
        effective_directory.as_os_str().as_encoded_bytes(),
    )?;
    match selector.as_ref() {
        None => hash.update([0]),
        Some(node) => {
            hash.update([1]);
            hash_json_node(&mut hash, node)?;
        }
    }

    Ok(ProfileCredentialSourceIdentityV2 {
        agent,
        descriptor_fingerprint: finish_fingerprint(hash)?,
        scheme: SourceIdentitySchemeV2::V2,
    })
}

pub fn profile_credential_material_revision_v2(
    agent: Agent,
    raw: &[u8],
) -> Result<ProfileCredentialFingerprintV2, ProfileCredentialV2Error> {
    if agent == Agent::Omp {
        if raw.len() > super::json::MAX_INPUT_BYTES {
            return Err(ProfileCredentialV2Error::InputTooLarge);
        }
        let mut hash = Sha256::new();
        hash_frame(&mut hash, MATERIAL_DOMAIN_V2)?;
        hash_frame(&mut hash, agent.slug().as_bytes())?;
        hash_frame(&mut hash, b"binary")?;
        hash_frame(&mut hash, raw)?;
        return finish_fingerprint(hash);
    }

    let node = parse_json(raw)?;
    let mut hash = Sha256::new();
    hash_frame(&mut hash, MATERIAL_DOMAIN_V2)?;
    hash_frame(&mut hash, agent.slug().as_bytes())?;
    hash_frame(&mut hash, b"json")?;
    if agent == Agent::Amp {
        hash_amp_payload(&mut hash, amp_token(&node)?)?;
    } else {
        hash_json_node(&mut hash, &node)?;
    }
    finish_fingerprint(hash)
}

fn amp_token<'node>(node: &'node JsonNode<'_>) -> Result<&'node str, ProfileCredentialV2Error> {
    let JsonNode::Object(entries) = node else {
        return Err(ProfileCredentialV2Error::InvalidAmpCredential);
    };
    let canonical = object_get(entries, AMP_CANONICAL_KEY);
    let alias = object_get(entries, AMP_ALIAS_KEY);
    let value = match (canonical, alias) {
        (Some(_), Some(_)) => return Err(ProfileCredentialV2Error::InvalidAmpCredential),
        (None, None) => return Err(ProfileCredentialV2Error::InvalidAmpCredential),
        (Some(value), None) | (None, Some(value)) => value,
    };
    let JsonNode::String(token) = value else {
        return Err(ProfileCredentialV2Error::InvalidAmpCredential);
    };
    if token.trim().is_empty() {
        return Err(ProfileCredentialV2Error::InvalidAmpCredential);
    }
    Ok(token)
}

fn object_get<'entry, 'raw>(
    entries: &'entry [(String, JsonNode<'raw>)],
    key: &str,
) -> Option<&'entry JsonNode<'raw>> {
    entries
        .binary_search_by(|(entry_key, _)| entry_key.as_str().cmp(key))
        .ok()
        .and_then(|index| entries.get(index).map(|(_, value)| value))
}
