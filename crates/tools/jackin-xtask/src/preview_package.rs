// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{env, fs, path::Path};

use crate::release_sbom::archive_sha256;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
];

pub(crate) fn run() -> Result<()> {
    let source = env::var("VELNOR_RELEASE_SOURCE_COMMIT")?;
    let source_ref = env::var("VELNOR_RELEASE_SOURCE_REF")?;
    let version = env::var("VELNOR_RELEASE_VERSION")?;
    let count = env::var("VELNOR_RELEASE_COMMIT_COUNT")?;
    validate_identity(&source, &source_ref, &version, &count)?;
    let output = env::var("VELNOR_RELEASE_OUTPUT_DIRECTORY")?;
    let manifest = env::var("VELNOR_RELEASE_OUTPUT_MANIFEST")?;
    let output = Path::new(&output);
    ensure!(
        output.is_absolute() && output.is_dir(),
        "Velnor output must be an existing absolute directory"
    );
    ensure!(
        Path::new(&manifest) == output.join("package-output.json"),
        "unexpected producer manifest path"
    );
    ensure!(
        crate::fs_util::read_dir_sorted(output)?.is_empty(),
        "producer output must be empty"
    );
    crate::release_archive::build_preview(&version, output)?;
    write_inventory(output, &source, &source_ref, &version)?;
    Ok(())
}

fn validate_identity(source: &str, source_ref: &str, version: &str, count: &str) -> Result<()> {
    ensure!(
        source.len() == 40
            && source
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid preview source SHA"
    );
    ensure!(
        source_ref == "refs/heads/main",
        "preview source must be main"
    );
    let parsed_count: u64 = count.parse().context("invalid preview commit count")?;
    ensure!(
        parsed_count > 0 && parsed_count.to_string() == count,
        "noncanonical preview commit count"
    );
    ensure!(
        version
            == format!(
                "{}-preview.{count}+{}",
                env!("CARGO_PKG_VERSION"),
                &source[..7]
            ),
        "preview version differs from Velnor identity or package base"
    );
    Ok(())
}

fn write_inventory(output: &Path, source: &str, source_ref: &str, version: &str) -> Result<()> {
    let mut payloads = Vec::new();
    let mut sboms = Vec::new();
    let mut capsule_targets = serde_json::Map::new();
    for product in ["jackin", "jackin-capsule"] {
        for target in TARGETS {
            if product == "jackin-capsule" && target.contains("apple") {
                continue;
            }
            let name = format!("{product}-{target}.tar.gz");
            let sha256 = archive_sha256(&output.join(&name))?;
            let sbom = format!("{name}.sbom.json");
            crate::release_sbom::verify(&output.join(&name), &output.join(&sbom))?;
            payloads.push(json!({"product_id": product, "target": target, "format": "tar-gz", "name": name, "sha256": sha256}));
            sboms.push(
                json!({"payload_name": name, "name": sbom, "sha256": archive_sha256(&output.join(&sbom))?}),
            );
            if product == "jackin-capsule" {
                capsule_targets.insert(target.to_owned(), Value::String(sha256));
            }
        }
    }
    payloads.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_owned());
    sboms.sort_by_key(|row| row["payload_name"].as_str().unwrap_or_default().to_owned());
    let companion = output.join("capsule-manifest.json");
    fs::write(
        &companion,
        serde_json::to_vec(&json!({"version": version, "targets": capsule_targets}))?,
    )?;
    let manifest: Value = json!({
        "schema": "velnor.package-output.v1", "source_commit": source,
        "source_ref": source_ref, "version": version, "payloads": payloads,
        "companions": [{"id": "capsule-manifest", "path": "capsule-manifest.json", "name": "capsule-manifest.json", "sha256": archive_sha256(&companion)?}],
        "sboms": sboms,
    });
    fs::write(
        output.join("package-output.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
