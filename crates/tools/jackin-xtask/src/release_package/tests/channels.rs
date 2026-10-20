// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn channel_request(assemble: bool, channel: PackageReleaseChannel) -> TaskRequest {
    let mut handoff = request(assemble);
    handoff.request.plan.channel = channel;
    handoff.request.plan_sha256 = release_plan_sha256(&handoff.request.plan).unwrap();
    if channel == PackageReleaseChannel::Stable {
        handoff.request.context.commit_count = 0;
        handoff.request.context.version = env!("CARGO_PKG_VERSION").to_owned();
    }
    handoff
}

#[test]
fn both_channels_accept_only_their_complete_context_and_inventory() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        for assemble in [false, true] {
            let source = tempfile::tempdir().unwrap();
            let mut handoff = channel_request(assemble, channel);
            if assemble {
                populate(&mut handoff, source.path());
            }
            let bytes = canonical_json_bytes(&handoff).unwrap();
            let admitted = handoff::parse(&bytes).unwrap();
            assert_eq!(canonical_json_bytes(&admitted).unwrap(), bytes);
            assert_eq!(admitted.request.plan.channel, channel);
        }
    }
}

#[test]
fn stable_assembly_preserves_six_payloads_and_capsule_version() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut handoff = channel_request(true, PackageReleaseChannel::Stable);
    populate(&mut handoff, source.path());
    let admitted = handoff::parse(&canonical_json_bytes(&handoff).unwrap()).unwrap();
    assembly::assemble(output.path(), &admitted).unwrap();
    inventory::write_complete(output.path(), &admitted.request).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(output.path().join("package-output.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["payloads"].as_array().unwrap().len(), 6);
    assert_eq!(manifest["sboms"].as_array().unwrap().len(), 6);
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    let companion: Value =
        serde_json::from_slice(&fs::read(output.path().join("capsule-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(companion["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(companion["targets"].as_object().unwrap().len(), 2);
}

#[test]
fn mixed_channels_old_schema_and_invalid_policy_digest_fail_closed() {
    let mut preview = channel_request(false, PackageReleaseChannel::Preview);
    preview.request.plan.channel = PackageReleaseChannel::Stable;
    preview.request.plan_sha256 = release_plan_sha256(&preview.request.plan).unwrap();
    handoff::parse(&canonical_json_bytes(&preview).unwrap()).unwrap_err();
    let mut stable = channel_request(false, PackageReleaseChannel::Stable);
    stable.request.plan.channel = PackageReleaseChannel::Preview;
    stable.request.plan_sha256 = release_plan_sha256(&stable.request.plan).unwrap();
    handoff::parse(&canonical_json_bytes(&stable).unwrap()).unwrap_err();
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        let mut old = channel_request(false, channel);
        old.schema = "velnor.preview-task.v1".to_owned();
        handoff::parse(&canonical_json_bytes(&old).unwrap()).unwrap_err();
        let mut malformed = channel_request(false, channel);
        malformed.request.policy_sha256 = "unknown".to_owned();
        handoff::parse(&canonical_json_bytes(&malformed).unwrap()).unwrap_err();
    }
}
