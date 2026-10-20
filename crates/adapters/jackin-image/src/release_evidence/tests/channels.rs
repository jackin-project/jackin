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
}
