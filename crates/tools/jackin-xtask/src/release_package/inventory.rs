// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use crate::release_sbom::archive_sha256;
use anyhow::{Context, Result, ensure};
use flate2::read::GzDecoder;
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};
use velnor_actions_contract::preview_production::{
    ReleasePayloadPair, ReleaseProductionRequest, expected_release_payload_pairs,
};

const CAPSULE_MANIFEST_V2_SCHEMA: &str = "jackin.capsule-manifest.v2";
const CAPSULE_BINARY_MEMBER: &str = "jackin-capsule";
const CAPSULE_FEATURE_USAGE_RELAY_PROXY_V2: &str = "usage-relay-proxy-v2";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CapsuleTargetDigests {
    pub(super) archive_sha256: String,
    pub(super) binary_sha256: Option<String>,
}

pub(super) fn write_leaf(
    output: &Path,
    request: &ReleaseProductionRequest,
    pair: &ReleasePayloadPair,
) -> Result<()> {
    let (payloads, sboms) = rows(output, std::slice::from_ref(pair))?;
    write_manifest(
        output,
        request,
        "velnor.package-payload-output.v1",
        payloads,
        sboms,
        Vec::new(),
    )
}

pub(super) fn write_complete(output: &Path, request: &ReleaseProductionRequest) -> Result<()> {
    let features = declared_capsule_features()?;
    refuse_unverified_feature_declarations(&features)?;
    let pairs = expected_release_payload_pairs(&request.plan)?;
    let (payloads, sboms) = rows(output, &pairs)?;
    let mut capsule_targets = BTreeMap::new();
    for pair in pairs
        .iter()
        .filter(|pair| pair.product_id == "jackin-capsule")
    {
        let archive = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        let archive_path = output.join(&archive);
        let binary_sha256 = if features.is_empty() {
            None
        } else {
            Some(capsule_binary_sha256(&archive_path)?)
        };
        capsule_targets.insert(
            pair.target.clone(),
            CapsuleTargetDigests {
                archive_sha256: archive_sha256(&archive_path)?,
                binary_sha256,
            },
        );
    }
    let companion_bytes =
        render_capsule_companion(&request.context.version, &capsule_targets, &features)?;
    let companion = output.join("capsule-manifest.json");
    fs::write(&companion, companion_bytes)?;
    let companions = vec![
        json!({"id":"capsule-manifest", "path":"capsule-manifest.json",
        "name":"capsule-manifest.json", "sha256":archive_sha256(&companion)?}),
    ];
    write_manifest(
        output,
        request,
        "velnor.package-output.v1",
        payloads,
        sboms,
        companions,
    )
}

pub(super) fn declared_capsule_features() -> Result<Vec<String>> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .context("xtask package path has no workspace root")?;
    let path = workspace_root.join("crates/apps/jackin-capsule/Cargo.toml");
    let manifest_text =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: toml::Value =
        toml::from_str(&manifest_text).with_context(|| format!("parsing {}", path.display()))?;
    let artifact = manifest
        .get("package")
        .and_then(|package| package.get("metadata"))
        .and_then(|metadata| metadata.get("jackin"))
        .and_then(|jackin| jackin.get("capsule-artifact"))
        .and_then(toml::Value::as_table)
        .context("capsule-artifact package metadata is required")?;
    ensure!(
        artifact.len() == 2 && artifact.get("schema").and_then(toml::Value::as_integer) == Some(1),
        "capsule-artifact metadata must have only schema=1 and features"
    );
    let features = artifact
        .get("features")
        .and_then(toml::Value::as_array)
        .context("capsule-artifact features must be an array")?
        .iter()
        .map(|feature| {
            feature
                .as_str()
                .map(str::to_owned)
                .context("capsule-artifact feature names must be strings")
        })
        .collect::<Result<Vec<_>>>()?;
    validate_capsule_features(&features)?;
    Ok(features)
}

pub(super) fn refuse_unverified_feature_declarations(features: &[String]) -> Result<()> {
    ensure!(
        features.is_empty(),
        "capsule feature issuance is disabled until the source-bound feature-test report gate is enforced"
    );
    Ok(())
}

fn validate_capsule_features(features: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for feature in features {
        ensure!(
            feature == CAPSULE_FEATURE_USAGE_RELAY_PROXY_V2,
            "unsupported capsule feature declaration"
        );
        ensure!(
            seen.insert(feature),
            "duplicate capsule feature declaration"
        );
    }
    ensure!(
        features.windows(2).all(|pair| pair[0] < pair[1]),
        "capsule feature declarations must be sorted"
    );
    Ok(())
}

