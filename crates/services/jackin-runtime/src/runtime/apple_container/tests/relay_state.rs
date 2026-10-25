// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;
use uuid::Uuid;

const BACKEND: &str = "apple-container";
const RECORD_MAX_BYTES: usize = 1024 * 1024;
const RECORD_KEY_PREFIX: &str = "apple-usage-relay-";
const RECORD_KEY_DOMAIN: &[u8] = b"jackin-apple-usage-relay-state-leaf-v1\0";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AppleUsageRelayRecordV1 {
    schema_version: u32,
    backend: String,
    container_id: String,
    launch_generation: String,
    credential_scope: UsageCredentialScope,
    capabilities: Vec<UsageAccountCapability>,
}

impl AppleUsageRelayRecordV1 {
    fn new(
        container_id: impl Into<String>,
        launch_generation: impl Into<String>,
        credential_scope: &UsageCredentialScope,
        capabilities: &[UsageAccountCapability],
    ) -> Result<Self, RelayStateError> {
        let record = Self {
            schema_version: 1,
            backend: BACKEND.to_owned(),
            container_id: container_id.into(),
            launch_generation: launch_generation.into(),
            credential_scope: credential_scope.clone(),
            capabilities: capabilities.to_vec(),
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), RelayStateError> {
        if self.schema_version != 1 {
            return Err(RelayStateError::Invalid(
                "unsupported record schema version",
            ));
        }
        if self.backend != BACKEND {
            return Err(RelayStateError::Invalid("record backend does not match"));
        }
        if self.container_id.is_empty() {
            return Err(RelayStateError::Invalid("container identity is empty"));
        }
        canonical_generation(&self.launch_generation)?;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, RelayStateError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > RECORD_MAX_BYTES {
            return Err(RelayStateError::Invalid(
                "serialized record exceeds the configured size limit",
            ));
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, RelayStateError> {
        if bytes.len() > RECORD_MAX_BYTES {
            return Err(RelayStateError::Invalid(
                "record exceeds the configured size limit",
            ));
        }
        let record: Self = serde_json::from_slice(bytes)?;
        record.validate()?;
        Ok(record)
    }

    fn matches_inspected_state(
        &self,
        container_id: &str,
        launch_generation: &str,
        credential_scope: &UsageCredentialScope,
        capabilities: &[UsageAccountCapability],
    ) -> bool {
        self.validate().is_ok()
            && canonical_generation(launch_generation).is_ok()
            && self.backend == BACKEND
            && self.container_id == container_id
            && self.launch_generation == launch_generation
            && &self.credential_scope == credential_scope
            && self.capabilities == capabilities
    }
}

#[derive(Debug)]
enum RelayStateError {
    Invalid(&'static str),
    Io(io::Error),
    Json(serde_json::Error),
    Publish(jackin_runtime_coordination::coordination::StatePublishError),
}

impl std::fmt::Display for RelayStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "coordination state I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "coordination state JSON is invalid: {error}"),
            Self::Publish(error) => {
                write!(formatter, "coordination state publication failed: {error}")
            }
        }
    }
}

impl std::error::Error for RelayStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(_) => None,
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Publish(error) => Some(error),
        }
    }
}

impl From<io::Error> for RelayStateError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for RelayStateError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<jackin_runtime_coordination::coordination::StatePublishError> for RelayStateError {
    fn from(error: jackin_runtime_coordination::coordination::StatePublishError) -> Self {
        Self::Publish(error)
    }
}

fn canonical_generation(value: &str) -> Result<Uuid, RelayStateError> {
    let generation = Uuid::parse_str(value)
        .map_err(|_| RelayStateError::Invalid("launch generation is not a UUID"))?;
    if generation.hyphenated().to_string() != value {
        return Err(RelayStateError::Invalid(
            "launch generation is not canonical lower-case UUID text",
        ));
    }
    Ok(generation)
}

fn record_key(container_id: &str) -> Result<String, RelayStateError> {
    if container_id.is_empty() {
        return Err(RelayStateError::Invalid("container identity is empty"));
    }
    let mut hasher = Sha256::new();
    hasher.update(RECORD_KEY_DOMAIN);
    hasher.update(BACKEND.as_bytes());
    hasher.update([0]);
    hasher.update(container_id.as_bytes());
    let digest = hasher.finalize();
    let hexadecimal = digest
        .iter()
        .flat_map(|byte| {
            let high = b"0123456789abcdef"[(byte >> 4) as usize];
            let low = b"0123456789abcdef"[(byte & 0x0f) as usize];
            [char::from(high), char::from(low)]
        })
        .collect::<String>();
    Ok(format!("{RECORD_KEY_PREFIX}{hexadecimal}"))
}

