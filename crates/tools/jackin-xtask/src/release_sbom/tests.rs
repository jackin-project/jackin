// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

// The ignored real-archive proof below consumes Cargo-produced build records.

use super::*;

mod profile;

#[test]
fn cached_compiler_records_generate_bound_target_and_member_inventory() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path(), "jackin", "aarch64-apple-darwin");
    let output = dir.path().join("bom.json");
    write_fixture(&fixture, &output, "0.6.4-preview.42+aaaaaaa").unwrap();
    verify(&fixture.archive, &output).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(document["components"].as_array().unwrap().len(), 5);
    let text = serde_json::to_string(&document).unwrap();
    assert!(!text.contains(dir.path().to_str().unwrap()));
    assert!(text.contains("workspace:crates/core/jackin-core/Cargo.toml"));
    assert!(
        document["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "jackin"
                && c["type"] == "application"
                && c["version"] == "0.6.4")
    );
}

#[test]
fn absent_malformed_failed_and_wrong_package_build_records_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "x86_64-unknown-linux-gnu");
    let original = fixture.records.clone();
    for records in [
        b"".to_vec(),
        b"not json".to_vec(),
        original[..original.len() - 1].to_vec(),
        String::from_utf8(original.clone())
            .unwrap()
            .replace("\"success\":true", "\"success\":false")
            .into_bytes(),
        String::from_utf8(original.clone())
            .unwrap()
            .replace("jackin-capsule@", "wrong-package@")
            .into_bytes(),
    ] {
        fixture.records = records;
        write_fixture(
            &fixture,
            &dir.path().join("bom.json"),
            "0.6.4-preview.42+aaaaaaa",
        )
        .expect_err("invalid compiler records must fail");
    }
    fixture.records = original;
    fs::write(
        fixture.release.join("jackin-capsule"),
        b"changed compiled executable",
    )
    .unwrap();
    write_fixture(
        &fixture,
        &dir.path().join("bom.json"),
        "0.6.4-preview.42+aaaaaaa",
    )
    .expect_err("archive must match actual root artifact bytes");
}

#[test]
fn cargo_root_binary_target_and_inventory_mismatches_fail() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin", "aarch64-apple-darwin");
    let original = String::from_utf8(fixture.records.clone()).unwrap();
    for records in [
        original
            .lines()
            .filter(|line| !line.contains("\"name\":\"jackin-role\""))
            .collect::<Vec<_>>()
            .join("\n"),
        original.replace(
            "aarch64-apple-darwin/release/jackin",
            "x86_64-apple-darwin/release/jackin",
        ),
        original.replace("\"test\":false", "\"test\":true"),
    ] {
        fixture.records = records.into_bytes();
        write_fixture(
            &fixture,
            &dir.path().join("bom.json"),
            "0.6.4-preview.42+aaaaaaa",
        )
        .expect_err("root target/inventory mismatch must fail");
    }
}

#[test]
fn bom_rejects_empty_dependencies_member_digest_and_coverage_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    let output = dir.path().join("bom.json");
    write_fixture(&fixture, &output, "0.6.4-preview.42+aaaaaaa").unwrap();
    let document: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    for pointer in [
        "/metadata/component/name",
        "/metadata/component/hashes/0/content",
        "/metadata/properties/0/value",
        "/metadata/properties/1/value",
        "/metadata/properties/2/value",
        "/components/0/version",
        "/compositions/0/aggregate",
    ] {
        let mut mutated = document.clone();
        *mutated.pointer_mut(pointer).unwrap() = json!("wrong");
        document::validate(&mutated, &fixture.archive).expect_err("mutated BOM binding must fail");
    }
    let mut empty = document.clone();
    empty["components"] = json!([]);
    document::validate(&empty, &fixture.archive).expect_err("empty inventory must fail");
    let mut no_dependencies = document;
    no_dependencies["components"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["type"] != "library");
    document::validate(&no_dependencies, &fixture.archive)
        .expect_err("missing crate inputs must fail");
}

