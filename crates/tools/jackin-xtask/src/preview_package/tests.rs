// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

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
            let path = crate::release_sbom::write_payload_fixture(
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
            &archive_sha256(&temp.path().join(format!("jackin-capsule-{target}.tar.gz"))).unwrap()
        );
    }
    assert_eq!(
        manifest["companions"][0]["sha256"],
        archive_sha256(&temp.path().join("capsule-manifest.json")).unwrap()
    );
    assert_eq!(
        crate::fs_util::read_dir_sorted(temp.path()).unwrap().len(),
        14
    );
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
