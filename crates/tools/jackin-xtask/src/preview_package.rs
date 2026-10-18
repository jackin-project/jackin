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
        fs::read_dir(output)?.next().is_none(),
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
mod tests {
    use super::*;

    #[test]
    fn inventory_binds_exactly_six_payloads_sboms_and_capsule_companion() {
        let temp = tempfile::tempdir().unwrap();
        for product in ["jackin", "jackin-capsule"] {
            for target in TARGETS {
                if product == "jackin-capsule" && target.contains("apple") {
                    continue;
                }
                let name = format!("{product}-{target}.tar.gz");
                let staging = tempfile::tempdir().unwrap();
                let path = crate::release_sbom::tests::write_payload_fixture(
                    staging.path(),
                    product,
                    target,
                    "0.6.4-preview.42+aaaaaaa",
                );
                fs::copy(&path, temp.path().join(&name)).unwrap();
                fs::copy(
                    format!("{}.sbom.json", path.display()),
                    temp.path().join(format!("{name}.sbom.json")),
                )
                .unwrap();
            }
        }
        write_inventory(
            temp.path(),
            &"a".repeat(40),
            "refs/heads/main",
            "0.6.4-preview.42+aaaaaaa",
        )
        .unwrap();
        let manifest: Value =
            serde_json::from_slice(&fs::read(temp.path().join("package-output.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["schema"], "velnor.package-output.v1");
        assert_eq!(manifest["payloads"].as_array().unwrap().len(), 6);
        assert_eq!(manifest["sboms"].as_array().unwrap().len(), 6);
        let names: Vec<_> = manifest["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        let companion: Value =
            serde_json::from_slice(&fs::read(temp.path().join("capsule-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(companion["targets"].as_object().unwrap().len(), 2);
        for (target, digest) in companion["targets"].as_object().unwrap() {
            assert_eq!(
                digest,
                &archive_sha256(&temp.path().join(format!("jackin-capsule-{target}.tar.gz")))
                    .unwrap()
            );
        }
        assert_eq!(
            manifest["companions"][0]["sha256"],
            archive_sha256(&temp.path().join("capsule-manifest.json")).unwrap()
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 14);
    }

    #[test]
    fn rejects_identity_conflicts_before_any_build() {
        let source = "a".repeat(40);
        let version = format!("{}-preview.42+aaaaaaa", env!("CARGO_PKG_VERSION"));
        validate_identity(&source, "refs/heads/main", &version, "42").unwrap();
        for (sha, branch, candidate, count) in [
            ("bad", "refs/heads/main", version.as_str(), "42"),
            (source.as_str(), "refs/heads/other", version.as_str(), "42"),
            (source.as_str(), "refs/heads/main", version.as_str(), "43"),
            (source.as_str(), "refs/heads/main", version.as_str(), "042"),
            (
                source.as_str(),
                "refs/heads/main",
                "0.0.0-preview.42+aaaaaaa",
                "42",
            ),
        ] {
            assert!(validate_identity(sha, branch, candidate, count).is_err());
        }
    }
}
