// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::release_archive::{
    ArchivePackage,
    layout::{ArchiveLayout, ArchiveProfile},
};

fn select_preview(fixture: &mut Fixture) {
    let identity = crate::release_archive::payload_identity(&fixture.archive).unwrap();
    fixture.layout = ArchiveLayout::new(
        fixture.layout.root(),
        identity.target,
        ArchiveProfile::Preview,
    )
    .unwrap();
    fixture.release = fixture.layout.target_output();
    fs::create_dir_all(&fixture.release).unwrap();
    for binary in identity.binaries {
        fs::write(fixture.release.join(binary), format!("preview-{binary}")).unwrap();
    }
    fixture.records = String::from_utf8(fixture.records.clone())
        .unwrap()
        .replace("/release/", "/preview/")
        .into_bytes();
    let package = if identity.package == "jackin" {
        ArchivePackage::Jackin
    } else {
        ArchivePackage::JackinCapsule
    };
    crate::release_archive::package(package, &fixture.layout, fixture.archive.parent().unwrap())
        .unwrap();
}

fn append_host_input(fixture: &mut Fixture) {
    let mut rows: Vec<Value> = String::from_utf8(fixture.records.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut host = rows[0].clone();
    host["target"] = json!({"kind":["proc-macro"],"name":"host_macro"});
    let path = fixture
        .layout
        .host_output()
        .join("deps/libhost_macro.dylib");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"host input").unwrap();
    host["filenames"] = json!([path]);
    rows.insert(1, host);
    fixture.records = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
}

#[test]
fn preview_archives_and_sboms_use_selected_bytes_and_host_profile_for_all_pairs() {
    for pair in crate::release_archive::preview_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let mut fixture = fixture(dir.path(), pair.package, pair.target);
        let stable_bytes = fs::read(fixture.release.join(pair.binaries[0])).unwrap();
        select_preview(&mut fixture);
        append_host_input(&mut fixture);
        let output = dir.path().join("bom.json");
        write_fixture(&fixture, &output, "0.6.4-preview.42+aaaaaaa").unwrap();
        verify(&fixture.archive, &output).unwrap();
        let identity = crate::release_archive::payload_identity(&fixture.archive).unwrap();
        let members = archive::members(&fixture.archive, &identity).unwrap();
        assert_ne!(members[pair.binaries[0]].sha256, digest(&stable_bytes));
        for binary in pair.binaries {
            assert_eq!(
                members[*binary].sha256,
                digest(format!("preview-{binary}").as_bytes())
            );
        }
        let text = fs::read_to_string(output).unwrap();
        assert!(text.contains("host_macro"));
        assert!(text.contains("host-build"));
        assert!(text.contains("proc-macro"));
        assert!(text.contains("target"));
    }
}

#[test]
fn both_profiles_reject_other_profile_and_wrong_target_records_before_file_reads() {
    for profile in [ArchiveProfile::Release, ArchiveProfile::Preview] {
        let dir = tempfile::tempdir().unwrap();
        let mut fixture = fixture(dir.path(), "jackin", "aarch64-apple-darwin");
        if profile == ArchiveProfile::Preview {
            select_preview(&mut fixture);
        }
        materialize_records(&fixture.records);
        let original = String::from_utf8(fixture.records.clone()).unwrap();
        let selected = if profile == ArchiveProfile::Release {
            "release"
        } else {
            "preview"
        };
        let other = if profile == ArchiveProfile::Release {
            "preview"
        } else {
            "release"
        };
        for replacement in [
            format!("aarch64-apple-darwin/{other}"),
            format!("x86_64-apple-darwin/{selected}"),
        ] {
            fixture.records = original
                .replace(&format!("aarch64-apple-darwin/{selected}"), &replacement)
                .into_bytes();
            materialize_records(&fixture.records);
            let error = write_fixture(&fixture, &dir.path().join("bad.json"), "0.6.4").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("wrong target or host-build path"),
                "{error:#}"
            );
        }
    }
}

