// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AgentCredentialEnv, InstanceCredentialEnv};
use serde_json::json;
use std::collections::BTreeMap;

const SENTINEL_VALUE: &str = "sentinel-secret-value-xyz";
const SENTINEL_KEY: &str = "SENTINEL_SECRET_KEY_XYZ";

fn sample() -> AgentCredentialEnv {
    let env = BTreeMap::from([(SENTINEL_KEY.to_owned(), SENTINEL_VALUE.to_owned())]);
    AgentCredentialEnv::new(BTreeMap::from([(
        "worker-0".to_owned(),
        InstanceCredentialEnv {
            agent: "codex".to_owned(),
            account_id: "acc-1".to_owned(),
            env,
        },
    )]))
}

#[test]
fn serializes_exact_v2_shape() {
    let encoded = serde_json::to_value(sample()).expect("serialize");
    assert_eq!(
        encoded,
        json!({
            "schema_version": 2,
            "instances": {
                "worker-0": {
                    "agent": "codex",
                    "account_id": "acc-1",
                    "env": { SENTINEL_KEY: SENTINEL_VALUE },
                },
            },
        }),
    );
}

#[test]
fn round_trip_preserves_envelope() {
    let original = sample();
    let encoded = serde_json::to_string(&original).expect("serialize");
    let decoded: AgentCredentialEnv = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded, original);
    assert_eq!(decoded.schema_version(), 2);
    assert_eq!(
        decoded
            .for_instance("worker-0")
            .and_then(|env| env.get(SENTINEL_KEY))
            .map(String::as_str),
        Some(SENTINEL_VALUE),
    );
}

#[test]
fn accessors_return_instance_data() {
    let creds = sample();
    let entry = creds.instance("worker-0").expect("entry");
    assert_eq!(entry.agent, "codex");
    assert_eq!(entry.account_id, "acc-1");
    assert!(creds.instance("missing").is_none());
    assert!(creds.for_instance("missing").is_none());
    assert_eq!(creds.iter().count(), 1);
    assert!(!creds.is_empty());
}

#[test]
fn debug_redacts_everything() {
    let rendered = format!("{:?}", sample());
    assert_eq!(rendered, "AgentCredentialEnv([REDACTED])");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains(SENTINEL_VALUE));
    assert!(!rendered.contains(SENTINEL_KEY));
}

#[test]
fn default_is_empty_v2() {
    let creds = AgentCredentialEnv::default();
    assert!(creds.is_empty());
    assert_eq!(creds.schema_version(), 2);
    assert_eq!(creds.iter().count(), 0);
    let encoded = serde_json::to_value(&creds).expect("serialize");
    assert_eq!(encoded, json!({ "schema_version": 2, "instances": {} }));
}
