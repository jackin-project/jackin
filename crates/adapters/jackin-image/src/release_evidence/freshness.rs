// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Public observations supplement already authenticated receipt bytes; they never
//! replace Sigstore verification or select a different requested helper version.
//! Historical repository and exact-attempt build evidence is admitted once per
//! pin. Mutable release lifecycle, inventory and source-bound tag facts remain fresh at every
//! execution and installation boundary; a new download must admit its own pin.
//! Repository ownership/privacy/default-branch admission is invocation-local, not
//! an immutability claim. Rechecks require the initially pinned positive release
//! ID and complete snapshot; moving a repository does not revoke verified bytes.

mod runs;
#[cfg(test)]
mod tests;

use super::{EVIDENCE_LIMIT, RECEIPT_LIMIT, digest, policy, transport::Transport};
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use velnor_actions_contract::package_release::{PackageReleaseChannel, PackageReleaseReceipt};

const API: &str = "https://api.github.com/repos/jackin-project/jackin";
const RECOVERY_ENVELOPE_LIMIT: usize = 110 * 1024;

#[derive(Debug)]
pub(super) struct PublicationPin {
    receipt: PackageReleaseReceipt,
    receipt_bytes: Vec<u8>,
    bundle_bytes: Vec<u8>,
    snapshot: Snapshot,
}

impl PublicationPin {
    pub(super) async fn recheck(&self, transport: &impl Transport) -> Result<()> {
        let base = download_base(&self.receipt);
        ensure!(
            transport
                .get(&format!("{base}/release.json"), RECEIPT_LIMIT)
                .await?
                == self.receipt_bytes,
            "published release receipt changed; refusing superseded remote helper"
        );
        ensure!(
            transport
                .get(&format!("{base}/release.json.bundle"), EVIDENCE_LIMIT)
                .await?
                == self.bundle_bytes,
            "published release receipt signature changed"
        );
        ensure!(
            observe(
                transport,
                &self.receipt,
                &self.receipt_bytes,
                &self.bundle_bytes
            )
            .await?
                == self.snapshot,
            "release source, tag or inventory changed during capsule download"
        );
        Ok(())
    }
}

/// Called only after the parent authenticates the receipt and all capsule proofs.
pub(super) async fn pin(
    transport: &impl Transport,
    receipt: &PackageReleaseReceipt,
    receipt_bytes: &[u8],
    bundle_bytes: &[u8],
) -> Result<Box<PublicationPin>> {
    runs::repository(transport).await?;
    runs::validate(transport, receipt).await?;
    let snapshot = observe(transport, receipt, receipt_bytes, bundle_bytes).await?;
    Ok(Box::new(PublicationPin {
        receipt: receipt.clone(),
        receipt_bytes: receipt_bytes.to_vec(),
        bundle_bytes: bundle_bytes.to_vec(),
        snapshot,
    }))
}

fn download_base(receipt: &PackageReleaseReceipt) -> String {
    format!("{}/releases/download/{}", policy::REPOSITORY, receipt.tag)
}

async fn read<T: serde::de::DeserializeOwned>(
    transport: &impl Transport,
    route: &str,
) -> Result<T> {
    Ok(serde_json::from_slice(
        &transport
            .get(&format!("{API}/{route}"), RECEIPT_LIMIT)
            .await?,
    )?)
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Release {
    id: u64,
    tag_name: String,
    draft: Option<bool>,
    prerelease: Option<bool>,
    immutable: Option<bool>,
    body: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Asset {
    id: u64,
    name: String,
    state: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    release: Release,
    assets: BTreeMap<String, Asset>,
    tag: Tag,
}

#[derive(Debug, PartialEq, Eq)]
struct Tag {
    object_sha: String,
    source_sha: String,
}

#[derive(Deserialize)]
struct Object {
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "ref")]
    name: String,
    object: Object,
}
#[derive(Deserialize)]
struct AnnotatedTag {
    sha: String,
    tag: String,
    message: Option<String>,
    object: Object,
}

async fn tag(
    transport: &impl Transport,
    receipt: &PackageReleaseReceipt,
    raw: &[u8],
    bundle: &[u8],
) -> Result<Tag> {
    let reference: Reference = read(transport, &format!("git/ref/tags/{}", receipt.tag)).await?;
    ensure!(
        reference.name == format!("refs/tags/{}", receipt.tag)
            && hex_sha(&reference.object.sha, 40),
        "release tag reference mismatch"
    );
    let source_sha = match reference.object.kind.as_str() {
        "tag" => {
            let annotated: AnnotatedTag =
                read(transport, &format!("git/tags/{}", reference.object.sha)).await?;
            ensure!(
                annotated.sha == reference.object.sha
                    && annotated.tag == receipt.tag
                    && annotated.object.kind == "commit",
                "release annotated tag mismatch"
            );
            if receipt.channel == PackageReleaseChannel::Preview {
                let expected_message = recovery_envelope(raw, bundle)?;
                ensure!(
                    annotated.message.as_deref() == Some(expected_message.as_str()),
                    "preview annotated tag recovery envelope mismatch"
                );
            }
            annotated.object.sha
        }
        _ => anyhow::bail!("unsupported release tag object"),
    };
    ensure!(
        source_sha == receipt.source.commit_sha,
        "release tag no longer selects signed source"
    );
    Ok(Tag {
        object_sha: reference.object.sha,
        source_sha,
    })
}

