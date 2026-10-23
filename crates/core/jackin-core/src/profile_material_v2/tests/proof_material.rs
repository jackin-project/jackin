// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    Agent, ProfileCredentialSourceMaterialV2, profile_credential_source_identity_v2,
    profile_credential_source_material_v2,
};

const SOURCE_SCHEME: &str = "jackin.profile-credential-source.v2";
const MATERIAL_SCHEME: &str = "jackin.profile-credential-material.v2";

fn valid_source_wire() -> String {
    let digest = "0123456789abcdef".repeat(4);
    format!(
        "{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{digest}\"}}"
    )
}

fn valid_material_wire() -> String {
    let digest = "0123456789abcdef".repeat(4);
    format!(
        "{{\"scheme\":\"{MATERIAL_SCHEME}\",\"source\":{},\"material_revision\":\"{digest}\"}}",
        valid_source_wire()
    )
}

fn example_material() -> ProfileCredentialSourceMaterialV2 {
    let source = profile_credential_source_identity_v2(
        Agent::Codex,
        "openai",
        Path::new("/profiles/main"),
        None,
    )
    .expect("valid source identity should be constructed");
    profile_credential_source_material_v2(source, br#"{"token":"material"}"#)
        .expect("valid profile material should be constructed")
}

#[test]
fn material_wire_has_exact_tags_and_round_trips_nested_identity() {
    let material = example_material();
    let encoded = serde_json::to_string(&material).expect("material serializes");
    let expected = format!(
        "{{\"scheme\":\"{MATERIAL_SCHEME}\",\"source\":{{\"scheme\":\"{SOURCE_SCHEME}\",\"agent\":\"codex\",\"descriptor_fingerprint\":\"{}\"}},\"material_revision\":\"{}\"}}",
        material.source().descriptor_fingerprint().as_str(),
        material.material_revision().as_str()
    );
    assert_eq!(encoded, expected);

    let decoded = serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&encoded)
        .expect("valid tagged material should deserialize");
    assert_eq!(decoded.source().agent(), Agent::Codex);
    assert_eq!(
        decoded.source().descriptor_fingerprint().as_str(),
        material.source().descriptor_fingerprint().as_str()
    );
    assert_eq!(
        decoded.material_revision().as_str(),
        material.material_revision().as_str()
    );
    let debug = format!("{decoded:?}");
    assert!(!debug.contains(material.source().descriptor_fingerprint().as_str()));
    assert!(!debug.contains(material.material_revision().as_str()));
}

