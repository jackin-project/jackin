// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    Agent, ProfileCredentialProofComparison, ProfileCredentialSourceMaterialV2,
    ProfileCredentialV2Error, compare_profile_credential_material_proofs,
    profile_credential_material_revision_v2, profile_credential_source_identity_v2,
    profile_credential_source_material_v2,
};

fn v2_material(
    agent: Agent,
    provider: &str,
    path: &str,
    raw: &[u8],
) -> ProfileCredentialSourceMaterialV2 {
    let source =
        profile_credential_source_identity_v2(agent, provider, Path::new(path), Some(b"null"))
            .expect("valid source identity should be constructed");
    profile_credential_source_material_v2(source, raw)
        .expect("valid source material should be constructed")
}

fn compare(
    left: &ProfileCredentialSourceMaterialV2,
    right: &ProfileCredentialSourceMaterialV2,
) -> ProfileCredentialProofComparison {
    compare_profile_credential_material_proofs(left, right)
}

#[test]
fn v2_comparison_checks_source_agent_and_material() {
    let first = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/main",
        br#"{"token":"a"}"#,
    );
    let same = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/main",
        br#"{"token":"a"}"#,
    );
    assert_eq!(
        compare(&first, &same),
        ProfileCredentialProofComparison::Equal
    );

    let other_source = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/other",
        br#"{"token":"a"}"#,
    );
    let other_agent = v2_material(
        Agent::Claude,
        "openai",
        "/profiles/main",
        br#"{"token":"a"}"#,
    );
    let serialized = serde_json::to_string(&first).expect("V2 proof serializes");
    let changed_agent_wire = serialized.replace("\"agent\":\"codex\"", "\"agent\":\"claude\"");
    assert_ne!(serialized, changed_agent_wire);
    let same_hashes_other_agent =
        serde_json::from_str::<ProfileCredentialSourceMaterialV2>(&changed_agent_wire)
            .expect("valid tagged proof with a changed agent should deserialize");
    assert_eq!(
        first.source().descriptor_fingerprint(),
        same_hashes_other_agent.source().descriptor_fingerprint()
    );
    assert_eq!(
        first.material_revision(),
        same_hashes_other_agent.material_revision()
    );
    assert_eq!(
        compare(&first, &same_hashes_other_agent),
        ProfileCredentialProofComparison::Different
    );
    let other_material = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/main",
        br#"{"token":"b"}"#,
    );
    for changed in [&other_source, &other_agent, &other_material] {
        assert_eq!(
            compare(&first, changed),
            ProfileCredentialProofComparison::Different
        );
    }
}

#[test]
fn source_material_wrapper_uses_amp_projection_and_keeps_omp_bytes_opaque() {
    let amp_canonical = br#"{"apiKey@https://ampcode.com/":" token "}"#;
    let amp_alias = br#"{"apiKey@https://ampcode.com":" token "}"#;
    let amp_expected = profile_credential_material_revision_v2(Agent::Amp, amp_canonical)
        .expect("unambiguous Amp token should have a revision");
    assert_eq!(
        amp_expected,
        profile_credential_material_revision_v2(Agent::Amp, amp_alias)
            .expect("unambiguous Amp alias should have a revision")
    );
    let amp_source =
        profile_credential_source_identity_v2(Agent::Amp, "amp", Path::new("/profiles/amp"), None)
            .expect("Amp source identity should be constructed");
    let amp_material = profile_credential_source_material_v2(amp_source, amp_canonical)
        .expect("Amp source material should use the projected token");
    assert_eq!(amp_material.material_revision(), &amp_expected);

    let invalid_amp_source =
        profile_credential_source_identity_v2(Agent::Amp, "amp", Path::new("/profiles/amp"), None)
            .expect("Amp source identity should be constructed");
    let amp_error = profile_credential_source_material_v2(invalid_amp_source, br"{}")
        .expect_err("invalid Amp token payload should be rejected");
    assert_eq!(amp_error, ProfileCredentialV2Error::InvalidAmpCredential);

    let opaque = b"\0{\"looks\":\"json\"}\xff";
    let omp_expected = profile_credential_material_revision_v2(Agent::Omp, opaque)
        .expect("Omp material is opaque binary");
    let omp_source =
        profile_credential_source_identity_v2(Agent::Omp, "omp", Path::new("/profiles/omp"), None)
            .expect("Omp source identity should be constructed");
    let omp_material = profile_credential_source_material_v2(omp_source, opaque)
        .expect("Omp source material should keep raw bytes opaque");
    assert_eq!(omp_material.material_revision(), &omp_expected);
}
