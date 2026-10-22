// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde_json::Value;

use std::path::Path;

use crate::{
    Agent, ProfileCredentialV2Error, profile_credential_material_revision_v2,
    profile_credential_source_identity_v2,
};

use super::super::json::parse_json;

#[test]
fn accepts_a_small_json_grammar_corpus_and_matches_serde_values() {
    let corpus: &[&[u8]] = &[
        b"null",
        b" true ",
        b"false",
        b"0",
        b"-0",
        b"12345678901234567890",
        b"1.25e-10",
        br#""plain string""#,
        br#""escaped quote: \" and slash: \\""#,
        br#""\ud834\udd1e""#,
        r#""raw unicode: 你好""#.as_bytes(),
        b"[]",
        b"[null, true, {\"k\": [1, 2.5]}]",
        b"{}",
        b"{\"a\":null,\"b\":false}",
        b"{ \"a\" : { \"b\" : \"c\" } }",
    ];

    for raw in corpus {
        let serde_accepts = serde_json::from_slice::<Value>(raw).is_ok();
        let v2_accepts = parse_json(raw).is_ok();
        assert_eq!(v2_accepts, serde_accepts, "{raw:?}");
        assert!(v2_accepts, "{raw:?}");
    }
}

#[test]
fn rejects_incomplete_ambiguous_or_non_json_structure() {
    let invalid: &[&[u8]] = &[
        b"",
        b" \n\t ",
        b"null false",
        b"[1 2]",
        b"[1,]",
        b"{\"a\" 1}",
        b"{\"a\":1 \"b\":2}",
        b"{\"a\":1,}",
        b"{a:1}",
        b"[1}",
        b"{\"a\":[1}",
        b"[1]]",
        b"\"unterminated",
        b"/* comment */ null",
        b"// comment\nnull",
        b"\x0bnull",
        b"\xef\xbb\xbfnull",
        b"\"raw\x01control\"",
        br#""\q""#,
        br#""\ud800""#,
        b"\"\xff\"",
    ];

    for raw in invalid {
        assert_eq!(
            parse_json(raw).err(),
            Some(ProfileCredentialV2Error::InvalidJson),
            "{raw:?}"
        );
    }
}

#[test]
fn rejects_duplicate_decoded_keys_at_root_nested_and_inside_arrays() {
    let duplicates: &[&[u8]] = &[
        br#"{"x":1,"x":2}"#,
        br#"{"outer":{"x":1,"x":2}}"#,
        br#"[{"x":1,"x":2}]"#,
        br#"{"x":1,"\u0078":2}"#,
    ];

    for raw in duplicates {
        assert_eq!(
            parse_json(raw).err(),
            Some(ProfileCredentialV2Error::DuplicateKey)
        );
        assert_eq!(
            profile_credential_material_revision_v2(Agent::Codex, raw).err(),
            Some(ProfileCredentialV2Error::DuplicateKey)
        );
    }
}

#[test]
fn decoded_escaped_key_and_string_spellings_are_equivalent() {
    let first = profile_credential_material_revision_v2(Agent::Codex, br#"{"a":"x"}"#)
        .expect("plain spelling is valid JSON");
    let second =
        profile_credential_material_revision_v2(Agent::Codex, br#"{ "\u0061" : "\u0078" }"#)
            .expect("escaped spelling is valid JSON");
    assert_eq!(first, second);

    let different_value = profile_credential_material_revision_v2(Agent::Codex, br#"{"a":"y"}"#)
        .expect("different string value is valid JSON");
    let number = profile_credential_material_revision_v2(Agent::Codex, br#"{"a":1}"#)
        .expect("number fixture is valid JSON");
    let numeric_string = profile_credential_material_revision_v2(Agent::Codex, br#"{"a":"1"}"#)
        .expect("numeric-looking string fixture is valid JSON");
    assert_ne!(first, different_value);
    assert_ne!(number, numeric_string);
}

#[test]
fn parser_errors_do_not_echo_untrusted_canaries() {
    let canary = "PROFILE_V2_GRAMMAR_CANARY";
    let raw = format!("{{\"value\": {canary}}}");
    let Err(error) = profile_credential_material_revision_v2(Agent::Codex, raw.as_bytes()) else {
        panic!("malformed JSON with a bare canary unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::InvalidJson);
    assert!(!error.to_string().contains(canary));
    assert!(!format!("{error:?}").contains(canary));

    let malformed_number = br#"{"field-canary":"value-canary","n":1eCANARY}"#;
    let Err(error) = profile_credential_material_revision_v2(Agent::Codex, malformed_number) else {
        panic!("malformed numeric JSON unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::InvalidJson);
    assert!(!error.to_string().contains("canary"));
    assert!(!format!("{error:?}").contains("canary"));

    let malformed_string = format!(r#"{{"bad":"{canary}\q"}}"#);
    let Err(error) =
        profile_credential_material_revision_v2(Agent::Codex, malformed_string.as_bytes())
    else {
        panic!("malformed JSON string with a canary unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::InvalidJson);
    assert!(!error.to_string().contains(canary));
    assert!(!format!("{error:?}").contains(canary));

    let duplicate_key = br#"{"CANARY_KEY":"CANARY_VALUE","CANARY_KEY":"other"}"#;
    let Err(error) = profile_credential_material_revision_v2(Agent::Codex, duplicate_key) else {
        panic!("duplicate-key JSON unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::DuplicateKey);
    assert!(!error.to_string().contains("CANARY_KEY"));
    assert!(!format!("{error:?}").contains("CANARY_VALUE"));
}

#[test]
fn raw_selectors_validate_grammar_for_every_agent_including_omp() {
    for agent in Agent::ALL {
        assert_eq!(
            profile_credential_source_identity_v2(
                *agent,
                "provider",
                Path::new("/profiles/main"),
                Some(br#"{"x":1,"x":2}"#),
            )
            .err(),
            Some(ProfileCredentialV2Error::DuplicateKey)
        );
    }
}
