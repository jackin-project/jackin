// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

    let identity =
        |directory| profile_credential_source_identity(Agent::Hermes, "fixture", directory, None);
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
