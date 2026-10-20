// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Native channel capsule evidence. Trust starts at production Sigstore TUF and the
//! fixed Jackin release policy; neither receipt fields nor downloaded bundles
//! select trust roots or authorize a workflow.

mod freshness;
mod policy;
mod provenance;
mod transport;

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use sha2::{Digest as _, Sha256};
use sigstore_types::{Artifact, Bundle, MediaType, Sha256Hash, SignatureContent};
use sigstore_verify::{IdentityMatcher, VerificationPolicy, Verifier};
use velnor_actions_contract::package_release::{
    PackageReleaseAsset, PackageReleaseReceipt, parse_package_release_receipt,
};

pub(crate) use policy::{release_channel, release_tag};
use transport::{GithubTransport, Transport};

const RECEIPT_LIMIT: usize = 1024 * 1024;
const EVIDENCE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct VerifiedCapsule {
    pub(crate) sha256: String,
    pub(crate) size: u64,
    publication: Box<freshness::PublicationPin>,
}

pub(crate) async fn verify_release(version: &str, target: &str) -> Result<VerifiedCapsule> {
    release_channel(version)?;
    let transport = GithubTransport::new()?;
    let root = sigstore_trust_root::TrustedRoot::production().await
        .context("refreshing production Sigstore trust through TUF; refusing unauthenticated capsule download")?;
    let verifier = Verifier::new(&root)?;
    verify_with_transport(&transport, &verifier, version, target).await
}

pub(crate) async fn recheck_release(evidence: &VerifiedCapsule) -> Result<()> {
    evidence.publication.recheck(&GithubTransport::new()?).await
}

fn signing_policy(
    channel: velnor_actions_contract::package_release::PackageReleaseChannel,
) -> VerificationPolicy {
    VerificationPolicy::new(
        IdentityMatcher::Uri(policy::identity(channel).into()),
        policy::ISSUER,
    )
}

fn parse_bundle(bytes: &[u8]) -> Result<Bundle> {
    let bundle: Bundle =
        serde_json::from_slice(bytes).context("parsing standard Sigstore bundle")?;
    ensure!(
        bundle.media_type == MediaType::Bundle0_3,
        "capsule evidence requires standard Sigstore bundle v0.3"
    );
    sigstore_bundle::validate_bundle(&bundle).context("invalid Sigstore bundle structure")?;
    Ok(bundle)
}

fn asset<'a>(receipt: &'a PackageReleaseReceipt, name: &str) -> Result<&'a PackageReleaseAsset> {
    receipt
        .assets
        .iter()
        .find(|a| a.name == name)
        .with_context(|| format!("release receipt missing {name}"))
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify_bytes(asset: &PackageReleaseAsset, bytes: &[u8]) -> Result<()> {
    ensure!(
        asset.size == bytes.len() as u64 && asset.sha256 == digest(bytes),
        "receipt size/SHA-256 mismatch for {}",
        asset.name
    );
    Ok(())
}

async fn signed_asset(
    transport: &impl Transport,
    verifier: &Verifier,
    receipt: &PackageReleaseReceipt,
    base: &str,
    name: &str,
) -> Result<()> {
    let signature_name = format!("{name}.bundle");
    let bytes = transport
        .get(&format!("{base}/{signature_name}"), EVIDENCE_LIMIT)
        .await?;
    verify_bytes(asset(receipt, &signature_name)?, &bytes)?;
    let bundle = parse_bundle(&bytes)?;
    ensure!(
        matches!(bundle.content, SignatureContent::MessageSignature(_)),
        "detached asset requires a message signature"
    );
    let hash = Sha256Hash::from_hex(&asset(receipt, name)?.sha256)?;
    let result = verifier
        .verify(
            Artifact::from_digest(hash),
            &bundle,
            &signing_policy(receipt.channel),
        )
        .with_context(|| format!("verifying detached signature for {name}"))?;
    policy::validate_claims(&result, receipt)
}

async fn verify_with_transport(
    transport: &impl Transport,
    verifier: &Verifier,
    version: &str,
    target: &str,
) -> Result<VerifiedCapsule> {
    release_channel(version)?;
    ensure!(
        policy::LINUX_TARGETS.contains(&target),
        "unsupported capsule target"
    );
    let channel = release_channel(version)?;
    let base = format!(
        "https://github.com/jackin-project/jackin/releases/download/{}",
        release_tag(version)?
    );
    let receipt_bytes = transport
        .get(&format!("{base}/release.json"), RECEIPT_LIMIT)
        .await?;
    let bundle_bytes = transport
        .get(&format!("{base}/release.json.bundle"), EVIDENCE_LIMIT)
        .await?;
    let bundle = parse_bundle(&bundle_bytes)?;
    ensure!(
        matches!(bundle.content, SignatureContent::MessageSignature(_)),
        "release receipt requires a detached message signature"
    );
    // Authenticate the bytes before consuming any receipt identity or inventory.
    let result = verifier
        .verify(receipt_bytes.as_slice(), &bundle, &signing_policy(channel))
        .context("authenticating signed release.json")?;
    let receipt = parse_package_release_receipt(&receipt_bytes)
        .context("validating canonical Velnor release receipt")?;
    policy::validate_receipt(&receipt, version)?;
    policy::validate_claims(&result, &receipt)?;

    let name = format!("jackin-capsule-{target}.tar.gz");
    let selected = asset(&receipt, &name)?;
    signed_asset(transport, verifier, &receipt, &base, &name).await?;
    let companion_bytes = transport
        .get(&format!("{base}/capsule-manifest.json"), RECEIPT_LIMIT)
        .await?;
    verify_bytes(asset(&receipt, "capsule-manifest.json")?, &companion_bytes)?;
    signed_asset(
        transport,
        verifier,
        &receipt,
        &base,
        "capsule-manifest.json",
    )
    .await?;
    validate_companion(&companion_bytes, &receipt, version)?;

    let mut subjects = BTreeMap::new();
    for product in &receipt.products {
        for payload in &product.payloads {
            subjects.insert(
                payload.name.clone(),
                asset(&receipt, &payload.name)?.sha256.clone(),
            );
        }
    }
    subjects.insert(
        "capsule-manifest.json".into(),
        asset(&receipt, "capsule-manifest.json")?.sha256.clone(),
    );
    subjects.insert("release.json".into(), digest(&receipt_bytes));
    provenance::verify(transport, verifier, &receipt, &selected.sha256, &subjects).await?;
    let publication = freshness::pin(transport, &receipt, &receipt_bytes, &bundle_bytes).await?;
    Ok(VerifiedCapsule {
        publication,
        sha256: selected.sha256.clone(),
        size: selected.size,
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CapsuleManifest {
    version: String,
    targets: BTreeMap<String, String>,
}

fn validate_companion(bytes: &[u8], receipt: &PackageReleaseReceipt, version: &str) -> Result<()> {
    let manifest: CapsuleManifest =
        serde_json::from_slice(bytes).context("parsing authenticated capsule companion")?;
    ensure!(
        manifest.version == version,
        "capsule companion version mismatch"
    );
    ensure!(
        manifest.targets.len() == policy::LINUX_TARGETS.len(),
        "capsule companion must contain exactly the two supported Linux targets"
    );
    for target in policy::LINUX_TARGETS {
        let name = format!("jackin-capsule-{target}.tar.gz");
        ensure!(
            manifest.targets.get(target) == Some(&asset(receipt, &name)?.sha256),
            "capsule companion target/digest mismatch for {target}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