fn render_capsule_companion(
    version: &str,
    targets: &BTreeMap<String, CapsuleTargetDigests>,
    features: &[String],
) -> Result<Vec<u8>> {
    validate_capsule_features(features)?;
    ensure!(
        targets.len() == 2
            && ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]
                .iter()
                .all(|target| targets.contains_key(*target)),
        "capsule companion must contain exactly the two supported Linux targets"
    );
    for digests in targets.values() {
        canonical_sha256(&digests.archive_sha256, "capsule archive digest")?;
    }
    let mut target_values = serde_json::Map::new();
    for (target, digests) in targets {
        let value = if features.is_empty() {
            Value::String(digests.archive_sha256.clone())
        } else {
            let binary_sha256 = digests
                .binary_sha256
                .as_deref()
                .context("capsule binary digest is required for feature manifest")?;
            canonical_sha256(binary_sha256, "capsule binary digest")?;
            json!({
                "archive_sha256": digests.archive_sha256,
                "binary_member": CAPSULE_BINARY_MEMBER,
                "binary_sha256": binary_sha256,
                "features": features,
            })
        };
        target_values.insert(target.clone(), value);
    }
    let manifest = if features.is_empty() {
        json!({"version":version,"targets":target_values})
    } else {
        json!({
            "schema":CAPSULE_MANIFEST_V2_SCHEMA,
            "version":version,
            "targets":target_values
        })
    };
    Ok(serde_json::to_vec(&manifest)?)
}

#[cfg(test)]
pub(super) fn render_capsule_companion_fixture(
    version: &str,
    targets: &BTreeMap<String, CapsuleTargetDigests>,
    features: &[String],
) -> Result<Vec<u8>> {
    render_capsule_companion(version, targets, features)
}

pub(super) fn capsule_binary_sha256(archive_path: &Path) -> Result<String> {
    let archive_fd = open(
        archive_path,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .with_context(|| format!("opening capsule archive {}", archive_path.display()))?;
    let archive_file = fs::File::from(archive_fd);
    let decoder = GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive.entries()?;
    let mut binary_digest = None;
    for entry in &mut entries {
        let mut entry = entry?;
        ensure!(
            entry.path_bytes().as_ref() == CAPSULE_BINARY_MEMBER.as_bytes(),
            "capsule archive contains an unexpected member"
        );
        ensure!(
            entry.header().entry_type().is_file(),
            "capsule binary member must be a regular file"
        );
        ensure!(
            entry.header().mode()? == 0o755,
            "capsule binary member must have canonical executable mode 0755"
        );
        ensure!(
            binary_digest.is_none(),
            "capsule archive has duplicate binary members"
        );
        let expected_size = entry.header().size()?;
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .context("capsule member size overflow")?;
            hasher.update(&buffer[..read]);
        }
        ensure!(
            total == expected_size,
            "capsule member size changed while hashing"
        );
        binary_digest = Some(hex::encode(hasher.finalize()));
    }
    binary_digest.context("capsule archive must contain exactly one executable binary")
}

fn canonical_sha256(value: &str, label: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{label} must be a canonical lowercase SHA-256 digest"
    );
    Ok(())
}

fn rows(output: &Path, pairs: &[ReleasePayloadPair]) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut payloads = Vec::new();
    let mut sboms = Vec::new();
    for pair in pairs {
        let name = format!("{}-{}.tar.gz", pair.product_id, pair.target);
        let sbom = format!("{name}.sbom.json");
        crate::release_sbom::verify(&output.join(&name), &output.join(&sbom))?;
        payloads.push(json!({"product_id":pair.product_id, "target":pair.target,
            "format":"tar-gz", "name":name, "sha256":archive_sha256(&output.join(&name))?}));
        sboms.push(json!({"payload_name":name, "name":sbom, "sha256":archive_sha256(&output.join(&sbom))?}));
    }
    payloads.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_owned());
    sboms.sort_by_key(|row| row["payload_name"].as_str().unwrap_or_default().to_owned());
    Ok((payloads, sboms))
}

fn write_manifest(
    output: &Path,
    request: &ReleaseProductionRequest,
    schema: &str,
    payloads: Vec<Value>,
    sboms: Vec<Value>,
    companions: Vec<Value>,
) -> Result<()> {
    let context = &request.context;
    let manifest = json!({"schema":schema, "source_commit":context.source_sha,
        "source_ref":context.source_ref, "version":context.version,
        "payloads":payloads,"companions":companions,"sboms":sboms});
    fs::write(
        output.join("package-output.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}
