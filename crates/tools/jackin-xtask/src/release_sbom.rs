// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Producer-side Cargo build-input BOMs. Ordinary stripped Rust binaries do not
//! contain a crate inventory for Syft to recover. Cargo's actual artifact stream
//! supplies that evidence; workspace metadata only supplies package identities.

mod archive;
mod cargo;
mod document;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const MAX_SBOM_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct BuildEvidence<'a> {
    pub(crate) version: &'a str,
    pub(crate) layout: &'a crate::release_archive::layout::ArchiveLayout,
}

pub(crate) fn generate(archive: &Path, output: &Path, evidence: &BuildEvidence<'_>) -> Result<()> {
    let metadata = cargo::metadata()?;
    let checksums = cargo::checksums(&metadata.workspace_root.join("Cargo.lock"))?;
    let records = fs::read(evidence.layout.artifacts())
        .context("reading successful Cargo build records for SBOM")?;
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
    let identity = crate::release_archive::payload_identity(archive)?;
    ensure!(
        evidence.layout.target() == identity.target,
        "Cargo layout does not match archive target"
    );
    ensure!(
        !evidence.version.is_empty(),
        "release application version is empty"
    );
    let members = archive::members(archive, &identity)?;
    let components =
        cargo::components(records, metadata, &identity, evidence, &members, checksums)?;
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

#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use std::{collections::BTreeMap, path::PathBuf};

// Synthetic Cargo-message fixtures model actual workspace package identities.

#[cfg(test)]
struct Fixture {
    archive: PathBuf,
    release: PathBuf,
    layout: crate::release_archive::layout::ArchiveLayout,
    records: Vec<u8>,
    metadata: cargo::Metadata,
}

#[cfg(test)]
fn fixture(dir: &Path, product: &str, target: &str) -> Fixture {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
    fs::write(dir.join("Cargo.lock"), "version=4\npackage=[]\n").unwrap();
    let archive = dir.join(format!("{product}-{target}.tar.gz"));
    let identity = crate::release_archive::payload_identity(&archive).unwrap();
    let layout = crate::release_archive::layout::ArchiveLayout::new(
        &dir.join(format!("build-{product}")),
        identity.target,
        crate::release_archive::layout::ArchiveProfile::Release,
    )
    .unwrap();
    let release = layout.target_output();
    fs::create_dir_all(&release).unwrap();
    let root_id = format!(
        "path+file://{}#{product}@{}",
        dir.display(),
        env!("CARGO_PKG_VERSION")
    );
    let core_id = format!(
        "path+file://{}/crates/core/jackin-core#{}",
        dir.display(),
        env!("CARGO_PKG_VERSION")
    );
    let metadata = serde_json::from_value(json!({"workspace_root":dir,"packages":[
        {"id":root_id,"name":product,"version":env!("CARGO_PKG_VERSION"),"source":null,"manifest_path":dir.join(format!("crates/apps/{product}/Cargo.toml")),"license":"Apache-2.0"},
        {"id":core_id,"name":"jackin-core","version":env!("CARGO_PKG_VERSION"),"source":null,"manifest_path":dir.join("crates/core/jackin-core/Cargo.toml"),"license":"Apache-2.0"}
    ]})).unwrap();
    let mut messages = vec![
        json!({"reason":"compiler-artifact","package_id":core_id,"target":{"kind":["lib"],"name":"jackin_core"},"profile":{"test":false},"features":[],"filenames":[release.join("deps/libjackin_core.rlib")],"executable":null,"fresh":true}),
    ];
    let gzip = flate2::write::GzEncoder::new(
        fs::File::create(&archive).unwrap(),
        flate2::Compression::default(),
    );
    let mut tar = tar::Builder::new(gzip);
    for name in identity.binaries {
        let bytes = name.as_bytes();
        fs::write(release.join(name), bytes).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        tar.append_data(&mut header, *name, bytes).unwrap();
        messages.push(json!({"reason":"compiler-artifact","package_id":root_id,"target":{"kind":["bin"],"name":name},"profile":{"test":false},"features":[],"filenames":[release.join(name)],"executable":release.join(name),"fresh":true}));
    }
    tar.into_inner().unwrap().finish().unwrap();
    messages.push(json!({"reason":"build-finished","success":true}));
    let records = messages
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    Fixture {
        archive,
        release,
        layout,
        records,
        metadata,
    }
}

#[cfg(test)]
fn write_fixture(fixture: &Fixture, output: &Path, version: &str) -> Result<()> {
    write_from_inputs(
        &fixture.archive,
        output,
        &BuildEvidence {
            version,
            layout: &fixture.layout,
        },
        &fixture.metadata,
        &fixture.records,
        &BTreeMap::new(),
    )
}

#[cfg(test)]
pub(crate) fn write_payload_fixture(
    dir: &Path,
    product: &str,
    target: &str,
    version: &str,
) -> PathBuf {
    let fixture = fixture(dir, product, target);
    write_fixture(
        &fixture,
        &PathBuf::from(format!("{}.sbom.json", fixture.archive.display())),
        version,
    )
    .unwrap();
    // Inputs remain private build evidence, outside the producer asset directory.
    fixture.archive
}
