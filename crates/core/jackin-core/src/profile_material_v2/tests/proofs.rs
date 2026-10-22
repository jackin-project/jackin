// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    Agent, ProfileCredentialFingerprintV2, ProfileCredentialSourceIdentityV2,
    profile_credential_source_identity_v2,
};

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
    let valid = format!(
        "{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"
    );
    let invalid = [
        format!("{{\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"),
        format!("{{\"scheme\":\"v1\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\",\"CANARY_FIELD\":null}}"),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"agent\":\"claude\",\"descriptor_fingerprint\":\"{digest}\"}}"),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\",\"scheme\":\"jackin.profile-credential-source.v2\"}}"),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"CANARY_VALUE\",\"descriptor_fingerprint\":\"{digest}\"}}"),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":7,\"descriptor_fingerprint\":\"{digest}\"}}"),
        "{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":7}".to_owned(),
        format!("{{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{canary}\"}}"),
        "{\"scheme\":\"jackin.profile-credential-source.v2\",\"agent\":\"codex\",\"descriptor_fingerprint\":null}".to_owned(),
    ];
    serde_json::from_str::<ProfileCredentialSourceIdentityV2>(&valid)
        .expect("valid source identity wire must deserialize");
    for raw in invalid {
        let error = serde_json::from_str::<ProfileCredentialSourceIdentityV2>(&raw)
            .expect_err("invalid source identity wire unexpectedly deserialized")
            .to_string();
        assert!(error.starts_with("invalid profile credential source identity"));
        assert!(!error.contains("CANARY_FIELD"));
        assert!(!error.contains("CANARY_VALUE"));
        assert!(!error.contains(canary));
    }
}