fn record_directory(paths: &jackin_core::JackinPaths) -> Result<PathBuf, RelayStateError> {
    Ok(jackin_runtime_coordination::coordination::universe_dir(
        paths,
    )?)
}

fn open_record_lock(
    paths: &jackin_core::JackinPaths,
    container_id: &str,
) -> Result<std::fs::File, RelayStateError> {
    let key = record_key(container_id)?;
    let directory = record_directory(paths)?;
    Ok(jackin_runtime_coordination::coordination::open_in_namespace(&directory, &key)?)
}

/// Read a host-only record while the caller holds its stable per-container lock.
fn read_record(
    paths: &jackin_core::JackinPaths,
    container_id: &str,
) -> Result<Option<AppleUsageRelayRecordV1>, RelayStateError> {
    let key = record_key(container_id)?;
    let directory = record_directory(paths)?;
    let Some(bytes) = jackin_runtime_coordination::coordination::read_state_bytes_in_namespace(
        &directory,
        &key,
        RECORD_MAX_BYTES,
    )?
    else {
        return Ok(None);
    };
    let record = AppleUsageRelayRecordV1::decode(&bytes)?;
    if record.container_id != container_id {
        return Err(RelayStateError::Invalid(
            "record container identity does not match the requested container",
        ));
    }
    Ok(Some(record))
}

/// Publish a complete host-only record while the caller holds its stable per-container lock.
fn publish_record(
    paths: &jackin_core::JackinPaths,
    record: &AppleUsageRelayRecordV1,
) -> Result<(), RelayStateError> {
    let bytes = record.encode()?;
    let key = record_key(&record.container_id)?;
    let directory = record_directory(paths)?;
    jackin_runtime_coordination::coordination::publish_state_bytes_in_namespace(
        &directory,
        &key,
        &bytes,
        RECORD_MAX_BYTES,
    )?;
    Ok(())
}

const CONTAINER_ID: &str = "apple-container/exact-id";
const GENERATION: &str = "00112233-4455-4677-8899-aabbccddeeff";

fn scope() -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: "account-a".to_owned(),
            surface_id: "openai".to_owned(),
            key: "OPENAI_API_KEY".to_owned(),
            source: UsageCredentialSourceIdentity::HostEnv {
                name: "OPENAI_API_KEY".to_owned(),
            },
            material_fingerprint: "fingerprint-only".to_owned(),
        }]),
    }
}

fn capabilities() -> Vec<UsageAccountCapability> {
    vec![
        UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "openai".to_owned(),
        },
        UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "anthropic".to_owned(),
        },
    ]
}

fn record() -> AppleUsageRelayRecordV1 {
    AppleUsageRelayRecordV1::new(CONTAINER_ID, GENERATION, &scope(), &capabilities()).unwrap()
}

#[test]
fn record_roundtrip_keeps_exact_scope_and_ordered_capabilities() {
    let expected = record();
    let encoded = expected.encode().unwrap();
    let decoded = AppleUsageRelayRecordV1::decode(&encoded).unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(decoded.capabilities, capabilities());
    assert_eq!(decoded.credential_scope, scope());
    assert!(decoded.matches_inspected_state(CONTAINER_ID, GENERATION, &scope(), &capabilities()));

    let mut reordered = capabilities();
    reordered.reverse();
    assert!(!decoded.matches_inspected_state(CONTAINER_ID, GENERATION, &scope(), &reordered));
    assert!(!decoded.matches_inspected_state(
        CONTAINER_ID,
        "10112233-4455-4677-8899-aabbccddeeff",
        &scope(),
        &capabilities()
    ));
    assert!(!decoded.matches_inspected_state(
        "a-different-container",
        GENERATION,
        &scope(),
        &capabilities()
    ));
    assert!(!decoded.matches_inspected_state(
        CONTAINER_ID,
        GENERATION,
        &UsageCredentialScope::default(),
        &capabilities()
    ));
}

