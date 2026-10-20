// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{Asset, Release, digest, hex_sha, policy};
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;
use velnor_actions_contract::package_release::PackageReleaseReceipt;
use velnor_actions_contract::{
    canonical_json_bytes, parse_strict_json,
    publication_retry::rolling::{RollingAssetState, RollingJournal, RollingPhase, staged_name},
};

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    repository: String,
    repository_id: u64,
    release_id: u64,
    tag: String,
    journal: RollingJournal,
    predecessor: Option<serde_json::Value>,
}

pub(super) fn validate(
    release: &Release,
    receipt: &PackageReleaseReceipt,
    raw: &[u8],
    expected: &BTreeMap<String, (u64, String)>,
    live: &BTreeMap<String, Asset>,
) -> Result<()> {
    let body = release
        .body
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing rolling journal"))?;
    ensure!(body.len() <= 110 * 1024, "oversized rolling journal");
    let json = body
        .strip_prefix("velnor-rolling-preview-v1\n")
        .ok_or_else(|| anyhow::anyhow!("missing native rolling journal"))?;
    let envelope: Envelope = serde_json::from_value(parse_strict_json(json)?)?;
    ensure!(
        canonical_json_bytes(&envelope)? == json.as_bytes(),
        "noncanonical rolling journal"
    );
    let journal = &envelope.journal;
    let hashes: BTreeMap<_, _> = expected
        .iter()
        .map(|(name, (_, sha))| (name.clone(), sha.clone()))
        .collect();
    ensure!(
        envelope.schema == "velnor.rolling-preview-journal.v1"
            && envelope.repository == "jackin-project/jackin"
            && envelope.repository_id == receipt.repository.id
            && envelope.release_id == policy::PREVIEW_RELEASE_ID
            && envelope.tag == "preview"
            && envelope.predecessor.is_none()
            && journal.phase == RollingPhase::Committed
            && journal.obsolete.is_empty()
            && journal.identity.source_sha == receipt.source.commit_sha
            && journal.identity.receipt_digest == digest(raw)
            && journal.identity.assets == hashes
            && journal.original.run_id == receipt.workflow.run_id
            && journal.original.run_attempt == receipt.workflow.run_attempt
            && journal.original.artifact_id > 0
            && hex_sha(&journal.original.archive_sha256, 64)
            && journal.staged.keys().eq(expected.keys()),
        "rolling publication is not the signed committed generation"
    );
    for (name, capture) in &journal.staged {
        let actual = &live[name];
        ensure!(
            capture.id == actual.id
                && capture.name == staged_name(&journal.identity, name)
                && capture.state == RollingAssetState::Uploaded
                && capture.size == actual.size
                && capture.sha256.as_deref() == Some(expected[name].1.as_str()),
            "rolling committed asset identity mismatch"
        );
    }
    Ok(())
}
