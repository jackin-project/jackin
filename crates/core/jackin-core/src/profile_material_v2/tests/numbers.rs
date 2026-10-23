// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{Agent, profile_credential_material_revision_v2};

#[test]
fn fixed_independent_lexical_number_vectors_match() {
    let vectors = [
        (
            "1",
            "2661af470182e28d1b84947501eada803a466111097455cb96b41f46f4813d4f",
        ),
        (
            "1.0",
            "4753b95cd745787d1dc5feb4d21b54f0d8f00935b9b595abeb49aac37b4f0c78",
        ),
        (
            "1e0",
            "71fb68fa02ee9094bc3a1f111d1e9fe56be020cd195a5fdd52eb8859455eb0a0",
        ),
        (
            "1E+0",
            "3afab45b66b0c87b4250854547d72fb21288763f1416a70d8edfc395eeeffbbc",
        ),
        (
            "-0",
            "9658d19c9438d66150e02eef20fab192a18fc8de3c102dc2aa7abbcd71b63e2c",
        ),
        (
            "18446744073709551616",
            "5c4b584f62b626ed8c271eebee50a1b60a0363a9852f3d9be58cd9aaa894906c",
        ),
        (
            "18446744073709551617",
            "4fd1548f3969d693e4db92945ead38bc6ec6b4438bc2731c6f78688bf8fc332c",
        ),
        (
            "1e999999",
            "4f9a627c5feff0db24d8c98970abe6a4ff2019714b86c5eb502da5dccc79720c",
        ),
    ];

    for (number, expected) in vectors {
        let raw = format!("{{\"a\":{number}}}");
        assert!(
            matches!(profile_credential_material_revision_v2(Agent::Codex, raw.as_bytes()), Ok(actual) if actual.as_str() == expected),
            "number token {number}"
        );
    }
}

#[test]
fn preserves_raw_number_tokens_at_root_and_nested_depth() {
    for raw in [
        b"0".as_slice(),
        b"-0".as_slice(),
        b"1.23456789012345678901234567890".as_slice(),
        b"1e999999999999999999999999999999999999999999".as_slice(),
        br#"{"outer":[-0,1.0,1e0,1E+0]}"#.as_slice(),
    ] {
        profile_credential_material_revision_v2(Agent::Codex, raw)
            .unwrap_or_else(|error| panic!("valid JSON number {raw:?} was rejected: {error:?}"));
    }
}

#[test]
fn accepts_long_legal_integer_and_exponent_without_number_conversion() {
    let long_integer = format!("1{}", "2".repeat(100_000));
    let long_exponent = format!("1e{}", "9".repeat(100_000));
    profile_credential_material_revision_v2(Agent::Codex, long_integer.as_bytes())
        .expect("long legal integer must be accepted without number conversion");
    profile_credential_material_revision_v2(Agent::Codex, long_exponent.as_bytes())
        .expect("long legal exponent must be accepted without number conversion");
}

#[test]
fn rejects_invalid_number_grammar() {
    for raw in [
        b"01".as_slice(),
        b"00".as_slice(),
        b"-".as_slice(),
        b"+1".as_slice(),
        b".1".as_slice(),
        b"1.".as_slice(),
        b"1e".as_slice(),
        b"1e+".as_slice(),
        b"NaN".as_slice(),
        b"Infinity".as_slice(),
        b"-Infinity".as_slice(),
        b"1x".as_slice(),
        b"[1junk]".as_slice(),
    ] {
        assert!(
            profile_credential_material_revision_v2(Agent::Codex, raw).is_err(),
            "{raw:?}"
        );
    }
}

#[test]
fn objects_ignore_order_and_whitespace_but_arrays_and_numbers_keep_order_and_spelling() {
    let first = profile_credential_material_revision_v2(Agent::Codex, br#"{"a":1,"b":["x",2]}"#)
        .expect("first object-order fixture is valid JSON");
    let equivalent = profile_credential_material_revision_v2(
        Agent::Codex,
        br#" { "b" : [ "x" , 2 ] , "a" : 1 } "#,
    )
    .expect("equivalent object-order fixture is valid JSON");
    assert_eq!(first, equivalent);

    let reordered_array =
        profile_credential_material_revision_v2(Agent::Codex, br#"{"a":1,"b":[2,"x"]}"#)
            .expect("reordered array fixture is valid JSON");
    let different_number =
        profile_credential_material_revision_v2(Agent::Codex, br#"{"a":1.0,"b":["x",2]}"#)
            .expect("different numeric spelling fixture is valid JSON");
    assert_ne!(first, reordered_array);
    assert_ne!(first, different_number);
}