#[test]
fn normalized_workspace_refs_do_not_depend_on_checkout_directory() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = fixture(first.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    let b = fixture(second.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    let extract = |f: &Fixture| {
        let identity = crate::release_archive::payload_identity(&f.archive).unwrap();
        cargo::components(
            &f.records,
            &f.metadata,
            &identity,
            &BuildEvidence {
                version: "0.6.4",
                layout: &f.layout,
            },
            &archive::members(&f.archive, &identity).unwrap(),
            &BTreeMap::new(),
        )
        .unwrap()
    };
    assert_eq!(extract(&a), extract(&b));
}

#[test]
#[ignore = "requires actual producer archive, successful Cargo JSON records, and matching release binaries"]
fn real_archive_cargo_evidence_proof() {
    let archive = PathBuf::from(std::env::var("JACKIN_REAL_SBOM_ARCHIVE").unwrap());
    let target_dir = PathBuf::from(std::env::var("JACKIN_REAL_SBOM_TARGET_DIR").unwrap());
    let version = std::env::var("JACKIN_REAL_SBOM_VERSION").unwrap();
    let output = PathBuf::from(std::env::var("JACKIN_REAL_SBOM_OUTPUT").unwrap());
    assert!(
        !output.exists(),
        "qualification must not overwrite an existing SBOM"
    );
    let identity = crate::release_archive::payload_identity(&archive).unwrap();
    let layout = crate::release_archive::layout::ArchiveLayout::new(
        &target_dir,
        identity.target,
        crate::release_archive::layout::ArchiveProfile::Release,
    )
    .unwrap();
    if std::env::var("JACKIN_REAL_SBOM_REPACKAGE").as_deref() == Ok("1") {
        assert!(
            !archive.exists(),
            "qualification must not overwrite an existing archive"
        );
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        let package = match identity.package {
            "jackin" => crate::release_archive::ArchivePackage::Jackin,
            "jackin-capsule" => crate::release_archive::ArchivePackage::JackinCapsule,
            _ => unreachable!("payload_identity admits only the two declared products"),
        };
        crate::release_archive::package(package, &layout, archive.parent().unwrap()).unwrap();
    }
    generate(
        &archive,
        &output,
        &BuildEvidence {
            version: &version,
            layout: &layout,
        },
    )
    .unwrap();
    verify(&archive, &output).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    let components = document["components"].as_array().unwrap();
    let actual_files = components
        .iter()
        .filter(|c| c["type"] == "file")
        .map(|component| component["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let expected_files = identity
        .binaries
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual_files, expected_files);
    assert!(components.iter().any(
        |component| component["type"] == "application" && component["name"] == identity.package
    ));
    assert!(
        components
            .iter()
            .any(|component| component["type"] == "library")
    );
    eprintln!(
        "verified {} actual Cargo/member components",
        components.len()
    );
}

#[test]
fn archive_members_must_be_exact_regular_executables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join("jackin-capsule-aarch64-unknown-linux-gnu.tar.gz");
    let identity = crate::release_archive::payload_identity(&path).unwrap();
    for (name, mode, count, regular) in [
        ("nested/jackin-capsule", 0o755, 1, true),
        ("jackin-capsule", 0o644, 1, true),
        ("jackin-capsule", 0o755, 2, true),
        ("jackin-capsule", 0o755, 1, false),
        ("jackin-capsule", 0o755, 0, true),
    ] {
        let gzip = flate2::write::GzEncoder::new(
            fs::File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(gzip);
        for _ in 0..count {
            let mut header = tar::Header::new_gnu();
            header.set_mode(mode);
            header.set_size(0);
            if !regular {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("jackin-capsule").unwrap();
            }
            header.set_cksum();
            tar.append_data(&mut header, name, b"".as_slice()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        archive::members(&path, &identity)
            .expect_err("invalid archive executable inventory must fail");
    }
}

#[test]
fn uncompiled_workspace_packages_are_not_reported_as_build_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    fixture.metadata.packages.push(serde_json::from_value(json!({
        "id":format!("path+file://{}/crates/tools/jackin-xtask#{}",dir.path().display(),env!("CARGO_PKG_VERSION")),
        "name":"jackin-xtask", "version":env!("CARGO_PKG_VERSION"), "source":null,
        "manifest_path":dir.path().join("crates/tools/jackin-xtask/Cargo.toml"), "license":"Apache-2.0"
    })).unwrap());
    // Full locked metadata can acquire unselected registry manifests too;
    // their availability must not turn them into claimed build inputs.
    fixture.metadata.packages.push(
        serde_json::from_value(json!({
            "id":"registry+https://github.com/rust-lang/crates.io-index#itoa@1.0.18",
            "name":"itoa", "version":"1.0.18",
            "source":"registry+https://github.com/rust-lang/crates.io-index",
            "manifest_path":dir.path().join("unselected-registry/itoa-1.0.18/Cargo.toml"),
            "license":"MIT OR Apache-2.0"
        }))
        .unwrap(),
    );
    let output = dir.path().join("bom.json");
    write_fixture(&fixture, &output, "0.6.4-preview.42+aaaaaaa").unwrap();
    let document: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert!(
        !document["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["name"] == "jackin-xtask" || component["name"] == "itoa")
    );
    fixture.records = String::from_utf8(fixture.records)
        .unwrap()
        .replace(
            "aarch64-unknown-linux-gnu/release/deps",
            "x86_64-unknown-linux-gnu/release/deps",
        )
        .into_bytes();
    write_fixture(
        &fixture,
        &dir.path().join("invalid.json"),
        "0.6.4-preview.42+aaaaaaa",
    )
    .expect_err("foreign target dependencies must not masquerade as host-build inputs");
}

#[test]
fn timings_build_started_is_validated_and_cannot_duplicate_or_reorder() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    let original = String::from_utf8(fixture.records.clone()).unwrap();
    let start = r#"{"reason":"build-started","run_id":"20261018T070426761Z-5c003beaf63de833"}"#;
    fixture.records = format!("{start}\n{original}").into_bytes();
    write_fixture(
        &fixture,
        &dir.path().join("valid.json"),
        "0.6.4-preview.42+aaaaaaa",
    )
    .unwrap();
    let mut lines = original.lines();
    let first = lines.next().unwrap();
    let rest = lines.collect::<Vec<_>>().join("\n");
    for records in [
        format!("{start}\n{start}\n{original}"),
        format!(
            "{{\"reason\":\"build-started\",\"run_id\":\"bad\",\"run_id\":\"20261018T070426761Z-5c003beaf63de833\"}}\n{original}"
        ),
        format!("{first}\n{start}\n{rest}"),
        format!("{original}\n{start}"),
        format!("{{\"reason\":\"unknown\"}}\n{original}"),
    ] {
        fixture.records = records.into_bytes();
        write_fixture(
            &fixture,
            &dir.path().join("invalid.json"),
            "0.6.4-preview.42+aaaaaaa",
        )
        .expect_err("invalid build record order must fail");
    }
    for invalid in [
        json!({"reason":"build-started"}),
        json!({"reason":"build-started","run_id":1}),
        json!({"reason":"build-started","run_id":""}),
        json!({"reason":"build-started","run_id":"20261318T070426761Z-5c003beaf63de833"}),
        json!({"reason":"build-started","run_id":"20261018T070426761Z-not-a-hash"}),
        json!({"reason":"build-started","run_id":"20261018T070426761Z-5c003beaf63de833","extra":true}),
    ] {
        fixture.records = format!("{invalid}\n{original}").into_bytes();
        write_fixture(
            &fixture,
            &dir.path().join("invalid.json"),
            "0.6.4-preview.42+aaaaaaa",
        )
        .expect_err("malformed build-started must fail");
    }
}
