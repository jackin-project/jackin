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
mod tests {
    use super::{
        amp_profile_credential_payload, profile_credential_material_revision,
        profile_credential_source_identity,
    };
    use crate::Agent;
    use serde_json::json;
    use std::path::Path;

    fn revision_for_test(agent: Agent, raw: &[u8], message: &str) -> Option<String> {
        let result = profile_credential_material_revision(agent, raw);
        assert!(result.is_ok(), "{message}");
        let Ok(revision) = result else {
            return None;
        };
        Some(revision)
    }

    #[test]
    fn source_identity_binds_every_descriptor_field() {
        let selector = json!({"provider": "fixture", "nested": {"slot": 1}});
        let identity = profile_credential_source_identity(
            Agent::Hermes,
            "fixture",
            Path::new("/profiles/first"),
            Some(&selector),
        );
        for changed in [
            profile_credential_source_identity(
                Agent::Hermes,
                "fixture",
                Path::new("/profiles/second"),
                Some(&selector),
            ),
            profile_credential_source_identity(
                Agent::Hermes,
                "fixture ",
                Path::new("/profiles/first"),
                Some(&selector),
            ),
            profile_credential_source_identity(
                Agent::Hermes,
                "fixture",
                Path::new("/profiles/first"),
                Some(&json!({"provider": "fixture", "nested": {"slot": 2}})),
            ),
            profile_credential_source_identity(
                Agent::Omp,
                "fixture",
                Path::new("/profiles/first"),
                Some(&selector),
            ),
        ] {
            assert_ne!(
                identity.descriptor_fingerprint,
                changed.descriptor_fingerprint
            );
        }
        assert_eq!(identity.descriptor_fingerprint.len(), 64);
        assert!(
            identity
                .descriptor_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }

    #[test]
    fn json_format_and_object_order_are_semantically_equal() {
        let first = br#"{"token":"fixture", "nested":{"b":true,"a":[null,3]}}"#;
        let second = br#"{ "nested": {"a": [null, 3], "b": true}, "token": "fixture" }"#;
        let Some(first_revision) =
            revision_for_test(Agent::Hermes, first, "first fixture is valid JSON")
        else {
            return;
        };
        let Some(second_revision) =
            revision_for_test(Agent::Hermes, second, "second fixture is valid JSON")
        else {
            return;
        };
        assert_eq!(first_revision, second_revision);
    }

    #[test]
    fn selectors_preserve_structure_and_distinguish_absence() {
        let first = json!({"b": true, "a": [null, 3]});
        let second = json!({"a": [null, 3], "b": true});
        let identity = |selector| {
            profile_credential_source_identity(
                Agent::Hermes,
                "fixture",
                Path::new("/profiles/first"),
                selector,
            )
        };
        assert_eq!(identity(Some(&first)), identity(Some(&second)));
        assert_ne!(identity(None), identity(Some(&serde_json::Value::Null)));
        assert_ne!(
            identity(Some(&first)),
            identity(Some(&json!({"a": [3, null], "b": true})))
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_identity_preserves_non_utf8_bytes() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let identity = |directory| {
            profile_credential_source_identity(Agent::Hermes, "fixture", directory, None)
        };
        assert_ne!(
            identity(Path::new(OsStr::from_bytes(b"/profiles/\xff"))),
            identity(Path::new(OsStr::from_bytes(b"/profiles/\xfe")))
        );
    }

    #[test]
    fn exact_strings_and_array_order_change_revision() {
        let Some(base) = revision_for_test(
            Agent::Hermes,
            br#"{"token":"fixture","slots":[1,2]}"#,
            "base fixture is valid JSON",
        ) else {
            return;
        };
        for raw in [
            br#"{"token":" fixture","slots":[1,2]}"#.as_slice(),
            br#"{"token":"fixture ","slots":[1,2]}"#.as_slice(),
            br#"{"token":"fixture","slots":[2,1]}"#.as_slice(),
        ] {
            let Some(changed_revision) =
                revision_for_test(Agent::Hermes, raw, "changed fixture is valid JSON")
            else {
                return;
            };
            assert_ne!(base, changed_revision);
        }
    }

    #[test]
    fn payload_agent_domains_and_binary_contents_are_distinct() {
        let Some(hermes_json_revision) =
            revision_for_test(Agent::Hermes, b"null", "null is valid JSON for Hermes")
        else {
            return;
        };
        let Some(claude_json_revision) =
            revision_for_test(Agent::Claude, b"null", "null is valid JSON for Claude")
        else {
            return;
        };
        assert_ne!(hermes_json_revision, claude_json_revision);

        let Some(omp_first_revision) =
            revision_for_test(Agent::Omp, &[0, 255], "Omp accepts opaque binary payloads")
        else {
            return;
        };
        let Some(omp_second_revision) =
            revision_for_test(Agent::Omp, &[0, 254], "Omp accepts opaque binary payloads")
        else {
            return;
        };
        assert_ne!(omp_first_revision, omp_second_revision);

        let Some(omp_json_revision) =
            revision_for_test(Agent::Omp, b"null", "Omp accepts opaque binary payloads")
        else {
            return;
        };
        assert_ne!(omp_json_revision, hermes_json_revision);
    }

    #[test]
    fn json_rejects_invalid_utf8_and_trailing_input() {
        for raw in [b"\"\xff\"".as_slice(), b"null false".as_slice()] {
            let result = profile_credential_material_revision(Agent::Hermes, raw);
            assert!(result.err().is_some());
        }
    }

    #[test]
    fn amp_payload_selects_canonical_server_and_preserves_exact_token() {
        let input = json!({
            "apiKey@https://foreign.example/": "foreign-fixture",
            "mcp-oauth@https://ampcode.com/": "mcp-fixture",
            "apiKey@https://ampcode.com": " canonical-fixture "
        });
        assert_eq!(
            amp_profile_credential_payload(&input),
            Ok(json!({"apiKey@https://ampcode.com/": " canonical-fixture "}))
        );
    }

    #[test]
    fn amp_payload_rejects_foreign_only_dual_aliases_and_unusable_tokens() {
        for input in [
            json!({"apiKey@https://foreign.example/": "foreign-fixture"}),
            json!({"mcp-oauth@https://ampcode.com/": "mcp-fixture"}),
            json!({
                "apiKey@https://ampcode.com/": "fixture",
                "apiKey@https://ampcode.com": "fixture"
            }),
            json!({"apiKey@https://ampcode.com/": " \t "}),
            json!({"apiKey@https://ampcode.com/": 1}),
        ] {
            assert!(amp_profile_credential_payload(&input).err().is_some());
        }
    }

    #[test]
    fn amp_revision_binds_only_exact_canonical_token() {
        let first = br#"{"apiKey@https://ampcode.com/":"fixture","apiKey@https://foreign.example/":"foreign-one","mcp-oauth@https://ampcode.com/":"mcp-one"}"#;
        let rotated = br#"{"apiKey@https://ampcode.com":"fixture","apiKey@https://foreign.example/":"foreign-two","mcp-oauth@https://ampcode.com/":"mcp-two"}"#;
        let changed = br#"{"apiKey@https://ampcode.com/":"fixture "}"#;
        let Some(first_revision) =
            revision_for_test(Agent::Amp, first, "first Amp fixture has a canonical token")
        else {
            return;
        };
        let Some(rotated_revision) = revision_for_test(
            Agent::Amp,
            rotated,
            "rotated Amp fixture has a canonical token",
        ) else {
            return;
        };
        assert_eq!(first_revision, rotated_revision);

        let Some(changed_revision) = revision_for_test(
            Agent::Amp,
            changed,
            "changed Amp fixture has a canonical token",
        ) else {
            return;
        };
        assert_ne!(first_revision, changed_revision);
    }
}
