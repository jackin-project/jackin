// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::{fmt, fmt::Debug};

use serde::de::value::MapAccessDeserializer;
use serde::de::{Error as DeError, MapAccess, Visitor};
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

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
enum SourceIdentitySchemeV2 {
    #[serde(rename = "jackin.profile-credential-source.v2")]
    V2,
}

impl<'de> Deserialize<'de> for SourceIdentitySchemeV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SchemeVisitor;

        impl<'de> Visitor<'de> for SchemeVisitor {
            type Value = SourceIdentitySchemeV2;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("the exact source identity scheme string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if value == "jackin.profile-credential-source.v2" {
                    Ok(SourceIdentitySchemeV2::V2)
                } else {
                    Err(E::custom("invalid profile credential source scheme"))
                }
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_str(value)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_str(SchemeVisitor)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
enum SourceMaterialSchemeV2 {
    #[serde(rename = "jackin.profile-credential-material.v2")]
    V2,
}

impl<'de> Deserialize<'de> for SourceMaterialSchemeV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SchemeVisitor;

        impl<'de> Visitor<'de> for SchemeVisitor {
            type Value = SourceMaterialSchemeV2;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("the exact source material scheme string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if value == "jackin.profile-credential-material.v2" {
                    Ok(SourceMaterialSchemeV2::V2)
                } else {
                    Err(E::custom("invalid profile credential material scheme"))
                }
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_str(value)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_str(SchemeVisitor)
    }
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

struct SourceIdentityWire {
    scheme: SourceIdentitySchemeV2,
    agent: Agent,
    descriptor_fingerprint: ProfileCredentialFingerprintV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIdentityWireFields {
    scheme: SourceIdentitySchemeV2,
    agent: Agent,
    descriptor_fingerprint: ProfileCredentialFingerprintV2,
}

impl<'de> Deserialize<'de> for SourceIdentityWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WireVisitor;

        impl<'de> Visitor<'de> for WireVisitor {
            type Value = SourceIdentityWire;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a tagged source identity object")
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let fields =
                    SourceIdentityWireFields::deserialize(MapAccessDeserializer::new(map))?;
                Ok(SourceIdentityWire {
                    scheme: fields.scheme,
                    agent: fields.agent,
                    descriptor_fingerprint: fields.descriptor_fingerprint,
                })
            }
        }

        deserializer.deserialize_map(WireVisitor)
    }
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

pub struct ProfileCredentialSourceMaterialV2 {
    source: ProfileCredentialSourceIdentityV2,
    material_revision: ProfileCredentialFingerprintV2,
    scheme: SourceMaterialSchemeV2,
}

impl Debug for ProfileCredentialSourceMaterialV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileCredentialSourceMaterialV2")
            .field("source", &self.source)
            .field("material_revision", &self.material_revision)
            .finish_non_exhaustive()
    }
}

impl ProfileCredentialSourceMaterialV2 {
    pub fn source(&self) -> &ProfileCredentialSourceIdentityV2 {
        &self.source
    }

    pub fn material_revision(&self) -> &ProfileCredentialFingerprintV2 {
        &self.material_revision
    }
}

struct SourceMaterialWireV2 {
    scheme: SourceMaterialSchemeV2,
    source: ProfileCredentialSourceIdentityV2,
    material_revision: ProfileCredentialFingerprintV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMaterialWireFieldsV2 {
    scheme: SourceMaterialSchemeV2,
    source: ProfileCredentialSourceIdentityV2,
    material_revision: ProfileCredentialFingerprintV2,
}

impl<'de> Deserialize<'de> for SourceMaterialWireV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WireVisitor;

        impl<'de> Visitor<'de> for WireVisitor {
            type Value = SourceMaterialWireV2;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a tagged source material object")
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let fields =
                    SourceMaterialWireFieldsV2::deserialize(MapAccessDeserializer::new(map))?;
                Ok(SourceMaterialWireV2 {
                    scheme: fields.scheme,
                    source: fields.source,
                    material_revision: fields.material_revision,
                })
            }
        }

        deserializer.deserialize_map(WireVisitor)
    }
}

impl Serialize for ProfileCredentialSourceMaterialV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ProfileCredentialSourceMaterialV2", 3)?;
        state.serialize_field("scheme", &self.scheme)?;
        state.serialize_field("source", &self.source)?;
        state.serialize_field("material_revision", &self.material_revision)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ProfileCredentialSourceMaterialV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SourceMaterialWireV2::deserialize(deserializer)
            .map_err(|_| D::Error::custom("invalid profile credential source material"))?;
        Ok(Self {
            source: wire.source,
            material_revision: wire.material_revision,
            scheme: wire.scheme,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileCredentialProofComparison {
    Equal,
    Different,
}

pub fn profile_credential_source_material_v2(
    source: ProfileCredentialSourceIdentityV2,
    raw: &[u8],
) -> Result<ProfileCredentialSourceMaterialV2, ProfileCredentialV2Error> {
    let material_revision = profile_credential_material_revision_v2(source.agent(), raw)?;
    Ok(ProfileCredentialSourceMaterialV2 {
        source,
        material_revision,
        scheme: SourceMaterialSchemeV2::V2,
    })
}

pub fn compare_profile_credential_material_proofs(
    left: &ProfileCredentialSourceMaterialV2,
    right: &ProfileCredentialSourceMaterialV2,
) -> ProfileCredentialProofComparison {
    if same_v2_proof(left, right) {
        ProfileCredentialProofComparison::Equal
    } else {
        ProfileCredentialProofComparison::Different
    }
}

fn same_v2_proof(
    left: &ProfileCredentialSourceMaterialV2,
    right: &ProfileCredentialSourceMaterialV2,
) -> bool {
    left.scheme == right.scheme
        && left.material_revision == right.material_revision
        && left.source.scheme == right.source.scheme
        && left.source.agent == right.source.agent
        && left.source.descriptor_fingerprint == right.source.descriptor_fingerprint
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