#[test]
fn record_rejects_bad_schema_backend_identity_generation_and_top_level_fields() {
    let expected = record();
    let encoded = expected.encode().unwrap();
    let value = serde_json::from_slice::<serde_json::Value>(&encoded).unwrap();

    let mut unknown_top_level = value.clone();
    unknown_top_level["future_field"] = serde_json::json!("must reject");
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&unknown_top_level).unwrap()),
        Err(RelayStateError::Json(_))
    ));

    let mut missing_required = value.clone();
    missing_required
        .as_object_mut()
        .unwrap()
        .remove("launch_generation");
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&missing_required).unwrap()),
        Err(RelayStateError::Json(_))
    ));

    let mut wrong_version = value.clone();
    wrong_version["schema_version"] = serde_json::json!(2);
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&wrong_version).unwrap()),
        Err(RelayStateError::Invalid(_))
    ));

    let mut wrong_backend = value.clone();
    wrong_backend["backend"] = serde_json::json!("docker");
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&wrong_backend).unwrap()),
        Err(RelayStateError::Invalid(_))
    ));

    let mut empty_id = value.clone();
    empty_id["container_id"] = serde_json::json!("");
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&empty_id).unwrap()),
        Err(RelayStateError::Invalid(_))
    ));

    let mut noncanonical_generation = value;
    noncanonical_generation["launch_generation"] = serde_json::json!(GENERATION.to_uppercase());
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&noncanonical_generation).unwrap()),
        Err(RelayStateError::Invalid(_))
    ));
    assert!(matches!(
        AppleUsageRelayRecordV1::decode(&vec![b' '; RECORD_MAX_BYTES + 1]),
        Err(RelayStateError::Invalid(_))
    ));
}

#[test]
fn only_top_level_record_fields_are_strict_under_existing_protocol_serde_types() {
    let expected = record();
    let mut value = serde_json::to_value(&expected).unwrap();
    value["credential_scope"]["future_nested_field"] = serde_json::json!("ignored by shared type");
    let decoded = AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(decoded, expected);

    value["future_top_level_field"] = serde_json::json!(true);
    AppleUsageRelayRecordV1::decode(&serde_json::to_vec(&value).unwrap()).unwrap_err();
}

#[test]
fn record_key_is_deterministic_opaque_and_validates_empty_identity() {
    let first = record_key(CONTAINER_ID).unwrap();
    assert_eq!(first, record_key(CONTAINER_ID).unwrap());
    assert_ne!(first, record_key("apple-container/exact-id-2").unwrap());
    assert!(first.starts_with("apple-usage-relay-"));
    assert!(!first.contains(CONTAINER_ID));
    assert!(
        first
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || b"-_..".contains(&byte) })
    );
    assert!(matches!(record_key(""), Err(RelayStateError::Invalid(_))));
}

#[test]
fn host_record_read_is_noncreating_and_roundtrips_only_under_its_stable_lock() {
    use std::io::Write as _;

    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    assert!(read_record(&paths, CONTAINER_ID).unwrap().is_none());
    assert!(!paths.home_dir.exists());

    let lock = open_record_lock(&paths, CONTAINER_ID).unwrap();
    lock.try_lock().unwrap();
    let expected = record();
    publish_record(&paths, &expected).unwrap();
    assert_eq!(read_record(&paths, CONTAINER_ID).unwrap(), Some(expected));

    let key = record_key(CONTAINER_ID).unwrap();
    let directory = jackin_runtime_coordination::coordination::universe_dir(&paths).unwrap();
    let leaf = format!("{key}.json");
    let mut file = jackin_runtime_coordination::coordination::open_state_in_namespace(
        &directory, &leaf, false,
    )
    .unwrap();
    file.set_len(0).unwrap();
    file.write_all(b"{").unwrap();
    assert!(matches!(
        read_record(&paths, CONTAINER_ID),
        Err(RelayStateError::Json(_))
    ));

    let other =
        AppleUsageRelayRecordV1::new("different-container", GENERATION, &scope(), &capabilities())
            .unwrap();
    let mut file = jackin_runtime_coordination::coordination::open_state_in_namespace(
        &directory, &leaf, false,
    )
    .unwrap();
    file.set_len(0).unwrap();
    file.write_all(&other.encode().unwrap()).unwrap();
    assert!(matches!(
        read_record(&paths, CONTAINER_ID),
        Err(RelayStateError::Invalid(_))
    ));
}
