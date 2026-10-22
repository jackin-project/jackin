// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    Agent, ProfileCredentialMaterialProofRef, ProfileCredentialProofComparison,
    ProfileCredentialSourceIdentity, ProfileCredentialSourceMaterial,
    ProfileCredentialSourceMaterialV2, ProfileCredentialV2Error,
    compare_profile_credential_material_proofs, profile_credential_material_revision_v2,
    profile_credential_source_identity_v2, profile_credential_source_material_v2,
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

fn v1_material(
    agent: Agent,
    descriptor_fingerprint: &str,
    material_revision: &str,
) -> ProfileCredentialSourceMaterial {
    ProfileCredentialSourceMaterial {
        source: ProfileCredentialSourceIdentity {
            agent,
            descriptor_fingerprint: descriptor_fingerprint.to_owned(),
        },
        material_revision: material_revision.to_owned(),
    }
}

fn compare(
    left: ProfileCredentialMaterialProofRef<'_>,
    right: ProfileCredentialMaterialProofRef<'_>,
) -> Result<ProfileCredentialProofComparison, ProfileCredentialV2Error> {
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
        compare(
            ProfileCredentialMaterialProofRef::V2(&first),
            ProfileCredentialMaterialProofRef::V2(&same)
        ),
        Ok(ProfileCredentialProofComparison::Equal)
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
        compare(
            ProfileCredentialMaterialProofRef::V2(&first),
            ProfileCredentialMaterialProofRef::V2(&same_hashes_other_agent)
        ),
        Ok(ProfileCredentialProofComparison::Different)
    );
    let other_material = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/main",
        br#"{"token":"b"}"#,
    );
    for changed in [&other_source, &other_agent, &other_material] {
        assert_eq!(
            compare(
                ProfileCredentialMaterialProofRef::V2(&first),
                ProfileCredentialMaterialProofRef::V2(changed)
            ),
            Ok(ProfileCredentialProofComparison::Different)
        );
    }
}

#[test]
fn v1_comparison_validates_strings_then_compares_complete_proof() {
    let digest = "0123456789abcdef".repeat(4);
    let revision = "fedcba9876543210".repeat(4);
    let first = v1_material(Agent::Codex, &digest, &revision);
    let same = v1_material(Agent::Codex, &digest, &revision);
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&first),
            ProfileCredentialMaterialProofRef::V1(&same)
        ),
        Ok(ProfileCredentialProofComparison::Equal)
    );

    let other_agent = v1_material(Agent::Claude, &digest, &revision);
    let other_source = v1_material(Agent::Codex, &revision, &revision);
    let other_revision = v1_material(Agent::Codex, &digest, &digest);
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&first),
            ProfileCredentialMaterialProofRef::V1(&other_agent)
        ),
        Ok(ProfileCredentialProofComparison::Different)
    );
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&first),
            ProfileCredentialMaterialProofRef::V1(&other_source)
        ),
        Ok(ProfileCredentialProofComparison::Different)
    );
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&first),
            ProfileCredentialMaterialProofRef::V1(&other_revision)
        ),
        Ok(ProfileCredentialProofComparison::Different)
    );

    let invalid_source = v1_material(Agent::Codex, "not-a-fingerprint", &revision);
    let invalid_revision = v1_material(Agent::Codex, &digest, &"A".repeat(64));
    let debug = format!(
        "{:?}",
        ProfileCredentialMaterialProofRef::V1(&invalid_revision)
    );
    assert!(!debug.contains(&"A".repeat(64)));
    for invalid in [&invalid_source, &invalid_revision] {
        assert_eq!(
            compare(
                ProfileCredentialMaterialProofRef::V1(invalid),
                ProfileCredentialMaterialProofRef::V1(&first)
            ),
            Err(ProfileCredentialV2Error::InvalidFingerprint)
        );
        assert_eq!(
            compare(
                ProfileCredentialMaterialProofRef::V1(&first),
                ProfileCredentialMaterialProofRef::V1(invalid)
            ),
            Err(ProfileCredentialV2Error::InvalidFingerprint)
        );
    }
}

#[test]
fn identical_cross_scheme_fingerprints_require_revalidation_in_both_orders() {
    let v2 = v2_material(
        Agent::Codex,
        "openai",
        "/profiles/main",
        br#"{"token":"a"}"#,
    );
    let v1 = v1_material(
        v2.source().agent(),
        v2.source().descriptor_fingerprint().as_str(),
        v2.material_revision().as_str(),
    );
    let debug = format!("{:?}", ProfileCredentialMaterialProofRef::V2(&v2));
    assert!(!debug.contains(v2.source().descriptor_fingerprint().as_str()));
    assert!(!debug.contains(v2.material_revision().as_str()));

    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&v1),
            ProfileCredentialMaterialProofRef::V2(&v2)
        ),
        Ok(ProfileCredentialProofComparison::RevalidationRequired)
    );
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V2(&v2),
            ProfileCredentialMaterialProofRef::V1(&v1)
        ),
        Ok(ProfileCredentialProofComparison::RevalidationRequired)
    );

    let invalid_v1 = v1_material(Agent::Codex, "invalid", "invalid");
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V1(&invalid_v1),
            ProfileCredentialMaterialProofRef::V2(&v2)
        ),
        Err(ProfileCredentialV2Error::InvalidFingerprint)
    );
    assert_eq!(
        compare(
            ProfileCredentialMaterialProofRef::V2(&v2),
            ProfileCredentialMaterialProofRef::V1(&invalid_v1)
        ),
        Err(ProfileCredentialV2Error::InvalidFingerprint)
    );
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
