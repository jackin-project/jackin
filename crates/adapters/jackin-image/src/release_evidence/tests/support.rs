// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Synthetic typed receipts for policy/API tests, never cryptographic evidence.

use super::*;
use velnor_actions_contract::package_release::PackageReleaseChannel;

pub(super) fn receipt(channel: PackageReleaseChannel) -> PackageReleaseReceipt {
    let mut value: Value =
        serde_json::from_slice(include_bytes!("../fixtures/jackin-policy-receipt.json")).unwrap();
    if channel == PackageReleaseChannel::Stable {
        value["channel"] = serde_json::json!("stable");
        value["tag"] = serde_json::json!("v0.6.4");
        // Stable workflow authority is the released source itself.
        value["workflow"]["authority_sha"] = value["source"]["commit_sha"].clone();
        value["source"]["event"] = serde_json::json!("workflow_dispatch");
        value["version"]["value"] = serde_json::json!("0.6.4");
        value["version"]["commit_count"] = Value::Null;
        value["version"]["source_sha7"] = Value::Null;
        value["workflow"]["path"] = serde_json::json!(channel.workflow_path());
        value["workflow"]["signer_workflow"] = serde_json::json!(channel.workflow_path());
        value["workflow"]["signer_job_id"] = serde_json::json!(channel.signer_job_id());
        value["evidence"]["signer_workflow_ref"] = serde_json::json!(format!(
            "jackin-project/jackin/{}@refs/heads/main",
            channel.workflow_path()
        ));
    }
    let receipt: PackageReleaseReceipt = serde_json::from_value(value).unwrap();
    policy::validate_receipt(&receipt, &receipt.version.value).unwrap();
    receipt
}

pub(super) fn verifier() -> Verifier {
    Verifier::new(
        &sigstore_trust_root::TrustedRoot::from_embedded(
            sigstore_trust_root::SigstoreInstance::PublicGood,
        )
        .unwrap(),
    )
    .unwrap()
}
