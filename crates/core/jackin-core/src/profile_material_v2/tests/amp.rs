// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{Agent, ProfileCredentialV2Error, profile_credential_material_revision_v2};

#[test]
fn fixed_amp_material_vector_matches() {
    let raw = br#"{"apiKey@https://ampcode.com/":"token"}"#;
    assert!(matches!(
        profile_credential_material_revision_v2(Agent::Amp, raw),
        Ok(revision) if revision.as_str() == "dba4d76bf0817f0d606c2146e3c4de22a28e729a09331b458b204b7b0c95951e"
    ));
}

#[test]
fn amp_alias_and_canonical_key_project_to_the_same_exact_token() {
    let canonical = profile_credential_material_revision_v2(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":" token "}"#,
    )
    .expect("canonical Amp credential is valid");
    let alias = profile_credential_material_revision_v2(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com":" token "}"#,
    )
    .expect("Amp alias is valid");
    assert_eq!(canonical, alias);

    let trimmed = profile_credential_material_revision_v2(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":"token"}"#,
    )
    .expect("trimmed-token Amp fixture is valid");
    assert_ne!(canonical, trimmed);
}

#[test]
fn amp_requires_one_nonblank_string_and_rejects_both_aliases() {
    let invalid: &[&[u8]] = &[
        b"{}",
        br#"{"apiKey@https://ampcode.com/":null}"#,
        br#"{"apiKey@https://ampcode.com/":7}"#,
        br#"{"apiKey@https://ampcode.com/":""}"#,
        br#"{"apiKey@https://ampcode.com/":" \t "}"#,
        br#"{"apiKey@https://ampcode.com/":"same","apiKey@https://ampcode.com":"same"}"#,
    ];

    for raw in invalid {
        assert_eq!(
            profile_credential_material_revision_v2(Agent::Amp, raw).err(),
            Some(ProfileCredentialV2Error::InvalidAmpCredential),
            "{raw:?}"
        );
    }
}

#[test]
fn amp_ignores_valid_unrelated_metadata_but_rejects_duplicate_keys_anywhere() {
    let first = profile_credential_material_revision_v2(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":"token","foreign":{"slot":1}}"#,
    )
    .expect("first valid Amp metadata fixture must produce a revision");
    let second = profile_credential_material_revision_v2(
        Agent::Amp,
        br#"{"foreign":{"slot":2},"apiKey@https://ampcode.com/":"token"}"#,
    )
    .expect("second valid Amp metadata fixture must produce a revision");
    assert_eq!(first, second);

    let duplicate = br#"{"apiKey@https://ampcode.com/":"token","foreign":{"x":1,"\u0078":2}}"#;
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Amp, duplicate).err(),
        Some(ProfileCredentialV2Error::DuplicateKey)
    );
}

#[test]
fn amp_errors_do_not_echo_token_or_field_canaries() {
    let canary = "AMP_PROFILE_V2_SECRET_CANARY";
    let raw = format!(
        "{{\"apiKey@https://ampcode.com/\":\"{canary}\",\"apiKey@https://ampcode.com\":\"x\"}}"
    );
    let Err(error) = profile_credential_material_revision_v2(Agent::Amp, raw.as_bytes()) else {
        panic!("ambiguous Amp credential unexpectedly succeeded");
    };
    assert_eq!(error, ProfileCredentialV2Error::InvalidAmpCredential);
    assert!(!error.to_string().contains(canary));
    assert!(!format!("{error:?}").contains(canary));
}

#[test]
fn omp_material_stays_opaque_and_uses_the_v2_binary_domain() {
    assert!(matches!(
        profile_credential_material_revision_v2(Agent::Omp, b"\x00opaque\xff"),
        Ok(revision) if revision.as_str() == "f433856f6e0cef2fad9ac532816a5efaa5e141151ee48b94655cb274c75a4f80"
    ));
    profile_credential_material_revision_v2(Agent::Omp, b"")
        .expect("empty OMP bytes must be accepted");
    profile_credential_material_revision_v2(Agent::Omp, b"not json {\xff")
        .expect("opaque OMP bytes must be accepted");
    let omp_one = profile_credential_material_revision_v2(Agent::Omp, b"one")
        .expect("opaque OMP bytes must produce a revision");
    let omp_two = profile_credential_material_revision_v2(Agent::Omp, b"two")
        .expect("opaque OMP bytes must produce a revision");
    assert_ne!(omp_one, omp_two);
    let omp_json_looking = profile_credential_material_revision_v2(Agent::Omp, b"null")
        .expect("JSON-looking OMP bytes remain opaque");
    let codex_json = profile_credential_material_revision_v2(Agent::Codex, b"null")
        .expect("null is valid Codex JSON");
    assert_ne!(omp_json_looking, codex_json);
}

#[test]
fn valid_amp_token_does_not_skip_value_budget_checks_in_ignored_metadata() {
    let mut raw = String::from("{\"apiKey@https://ampcode.com/\":\"token\",\"ignored\":[");
    for index in 0..65_534 {
        if index != 0 {
            raw.push(',');
        }
        raw.push_str("null");
    }
    raw.push_str("]}");
    assert_eq!(
        profile_credential_material_revision_v2(Agent::Amp, raw.as_bytes()).err(),
        Some(ProfileCredentialV2Error::TooManyValues)
    );
}
