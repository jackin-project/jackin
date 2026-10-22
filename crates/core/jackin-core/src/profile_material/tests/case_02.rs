// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{ProfileCredentialSourceIdentity, ProfileCredentialSourceMaterial};
use super::*;

const ZERO_FINGERPRINT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn revision(agent: Agent, raw: &[u8]) -> Result<String, serde_json::Error> {
    profile_credential_material_revision(agent, raw)
}

fn identity_json(fingerprint: &str) -> String {
    format!(r#"{{"agent":"hermes","descriptor_fingerprint":"{fingerprint}"}}"#)
}

fn material_json(descriptor: &str, material: &str) -> String {
    format!(
        r#"{{"source":{},"material_revision":"{material}"}}"#,
        identity_json(descriptor)
    )
}

fn nested_arrays(depth: usize) -> Vec<u8> {
    let mut raw = vec![b'['; depth];
    raw.extend_from_slice(b"null");
    raw.extend(std::iter::repeat_n(b']', depth));
    raw
}

fn array_of_nulls(values: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(values * 5 + 2);
    raw.push(b'[');
    for index in 0..values {
        if index != 0 {
            raw.push(b',');
        }
        raw.extend_from_slice(b"null");
    }
    raw.push(b']');
    raw
}

#[test]
fn version_one_fingerprints_keep_frozen_valid_bytes() {
    let ordinary = revision(
        Agent::Hermes,
        br#"{"token":"fixture","nested":{"b":true,"a":[null,3]}}"#,
    )
    .expect("ordinary JSON fixture is valid");
    assert_eq!(
        ordinary,
        "97be49d8086755c89d452455ed56ffd39800fe7f37f4ce079331be97644c830a"
    );
    assert_eq!(
        revision(
            Agent::Hermes,
            br#"{ "nested": {"a": [null, 3], "b": true}, "token": "fixture" }"#,
        )
        .expect("reordered JSON fixture is valid"),
        ordinary
    );

    assert_eq!(
        revision(Agent::Hermes, br#"{"revision":1}"#).expect("integer JSON is valid"),
        "771a6b91867e6431b0ff7e326602a82422f3fbc180222d9dfd630383ce4ec69b"
    );
    let normalized_number = "c13ba930b842d8541de9e10cc8b2068f5c6e0c52ae65b8c5025cb8790aeeb6bd";
    for raw in [
        br#"{"revision":1.0}"#.as_slice(),
        br#"{"revision":1e0}"#.as_slice(),
        br#"{"revision":1E+0}"#.as_slice(),
    ] {
        assert_eq!(
            revision(Agent::Hermes, raw).expect("equivalent JSON number is valid"),
            normalized_number
        );
    }
    let normalized_large_integer =
        "ce9d85fc07f0c592610f7f77e8b6f3dc5e07f019fb3d18011aaf7e49bfbd8055";
    for raw in [
        br#"{"revision":18446744073709551616}"#.as_slice(),
        br#"{"revision":18446744073709551617}"#.as_slice(),
    ] {
        assert_eq!(
            revision(Agent::Hermes, raw).expect("large JSON integer is valid"),
            normalized_large_integer
        );
    }

    let amp_revision = "13369003a3a9aa6f207ce6aa0453bcf988f954a7f8ad242137a0771204191e78";
    for raw in [
        br#"{"apiKey@https://ampcode.com/":"fixture","apiKey@https://foreign.example/":"foreign-one","mcp-oauth@https://ampcode.com/":"mcp-one"}"#.as_slice(),
        br#"{"apiKey@https://ampcode.com":"fixture","apiKey@https://foreign.example/":"foreign-two","mcp-oauth@https://ampcode.com/":"mcp-two"}"#.as_slice(),
    ] {
        assert_eq!(
            revision(Agent::Amp, raw).expect("Amp fixture has one canonical token"),
            amp_revision
        );
    }
    assert_eq!(
        revision(Agent::Omp, &[0, 255, 0, 128]).expect("Omp payload is opaque bytes"),
        "421e1f1623eba2c249437e67eaee9e68416cc4b6df15bf8b6ed62c25069a2f47"
    );

    let absent = profile_credential_source_identity(
        Agent::Hermes,
        "fixture",
        Path::new("/profiles/frozen-v1"),
        None,
    );
    let explicit_null = profile_credential_source_identity(
        Agent::Hermes,
        "fixture",
        Path::new("/profiles/frozen-v1"),
        Some(&serde_json::Value::Null),
    );
    assert_eq!(
        absent.descriptor_fingerprint,
        "94bcc0f0d3b616de0f251f0ee7eb2a80b75f45f6d8482d77ffc5bd56722fbee8"
    );
    assert_eq!(
        explicit_null.descriptor_fingerprint,
        "2cacecca69c342d2528ff7f26a28414cfec212ca82a8ab0344faf2ae2680c31d"
    );

    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let identity = profile_credential_source_identity(
            Agent::Hermes,
            "fixture",
            Path::new(OsStr::from_bytes(b"/profiles/frozen-v1/\xff")),
            None,
        );
        assert_eq!(
            identity.descriptor_fingerprint,
            "9303b09bd62d54171fdbeaf31a68e53f1bba85141e3c0e34a9ec6d08cda22cfb"
        );
    }
}

#[test]
fn json_material_rejects_duplicate_decoded_keys_recursively() {
    for raw in [
        br#"{"secret-canary":"first","secret-canary":"second"}"#.as_slice(),
        br#"{"outer":{"secret-canary":"first","secret-canary":"second"}}"#.as_slice(),
        br#"[{"secret-canary":"first","secret-canary":"second"}]"#.as_slice(),
        br#"{"token":"first","to\u006ben":"second"}"#.as_slice(),
    ] {
        let error = revision(Agent::Hermes, raw).expect_err("duplicate decoded keys must fail");
        let message = error.to_string();
        assert!(message.contains("duplicate object keys"));
        assert!(!message.contains("secret-canary"));
        assert!(!message.contains("first"));
        assert!(!message.contains("second"));
    }

    let literal = revision(Agent::Hermes, br#"{"token":"fixture","other":1}"#)
        .expect("distinct object keys are valid");
    let escaped = revision(Agent::Hermes, br#"{"to\u006ben":"fixture","other":1}"#)
        .expect("a unique escaped key is valid");
    assert_eq!(literal, escaped);
}

#[test]
fn amp_rejects_duplicates_in_canonical_and_ignored_metadata() {
    for raw in [
        br#"{"apiKey@https://ampcode.com/":"first-canary","apiKey@https://ampcode.com/":"second-canary"}"#.as_slice(),
        br#"{"apiKey@https://ampcode.com/":"token-canary","apiKey@https://foreign.example/":"first-canary","apiKey@https://foreign.example/":"second-canary"}"#.as_slice(),
    ] {
        let error = revision(Agent::Amp, raw).expect_err("Amp JSON duplicates must fail");
        let message = error.to_string();
        assert!(message.contains("duplicate object keys"));
        assert!(!message.contains("canary"));
    }

    let first = revision(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":"fixture","apiKey@https://foreign.example/":"foreign-one","mcp-oauth@https://ampcode.com/":"mcp-one"}"#,
    )
    .expect("Amp ignores valid unrelated metadata");
    let second = revision(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":"fixture","apiKey@https://foreign.example/":"foreign-two","mcp-oauth@https://ampcode.com/":"mcp-two"}"#,
    )
    .expect("Amp ignores changed valid unrelated metadata");
    assert_eq!(first, second);
}

#[test]
fn proof_dto_deserialization_validates_fingerprint_shape_and_keeps_wire_shape() {
    let identity_raw = identity_json(ZERO_FINGERPRINT);
    let identity: ProfileCredentialSourceIdentity =
        serde_json::from_str(&identity_raw).expect("lowercase SHA-256 is valid");
    assert_eq!(serde_json::to_string(&identity).unwrap(), identity_raw);

    let material_raw = material_json(ZERO_FINGERPRINT, ZERO_FINGERPRINT);
    let material: ProfileCredentialSourceMaterial =
        serde_json::from_str(&material_raw).expect("lowercase SHA-256 material is valid");
    assert_eq!(serde_json::to_string(&material).unwrap(), material_raw);

    let invalid_fingerprints = [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
    ];
    for fingerprint in &invalid_fingerprints {
        let _error =
            serde_json::from_str::<ProfileCredentialSourceIdentity>(&identity_json(fingerprint))
                .expect_err("invalid descriptor fingerprint must fail");
        let _error = serde_json::from_str::<ProfileCredentialSourceMaterial>(&material_json(
            fingerprint,
            ZERO_FINGERPRINT,
        ))
        .expect_err("invalid nested descriptor fingerprint must fail");
        let _error = serde_json::from_str::<ProfileCredentialSourceMaterial>(&material_json(
            ZERO_FINGERPRINT,
            fingerprint,
        ))
        .expect_err("invalid material fingerprint must fail");
    }

    for raw in [
        r#"{"agent":"hermes","descriptor_fingerprint":null}"#.to_owned(),
        r#"{"agent":"hermes","descriptor_fingerprint":7}"#.to_owned(),
        format!(r#"{{"agent":"unknown","descriptor_fingerprint":"{ZERO_FINGERPRINT}"}}"#),
    ] {
        let _error = serde_json::from_str::<ProfileCredentialSourceIdentity>(&raw)
            .expect_err("malformed identity fingerprint must fail");
    }
    for raw in [
        format!(r#"{{"source":{identity_raw},"material_revision":null}}"#),
        format!(r#"{{"source":{identity_raw},"material_revision":7}}"#),
    ] {
        let _error = serde_json::from_str::<ProfileCredentialSourceMaterial>(&raw)
            .expect_err("malformed material fingerprint must fail");
    }
}

#[test]
fn proof_dto_deserialization_keeps_required_duplicate_and_unknown_field_checks() {
    let valid = identity_json(ZERO_FINGERPRINT);
    let invalid_identity = [
        r#"{"agent":"hermes"}"#.to_owned(),
        format!(
            r#"{{"agent":"hermes","descriptor_fingerprint":"{ZERO_FINGERPRINT}","extra":"secret-canary"}}"#
        ),
        format!(
            r#"{{"agent":"hermes","descriptor_fingerprint":"{ZERO_FINGERPRINT}","descriptor_fingerprint":"secret-canary"}}"#
        ),
    ];
    for raw in &invalid_identity {
        let error = serde_json::from_str::<ProfileCredentialSourceIdentity>(raw)
            .expect_err("malformed identity DTO must fail");
        assert!(!error.to_string().contains("secret-canary"));
    }

    let invalid_material = [
        format!(r#"{{"source":{valid}}}"#),
        format!(
            r#"{{"source":{valid},"material_revision":"{ZERO_FINGERPRINT}","extra":"secret-canary"}}"#
        ),
        format!(
            r#"{{"source":{valid},"material_revision":"{ZERO_FINGERPRINT}","material_revision":"secret-canary"}}"#
        ),
        format!(
            r#"{{"source":{{"agent":"hermes","descriptor_fingerprint":"{ZERO_FINGERPRINT}","descriptor_fingerprint":"secret-canary"}},"material_revision":"{ZERO_FINGERPRINT}"}}"#
        ),
    ];
    for raw in &invalid_material {
        let error = serde_json::from_str::<ProfileCredentialSourceMaterial>(raw)
            .expect_err("malformed material DTO must fail");
        assert!(!error.to_string().contains("secret-canary"));
    }
}

#[test]
fn json_parser_keeps_grammar_and_trailing_input_checks() {
    for raw in [
        br#"{"revision":01}"#.as_slice(),
        br#"{"revision":+1}"#.as_slice(),
        br#"{"revision":.1}"#.as_slice(),
        br#"{"revision":1.}"#.as_slice(),
        br#"{"revision":1e+-2}"#.as_slice(),
        b"null false".as_slice(),
        b"\"\xff\"".as_slice(),
    ] {
        let _error = revision(Agent::Hermes, raw).expect_err("invalid JSON must fail");
    }
}

#[test]
fn json_parser_keeps_the_existing_depth_limit() {
    let _revision = revision(Agent::Hermes, &nested_arrays(127))
        .expect("serde_json accepts 127 nested containers by default");
    let _error = revision(Agent::Hermes, &nested_arrays(128))
        .expect_err("serde_json rejects its 128th nested container by default");
}

#[test]
fn large_whitespace_and_ignored_amp_metadata_keep_v1_fingerprints() {
    let mut whitespace_json = br#"{"revision":1}"#.to_vec();
    whitespace_json.resize(8 * 1024 * 1024 + 1, b' ');
    assert_eq!(
        revision(Agent::Hermes, &whitespace_json).expect("large trailing whitespace is valid"),
        "771a6b91867e6431b0ff7e326602a82422f3fbc180222d9dfd630383ce4ec69b"
    );

    let mut amp_json = br#"{"apiKey@https://ampcode.com/":"fixture","ignored":""#.to_vec();
    amp_json.extend(std::iter::repeat_n(b'x', 8 * 1024 * 1024 + 1));
    amp_json.extend_from_slice(b"\"}");
    assert_eq!(
        revision(Agent::Amp, &amp_json).expect("large ignored Amp metadata is valid"),
        "13369003a3a9aa6f207ce6aa0453bcf988f954a7f8ad242137a0771204191e78"
    );
}

#[test]
fn json_with_more_than_sixty_five_thousand_values_keeps_its_v1_fingerprint() {
    let raw = array_of_nulls(65_536);
    assert_eq!(
        revision(Agent::Hermes, &raw).expect("large JSON array was previously accepted"),
        "43cd8c3ce164d0c6d72c674f4118e1ff317e060b88a2888b67231b9901083d10"
    );
}
