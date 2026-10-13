// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! `CycloneDX` SBOM generation and archive binding checks.

use std::{fs, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cmd;

#[cfg(test)]
mod tests;

const MAX_SBOM_BYTES: usize = 32 * 1024 * 1024;
const ARCHIVE_DIGEST_PROPERTY: &str = "jackin:archive-sha256";
const ARCHIVE_NAME_PROPERTY: &str = "jackin:archive-name";

pub(crate) fn generate(archive: &Path, output: &Path) -> Result<()> {
    let sbom = cmd::output(
        cmd::command("syft").args([
            "scan",
            archive
                .to_str()
                .with_context(|| format!("archive path is not UTF-8: {}", archive.display()))?,
            "-o",
            "cyclonedx-json",
        ]),
    )
    .with_context(|| format!("generating SBOM for {}", archive.display()))?;
    let mut document: Value = serde_json::from_slice(&sbom)
        .with_context(|| format!("parsing generated SBOM for {}", archive.display()))?;
    bind_archive(&mut document, archive)?;
    validate_document(&document, archive)?;
    fs::write(
        output,
        serde_json::to_vec_pretty(&document).context("serializing bound CycloneDX SBOM")?,
    )
    .with_context(|| format!("writing SBOM {}", output.display()))
}

pub(crate) fn verify(archive: &Path, path: &Path) -> Result<()> {
    ensure!(
        path.is_file(),
        "SBOM sidecar does not exist: {}",
        path.display()
    );
    let content = fs::read(path).with_context(|| format!("reading SBOM {}", path.display()))?;
    ensure!(
        !content.is_empty(),
        "SBOM sidecar is empty: {}",
        path.display()
    );
    ensure!(
        content.len() <= MAX_SBOM_BYTES,
        "SBOM sidecar is too large: {}",
        path.display()
    );
    let document: Value = serde_json::from_slice(&content)
        .with_context(|| format!("parsing SBOM JSON {}", path.display()))?;
    validate_document(&document, archive)
}

fn bind_archive(document: &mut Value, archive: &Path) -> Result<()> {
    let archive_name = archive_name(archive)?;
    let archive_digest = archive_sha256(archive)?;
    let root = document
        .as_object_mut()
        .context("generated SBOM root must be a JSON object")?;
    let metadata = root
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .context("generated SBOM metadata must be an object")?;
    let component = metadata
        .entry("component")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("generated SBOM metadata.component must be an object")?;
    component.insert("type".to_owned(), Value::String("file".to_owned()));
    component.insert("name".to_owned(), Value::String(archive_name.clone()));

    let properties = component
        .entry("properties")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("generated SBOM metadata.component.properties must be an array")?;
    properties.retain(|property| {
        !matches!(
            property.get("name").and_then(Value::as_str),
            Some(ARCHIVE_DIGEST_PROPERTY | ARCHIVE_NAME_PROPERTY)
        )
    });
    properties.push(json!({
        "name": ARCHIVE_DIGEST_PROPERTY,
        "value": archive_digest,
    }));
    properties.push(json!({
        "name": ARCHIVE_NAME_PROPERTY,
        "value": archive_name,
    }));
    Ok(())
}

fn validate_document(document: &Value, archive: &Path) -> Result<()> {
    let root = document
        .as_object()
        .context("SBOM root must be a JSON object")?;
    ensure!(
        root.get("bomFormat").and_then(Value::as_str) == Some("CycloneDX"),
        "SBOM bomFormat must be CycloneDX"
    );
    let spec_version = root
        .get("specVersion")
        .and_then(Value::as_str)
        .context("SBOM specVersion must be a string")?;
    ensure!(
        matches!(spec_version, "1.3" | "1.4" | "1.5" | "1.6" | "1.7"),
        "unsupported CycloneDX SBOM specVersion: {spec_version}"
    );
    let serial_number = root
        .get("serialNumber")
        .and_then(Value::as_str)
        .context("SBOM serialNumber must be a string")?;
    ensure!(
        serial_number
            .strip_prefix("urn:uuid:")
            .is_some_and(|uuid| !uuid.is_empty()),
        "SBOM serialNumber must be a non-empty urn:uuid value"
    );

    let metadata = root
        .get("metadata")
        .and_then(Value::as_object)
        .context("SBOM metadata must be an object")?;
    let component = metadata
        .get("component")
        .and_then(Value::as_object)
        .context("SBOM metadata.component must be an object")?;
    ensure!(
        component.get("type").and_then(Value::as_str) == Some("file"),
        "SBOM metadata.component.type must be file"
    );
    let archive_name = archive_name(archive)?;
    ensure!(
        component.get("name").and_then(Value::as_str) == Some(archive_name.as_str()),
        "SBOM metadata.component.name is not bound to the archive"
    );
    let properties = component
        .get("properties")
        .and_then(Value::as_array)
        .context("SBOM metadata.component.properties must be an array")?;
    let digest_properties = properties
        .iter()
        .filter(|property| {
            property.get("name").and_then(Value::as_str) == Some(ARCHIVE_DIGEST_PROPERTY)
        })
        .collect::<Vec<_>>();
    ensure!(
        digest_properties.len() == 1,
        "SBOM must contain exactly one archive digest binding"
    );
    let actual_digest = archive_sha256(archive)?;
    ensure!(
        digest_properties[0].get("value").and_then(Value::as_str) == Some(actual_digest.as_str()),
        "SBOM archive digest binding does not match the archive"
    );
    let name_properties = properties
        .iter()
        .filter(|property| {
            property.get("name").and_then(Value::as_str) == Some(ARCHIVE_NAME_PROPERTY)
        })
        .collect::<Vec<_>>();
    ensure!(
        name_properties.len() == 1
            && name_properties[0].get("value").and_then(Value::as_str)
                == Some(archive_name.as_str()),
        "SBOM archive name binding does not match the archive"
    );

    let components = root
        .get("components")
        .and_then(Value::as_array)
        .context("SBOM components must be an array")?;
    ensure!(!components.is_empty(), "SBOM components must not be empty");
    for (index, component) in components.iter().enumerate() {
        let component = component
            .as_object()
            .with_context(|| format!("SBOM component {index} must be an object"))?;
        ensure!(
            component
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "SBOM component {index} has no type"
        );
        ensure!(
            component
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "SBOM component {index} has no name"
        );
    }
    Ok(())
}

fn archive_name(archive: &Path) -> Result<String> {
    archive
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .with_context(|| format!("archive path has no UTF-8 filename: {}", archive.display()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "release SBOM verification is host-side xtask filesystem work"
)]
fn archive_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("reading {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}
