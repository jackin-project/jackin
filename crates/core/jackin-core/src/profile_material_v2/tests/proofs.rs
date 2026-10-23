// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    Agent, ProfileCredentialFingerprintV2, ProfileCredentialSourceIdentityV2,
    profile_credential_source_identity_v2,
};

const SOURCE_SCHEME: &str = "jackin.profile-credential-source.v2";

#[test]
fn fixed_source_vectors_bind_absence_null_and_raw_number_tokens() {
    let expected = [
        (
            None,
            "95b5375b44aad30b66a156882a34656d30f43bf9634b1500d3ad1566c62d66b2",
        ),
        (
            Some(b"null".as_slice()),
            "ed263127ddf87fa2b45b35ccd139701b9c765196d6d337f265818f08aeb6a833",
        ),
        (
            Some(b"1".as_slice()),
            "cf0ca624a47ee6f02ab574156df3b1718c088bd8adae4a96d6c802cf6441de5e",
        ),
    ];

    for (selector, fingerprint) in expected {
        assert!(matches!(
            profile_credential_source_identity_v2(
                Agent::Codex,
                "openai",
                Path::new("/profiles/main"),
                selector,
            ),
            Ok(identity) if identity.agent() == Agent::Codex
                && identity.descriptor_fingerprint().as_str() == fingerprint
        ));
    }
}

#[test]
fn source_identity_binds_exact_provider_path_agent_and_selector() {
    let base = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        Some(br#"{"region":"us"}"#),
    );
    let changed = [
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai ",
            Path::new("/profiles/main"),
            Some(br#"{"region":"us"}"#),
        ),
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai",
            Path::new("/profiles/other"),
            Some(br#"{"region":"us"}"#),
        ),
        profile_credential_source_identity_v2(
            Agent::Claude,
            "openai",
            Path::new("/profiles/main"),
            Some(br#"{"region":"us"}"#),
        ),
        profile_credential_source_identity_v2(
            Agent::Codex,
            "openai",
            Path::new("/profiles/main"),
            Some(br#"{"region":"eu"}"#),
        ),
    ];

    for candidate in changed {
        assert!(
            matches!((base.as_ref(), candidate.as_ref()), (Ok(left), Ok(right))
            if left.descriptor_fingerprint() != right.descriptor_fingerprint())
        );
    }
}

#[test]
fn source_json_whitespace_and_object_order_are_semantically_equal() {
    let first = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        Some(br#"{"a":1,"b":[true,null]}"#),
    );
    let second = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        Some(br#" { "b" : [ true , null ] , "a" : 1 } "#),
    );
    assert!(matches!((first, second), (Ok(left), Ok(right))
        if left.descriptor_fingerprint() == right.descriptor_fingerprint()));
}

#[cfg(unix)]
#[test]
fn source_identity_preserves_non_utf8_path_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let first = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new(OsStr::from_bytes(b"/profiles/\xff")),
        None,
    );
    let second = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new(OsStr::from_bytes(b"/profiles/\xfe")),
        None,
    );
    assert!(matches!((first, second), (Ok(left), Ok(right))
        if left.descriptor_fingerprint() != right.descriptor_fingerprint()));
}

#[test]
fn source_identity_wire_is_tagged_exact_and_round_trips() {
    let identity = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        None,
    );
    assert!(matches!(identity, Ok(ref source)
        if matches!(serde_json::to_string(source), Ok(encoded)
            if encoded == format!(
                "{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{}\"}}",
                source.descriptor_fingerprint().as_str()
            )
            && matches!(serde_json::from_str::<ProfileCredentialSourceIdentityV2>(&encoded), Ok(decoded)
                if decoded.agent() == Agent::Codex
                    && decoded.descriptor_fingerprint() == source.descriptor_fingerprint()))));
}

#[test]
fn fingerprint_wire_is_a_validated_json_string_and_debug_is_redacted() {
    let canary = "0123456789abcdef".repeat(4);
    let parsed = ProfileCredentialFingerprintV2::parse_hex(&canary);
    assert!(matches!(parsed, Ok(ref digest)
        if matches!(serde_json::to_string(digest), Ok(encoded) if encoded == format!("\"{canary}\""))
            && !format!("{digest:?}").contains(&canary)));
    for invalid in [
        String::new(),
        "0".repeat(63),
        "0".repeat(65),
        "A".repeat(64),
        "z".repeat(64),
        "é".repeat(32),
    ] {
        assert_eq!(
            ProfileCredentialFingerprintV2::parse_hex(&invalid).err(),
            Some(crate::ProfileCredentialV2Error::InvalidFingerprint)
        );
    }
}

#[test]
fn identity_deserialization_rejects_bad_shapes_with_static_errors() {
    let digest = "0".repeat(64);
    let canary = "PROFILE_V2_DIGEST_CANARY";
    let scheme_canary = "PROFILE_V2_SCHEME_CANARY";
    let valid = format!(
        "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"
    );
    let invalid = [
        format!("{{\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"),
        format!(
            "{{\"scheme\":\"v1\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\",\"CANARY_FIELD\":null}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"agent\":\"claude\",\"descriptor_fingerprint\":\"{digest}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\",\"scheme\":\"{SOURCE_SCHEME}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"CANARY_VALUE\",\"descriptor_fingerprint\":\"{digest}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":7,\"descriptor_fingerprint\":\"{digest}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":7}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{canary}\"}}"
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{}\"}}",
            "0".repeat(63)
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{}\"}}",
            "0".repeat(65)
        ),
        format!(
            "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":null}}"
        ),
        format!(
            r#"{{"scheme":{{"{SOURCE_SCHEME}":null}},"agent":"codex","descriptor_fingerprint":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":{{"{scheme_canary}":null}},"agent":"codex","descriptor_fingerprint":"{digest}"}}"#
        ),
        format!(r#"{{"scheme":[],"agent":"codex","descriptor_fingerprint":"{digest}"}}"#),
        format!(r#"{{"scheme":7,"agent":"codex","descriptor_fingerprint":"{digest}"}}"#),
        format!(r#"{{"scheme":true,"agent":"codex","descriptor_fingerprint":"{digest}"}}"#),
        format!(r#"{{"scheme":null,"agent":"codex","descriptor_fingerprint":"{digest}"}}"#),
        "[]".to_owned(),
        format!(r#"["{SOURCE_SCHEME}"]"#),
        format!(r#"["{SOURCE_SCHEME}","codex","{digest}"]"#),
        format!(r#"["{SOURCE_SCHEME}","codex","{digest}",null]"#),
    ];
    serde_json::from_str::<ProfileCredentialSourceIdentityV2>(&valid)
        .expect("valid source identity wire must deserialize");
    for raw in invalid {
        let error = serde_json::from_str::<ProfileCredentialSourceIdentityV2>(&raw)
            .expect_err("invalid source identity wire unexpectedly deserialized");
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(display.starts_with("invalid profile credential source identity"));
        for secret in ["CANARY_FIELD", "CANARY_VALUE", canary, scheme_canary] {
            assert!(!display.contains(secret));
            assert!(!debug.contains(secret));
        }
    }
}