#[test]
fn preview_host_and_target_custody_cannot_mix_or_traverse_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    select_preview(&mut fixture);
    append_host_input(&mut fixture);
    let original = String::from_utf8(fixture.records.clone()).unwrap();
    let host = fixture
        .layout
        .host_output()
        .join("deps/libhost_macro.dylib");
    let mut cases = vec![
        (
            original.replace(
                "build-jackin-capsule/preview/deps",
                "build-jackin-capsule/release/deps",
            ),
            "wrong target or host-build path",
        ),
        (
            original.replace(
                "/preview/jackin-capsule",
                "/preview/../preview/jackin-capsule",
            ),
            "noncanonical Cargo artifact path",
        ),
    ];
    let rows: Vec<Value> = original
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut mixed = rows.clone();
    mixed[1]["filenames"] = json!([host, fixture.release.join("deps/target.rlib")]);
    cases.push((
        mixed
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        "wrong target or host-build path",
    ));
    let mut wrong_executable = rows;
    wrong_executable[1]["target"]["kind"] = json!(["custom-build"]);
    wrong_executable[1]["executable"] = json!(fixture.release.join("foreign-build"));
    cases.push((
        wrong_executable
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        "mixed target/profile custody",
    ));
    for (records, reason) in cases {
        fixture.records = records.into_bytes();
        let error = write_fixture(&fixture, &dir.path().join("bad.json"), "0.6.4").unwrap_err();
        assert!(error.to_string().contains(reason), "{error:#}");
    }
}

#[cfg(unix)]
#[test]
fn preview_rejects_symlink_alias_into_release_even_for_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    select_preview(&mut fixture);
    let selected = fixture.release.join("jackin-capsule");
    let stable = fixture
        .layout
        .root()
        .join(fixture.layout.target())
        .join("release/jackin-capsule");
    fs::write(&stable, fs::read(&selected).unwrap()).unwrap();
    fs::remove_file(&selected).unwrap();
    std::os::unix::fs::symlink(stable, selected).unwrap();
    let error = write_fixture(&fixture, &dir.path().join("bad.json"), "0.6.4").unwrap_err();
    assert!(error.to_string().contains("symlink alias"), "{error:#}");
}

fn materialize_records(records: &[u8]) {
    for row in String::from_utf8_lossy(records).lines() {
        let value: Value = serde_json::from_str(row).unwrap();
        if let Some(paths) = value["filenames"].as_array() {
            for path in paths {
                let path = Path::new(path.as_str().unwrap());
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                if !path.exists() {
                    fs::write(path, b"present alternate build input").unwrap();
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn repointed_configured_alias_cannot_move_packaging_or_sbom_to_another_root() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture(dir.path(), "jackin-capsule", "aarch64-unknown-linux-gnu");
    select_preview(&mut fixture);
    let alias = dir.path().join("configured-root");
    std::os::unix::fs::symlink(fixture.layout.root(), &alias).unwrap();
    let held =
        ArchiveLayout::new(&alias, fixture.layout.target(), ArchiveProfile::Preview).unwrap();
    let other = dir.path().join("other-root");
    let alternate = other.join(fixture.layout.target()).join("preview");
    fs::create_dir_all(&alternate).unwrap();
    fs::write(alternate.join("jackin-capsule"), b"different root bytes").unwrap();
    fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(other, alias).unwrap();
    assert_eq!(held.root(), fixture.layout.root());
    fixture.layout = held;
    crate::release_archive::package(ArchivePackage::JackinCapsule, &fixture.layout, dir.path())
        .unwrap();
    let output = dir.path().join("bom.json");
    write_fixture(&fixture, &output, "0.6.4-preview.42+aaaaaaa").unwrap();
    verify(&fixture.archive, &output).unwrap();
    let identity = crate::release_archive::payload_identity(&fixture.archive).unwrap();
    let members = archive::members(&fixture.archive, &identity).unwrap();
    assert_eq!(
        members["jackin-capsule"].sha256,
        digest(b"preview-jackin-capsule")
    );
    assert_ne!(
        members["jackin-capsule"].sha256,
        digest(b"different root bytes")
    );
}
