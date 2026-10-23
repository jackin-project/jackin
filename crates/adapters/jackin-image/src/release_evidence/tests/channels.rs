// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn stable_and_preview_have_disjoint_tag_workflow_and_event_authority() {
    let preview = receipt();
    let stable = channel_receipt(PackageReleaseChannel::Stable);
    for receipt in [preview, stable] {
        policy::validate_receipt(&receipt, &receipt.version.value).unwrap();
        policy::validate_ci_claims(&claims(&receipt), &receipt).unwrap();
        let (statement, subjects) = statement(&receipt);
        provenance::validate_statement(&statement, &receipt, &subjects).unwrap();
        assert_eq!(subjects.len(), 8);
        for wrong in [
            "push",
            "pull_request",
            if receipt.channel == PackageReleaseChannel::Stable {
                "workflow_run"
            } else {
                "workflow_dispatch"
            },
        ] {
            let mut invalid = claims(&receipt);
            invalid.build_trigger = Some(wrong.into());
            assert!(policy::validate_ci_claims(&invalid, &receipt).is_err());
        }
        let mut wrong = claims(&receipt);
        let other = if receipt.channel == PackageReleaseChannel::Preview {
            PackageReleaseChannel::Stable
        } else {
            PackageReleaseChannel::Preview
        };
        wrong.build_signer_uri = Some(policy::identity(other).into());
        assert!(policy::validate_ci_claims(&wrong, &receipt).is_err());
    }
}

#[test]
fn superseded_preview_and_version_tag_aliases_are_not_admitted() {
    let current = receipt();
    assert!(policy::validate_receipt(&current, "0.6.4-preview.2+abcdef0").is_err());
    let mut legacy = current.clone();
    legacy.tag = format!("v{}", legacy.version.value);
    assert!(policy::validate_receipt(&legacy, &legacy.version.value).is_err());
    let mut stable = channel_receipt(PackageReleaseChannel::Stable);
    stable.tag = "preview".into();
    assert!(policy::validate_receipt(&stable, "0.6.4").is_err());
    for tag in [
        "preview".to_owned(),
        format!("preview-{}", current.workflow.authority_sha),
        format!("preview-{}", &current.source.commit_sha[..7]),
    ] {
        let mut aliased = current.clone();
        aliased.tag = tag;
        assert!(
            policy::validate_receipt(&aliased, &aliased.version.value).is_err(),
            "{}",
            aliased.tag
        );
    }
}

#[test]
fn preview_signing_binds_workflow_authority_while_the_tag_binds_source() {
    let preview = receipt();
    assert_ne!(preview.source.commit_sha, preview.workflow.authority_sha);
    assert_eq!(
        preview.tag,
        format!("preview-{}", preview.source.commit_sha)
    );
    let mut source_signed = claims(&preview);
    source_signed.source_repository_digest = Some(preview.source.commit_sha.clone());
    assert!(policy::validate_ci_claims(&source_signed, &preview).is_err());
    let (mut statement, subjects) = statement(&preview);
    statement["predicate"]["buildDefinition"]["resolvedDependencies"][0]["digest"]["gitCommit"] =
        json!(preview.source.commit_sha);
    assert!(provenance::validate_statement(&statement, &preview, &subjects).is_err());
}

#[test]
fn signed_release_source_must_equal_the_native_application_source() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        let receipt = channel_receipt(channel);
        policy::validate_receipt_source(
            &receipt,
            &receipt.version.value,
            &receipt.source.commit_sha,
        )
        .unwrap();
        policy::validate_receipt_source(&receipt, &receipt.version.value, &"f".repeat(40))
            .unwrap_err();
    }
}
