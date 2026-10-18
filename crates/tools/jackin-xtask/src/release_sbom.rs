// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Producer-side Cargo build-input BOMs. Ordinary stripped Rust binaries do not
//! contain a crate inventory for Syft to recover. Cargo's actual artifact stream
//! supplies that evidence; workspace metadata only supplies package identities.

mod archive;
mod cargo;
mod document;
#[cfg(test)]
pub(crate) mod tests;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const MAX_SBOM_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct BuildEvidence<'a> {
    pub(crate) version: &'a str,
    pub(crate) release_dir: &'a Path,
    pub(crate) artifacts: &'a Path,
}

pub(crate) fn generate(archive: &Path, output: &Path, evidence: &BuildEvidence<'_>) -> Result<()> {
    let metadata = cargo::metadata()?;
    let checksums = cargo::checksums(&metadata.workspace_root.join("Cargo.lock"))?;
    let records =
        fs::read(evidence.artifacts).context("reading successful Cargo build records for SBOM")?;
    write_from_inputs(archive, output, evidence, &metadata, &records, &checksums)
}

fn write_from_inputs(
    archive: &Path,
    output: &Path,
    evidence: &BuildEvidence<'_>,
    metadata: &cargo::Metadata,
    records: &[u8],
    checksums: &cargo::Checksums,
) -> Result<()> {
    let release_dir =
        std::path::absolute(evidence.release_dir).context("resolving Cargo release directory")?;
    let evidence = BuildEvidence {
        version: evidence.version,
        release_dir: &release_dir,
        artifacts: evidence.artifacts,
    };
    let identity = crate::release_archive::payload_identity(archive)?;
    ensure!(
        evidence
            .release_dir
            .ends_with(Path::new(identity.target).join("release")),
        "Cargo release directory does not match archive target"
    );
    ensure!(
        !evidence.version.is_empty(),
        "release application version is empty"
    );
    let members = archive::members(archive, &identity)?;
    let components =
        cargo::components(records, metadata, &identity, &evidence, &members, checksums)?;
    let document = document::build(
        archive,
        evidence.version,
        &identity,
        &members,
        components,
        &metadata.workspace_root,
    )?;
    document::validate(&document, archive)?;
    let bytes = serde_json::to_vec_pretty(&document)?;
    ensure!(bytes.len() <= MAX_SBOM_BYTES, "generated SBOM is too large");
    fs::write(output, bytes).with_context(|| format!("writing SBOM {}", output.display()))
}

pub(crate) fn verify(archive: &Path, path: &Path) -> Result<()> {
    ensure!(
        path.is_file(),
        "SBOM sidecar does not exist: {}",
        path.display()
    );
    let content = fs::read(path).with_context(|| format!("reading SBOM {}", path.display()))?;
    ensure!(
        !content.is_empty() && content.len() <= MAX_SBOM_BYTES,
        "SBOM sidecar is empty or too large"
    );
    let document: Value = serde_json::from_slice(&content).context("parsing CycloneDX SBOM")?;
    document::validate(&document, archive)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[expect(
    clippy::disallowed_methods,
    reason = "release SBOM verification is host-side xtask filesystem work"
)]
pub(crate) fn archive_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}