#[test]
fn material_wire_rejects_bad_shapes_with_static_errors() {
    let source = valid_source_wire();
    let digest = "0123456789abcdef".repeat(4);
    let canary = "PROFILE_V2_MATERIAL_CANARY";
    let scheme_canary = "PROFILE_V2_SCHEME_CANARY";
    let uppercase = "A".repeat(64);
    let uppercase_source = source.replace(&digest, &uppercase);
    let invalid = [
        format!(r#"{{"source":{source},"material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source}}}"#),
        format!(r#"{{"scheme":"v1","source":{source},"material_revision":"{digest}"}}"#),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{digest}","{canary}":null}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{digest}","material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":"v1","agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":"{SOURCE_SCHEME}","agent":"codex","descriptor_fingerprint":"{digest}","{canary}":null}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":"{SOURCE_SCHEME}","agent":"codex","agent":"claude","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{canary}"}}"#
        ),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":7}}"#),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":"{SOURCE_SCHEME}","agent":"{canary}","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{uppercase}"}}"#
        ),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"0123"}}"#),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{uppercase_source},"material_revision":"{digest}"}}"#
        ),
        format!(r#"{{"scheme":"{canary}","source":{source},"material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":1e+}}"#),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"\q"}}"#),
        format!(
            r#"{{"scheme":{{"{MATERIAL_SCHEME}":null}},"source":{source},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":{{"{scheme_canary}":null}},"source":{source},"material_revision":"{digest}"}}"#
        ),
        format!(r#"{{"scheme":[],"source":{source},"material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":7,"source":{source},"material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":true,"source":{source},"material_revision":"{digest}"}}"#),
        format!(r#"{{"scheme":null,"source":{source},"material_revision":"{digest}"}}"#),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":{{"{SOURCE_SCHEME}":null}},"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":{{"{scheme_canary}":null}},"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":[],"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":7,"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":true,"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{{"scheme":null,"agent":"codex","descriptor_fingerprint":"{digest}"}},"material_revision":"{digest}"}}"#
        ),
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":[],"material_revision":"{digest}"}}"#),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":["{SOURCE_SCHEME}"],"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":["{SOURCE_SCHEME}","codex","{digest}"],"material_revision":"{digest}"}}"#
        ),
        format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":["{SOURCE_SCHEME}","codex","{digest}",null],"material_revision":"{digest}"}}"#
        ),
        "[]".to_owned(),
        format!(r#"["{MATERIAL_SCHEME}"]"#),
        format!(r#"["{MATERIAL_SCHEME}",{source},"{digest}"]"#),
        format!(r#"["{MATERIAL_SCHEME}",{source},"{digest}",null]"#),
        format!(r#"["{MATERIAL_SCHEME}",["{SOURCE_SCHEME}","codex","{digest}"],"{digest}"]"#),
    ];

    serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&valid_material_wire())
        .expect("valid material wire should deserialize");
    for raw in invalid {
        let serde_error = serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&raw)
            .expect_err("invalid material wire unexpectedly deserialized");
        let display = serde_error.to_string();
        let debug = format!("{serde_error:?}");
        assert!(display.starts_with("invalid profile credential source material"));
        assert!(!display.contains(canary));
        assert!(!debug.contains(canary));
        assert!(!display.contains(scheme_canary));
        assert!(!debug.contains(scheme_canary));
    }

    for invalid_digest in [
        String::new(),
        "0".repeat(63),
        "0".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
    ] {
        let invalid_source = source.replace(&digest, &invalid_digest);
        let invalid_descriptor = format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{invalid_source},"material_revision":"{digest}"}}"#
        );
        let invalid_revision = format!(
            r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":"{invalid_digest}"}}"#
        );

        for raw in [invalid_descriptor, invalid_revision] {
            let error = serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&raw)
                .expect_err("invalid fingerprint shape unexpectedly deserialized");
            assert!(
                error
                    .to_string()
                    .starts_with("invalid profile credential source material")
            );
            if !invalid_digest.is_empty() {
                assert!(!format!("{error:?}").contains(&invalid_digest));
            }
        }
    }

    let quoted_digest = format!("\"{digest}\"");
    let wrong_type_source = source.replace(quoted_digest.as_str(), "7");
    let wrong_descriptor_type = format!(
        r#"{{"scheme":"{MATERIAL_SCHEME}","source":{wrong_type_source},"material_revision":"{digest}"}}"#
    );
    let wrong_revision_type =
        format!(r#"{{"scheme":"{MATERIAL_SCHEME}","source":{source},"material_revision":7}}"#);
    for raw in [wrong_descriptor_type, wrong_revision_type] {
        let error = serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&raw)
            .expect_err("wrong fingerprint type unexpectedly deserialized");
        assert!(
            error
                .to_string()
                .starts_with("invalid profile credential source material")
        );
    }
}

#[test]
fn v1_and_untagged_wire_records_are_rejected() {
    let synthetic_digest = "0123456789abcdef".repeat(4);
    let untagged_identity =
        format!(r#"{{"agent":"codex","descriptor_fingerprint":"{synthetic_digest}"}}"#);
    let actual = example_material();
    let descriptor = actual.source().descriptor_fingerprint().as_str();
    let revision = actual.material_revision().as_str();
    let v1_identity = format!(
        r#"{{"scheme":"jackin.profile-credential-source.v1","agent":"codex","descriptor_fingerprint":"{descriptor}"}}"#
    );
    let v2_identity = serde_json::to_string(actual.source()).expect("V2 identity serializes");
    let untagged_material =
        format!(r#"{{"source":{untagged_identity},"material_revision":"{synthetic_digest}"}}"#);
    let v1_material = format!(
        r#"{{"scheme":"jackin.profile-credential-material.v1","source":{v1_identity},"material_revision":"{revision}"}}"#
    );
    let v1_outer_v2_inner = format!(
        r#"{{"scheme":"jackin.profile-credential-material.v1","source":{v2_identity},"material_revision":"{revision}"}}"#
    );
    let v2_outer_v1_inner = format!(
        r#"{{"scheme":"{MATERIAL_SCHEME}","source":{v1_identity},"material_revision":"{revision}"}}"#
    );

    for raw in [untagged_identity, v1_identity] {
        let Err(error) = serde_json::from_str::<crate::ProfileCredentialSourceIdentityV2>(&raw)
        else {
            panic!("legacy identity wire unexpectedly deserialized");
        };
        assert!(
            error
                .to_string()
                .starts_with("invalid profile credential source identity")
        );
        assert!(!error.to_string().contains(descriptor));
        assert!(!format!("{error:?}").contains(descriptor));
    }

    for raw in [
        untagged_material,
        v1_material,
        v1_outer_v2_inner,
        v2_outer_v1_inner,
    ] {
        let Err(error) = serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&raw) else {
            panic!("legacy or mismatched proof wire unexpectedly deserialized");
        };
        assert!(
            error
                .to_string()
                .starts_with("invalid profile credential source material")
        );
        assert!(!error.to_string().contains(descriptor));
        assert!(!error.to_string().contains(revision));
        assert!(!format!("{error:?}").contains(descriptor));
        assert!(!format!("{error:?}").contains(revision));
    }
}