fn hex_sha(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}

fn expected_assets(
    receipt: &PackageReleaseReceipt,
    raw: &[u8],
    bundle: &[u8],
) -> BTreeMap<String, (u64, String)> {
    let mut expected: BTreeMap<_, _> = receipt
        .assets
        .iter()
        .map(|asset| (asset.name.clone(), (asset.size, asset.sha256.clone())))
        .collect();
    expected.insert("release.json".into(), (raw.len() as u64, digest(raw)));
    expected.insert(
        "release.json.bundle".into(),
        (bundle.len() as u64, digest(bundle)),
    );
    expected
}

fn assets(
    rows: Vec<Asset>,
    expected: &BTreeMap<String, (u64, String)>,
) -> Result<BTreeMap<String, Asset>> {
    ensure!(
        rows.len() == expected.len() && rows.len() < 100,
        "release asset inventory changed"
    );
    let mut ids = BTreeSet::new();
    let mut result = BTreeMap::new();
    for row in rows {
        let (size, sha) = expected
            .get(&row.name)
            .ok_or_else(|| anyhow::anyhow!("unexpected release asset"))?;
        ensure!(
            row.id > 0
                && ids.insert(row.id)
                && row.state == "uploaded"
                && row.size == *size
                && row.size > 0
                && row.digest.as_deref() == Some(format!("sha256:{sha}").as_str()),
            "release asset identity mismatch"
        );
        ensure!(
            result.insert(row.name.clone(), row).is_none(),
            "duplicate release asset name"
        );
    }
    Ok(result)
}

async fn observe(
    transport: &impl Transport,
    receipt: &PackageReleaseReceipt,
    raw: &[u8],
    bundle: &[u8],
) -> Result<Snapshot> {
    let release: Release = read(transport, &format!("releases/tags/{}", receipt.tag)).await?;
    ensure!(
        release.id > 0 && release.tag_name == receipt.tag && release.draft == Some(false),
        "release identity or draft state mismatch"
    );
    match receipt.channel {
        PackageReleaseChannel::Preview => {
            ensure!(
                release.prerelease == Some(true) && release.immutable == Some(true),
                "preview release must be permanent and immutable"
            );
            let expected_body = recovery_envelope(raw, bundle)?;
            ensure!(
                release.body.as_deref() == Some(expected_body.as_str()),
                "preview release body recovery envelope mismatch"
            );
        }
        PackageReleaseChannel::Stable => ensure!(
            release.prerelease == Some(false) && release.immutable == Some(true),
            "stable release must be permanent and immutable"
        ),
    }
    let expected = expected_assets(receipt, raw, bundle);
    let rows: Vec<Asset> = read(
        transport,
        &format!("releases/{}/assets?per_page=100&page=1", release.id),
    )
    .await?;
    let assets = assets(rows, &expected)?;
    let tag = tag(transport, receipt, raw, bundle).await?;
    let final_release: Release = read(transport, &format!("releases/tags/{}", receipt.tag)).await?;
    ensure!(
        final_release == release,
        "release lifecycle changed during observation"
    );
    let final_tag = self::tag(transport, receipt, raw, bundle).await?;
    ensure!(final_tag == tag, "release tag changed during observation");
    Ok(Snapshot {
        release,
        assets,
        tag,
    })
}

pub(super) fn recovery_envelope(receipt: &[u8], bundle: &[u8]) -> Result<String> {
    let digest = digest(receipt);
    let header = format!("velnor-package-release-recovery-v1\nreceipt_sha256={digest}\nreceipt=");
    let middle = "\nbundle=";
    let encoded_receipt_len = unpadded_base64_len(receipt.len());
    let encoded_bundle_len = unpadded_base64_len(bundle.len());
    ensure!(
        header.len() + encoded_receipt_len + middle.len() + encoded_bundle_len
            < RECOVERY_ENVELOPE_LIMIT,
        "preview recovery envelope exceeds native limit"
    );
    Ok(format!(
        "velnor-package-release-recovery-v1\nreceipt_sha256={digest}\nreceipt={}\nbundle={}\n",
        URL_SAFE_NO_PAD.encode(receipt),
        URL_SAFE_NO_PAD.encode(bundle),
    ))
}

fn unpadded_base64_len(bytes: usize) -> usize {
    (bytes / 3) * 4
        + match bytes % 3 {
            1 => 2,
            2 => 3,
            _ => 0,
        }
}
