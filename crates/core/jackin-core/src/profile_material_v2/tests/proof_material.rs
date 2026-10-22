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
}
