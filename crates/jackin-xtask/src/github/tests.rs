// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use super::{is_release_asset, release_already_exists, release_assets, release_missing};

#[test]
fn classifies_release_lookup_failures() {
    assert!(release_missing("release not found"));
    assert!(release_missing("HTTP 404: Not Found"));
    assert!(!release_missing("HTTP 503: unavailable"));
    assert!(release_already_exists(
        "a release with the same tag name already exists: preview"
    ));
    assert!(!release_already_exists("permission denied"));
}

#[test]
fn selects_only_public_preview_assets() {
    for accepted in [
        "jackin-x86_64-unknown-linux-gnu.tar.gz",
        "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.sha256",
        "jackin-aarch64-apple-darwin.tar.gz.bundle",
        "jackin-aarch64-apple-darwin.tar.gz.sbom.json",
        "capsule-manifest.json",
        "capsule-manifest.json.bundle",
    ] {
        assert!(is_release_asset(accepted.as_ref()), "{accepted}");
    }
    for rejected in [
        "cargo-timing.html".to_owned(),
        "notes.txt".to_owned(),
        ["jackin", "zip"].join("."),
    ] {
        assert!(!is_release_asset(rejected.as_ref()), "{rejected}");
    }
}

#[test]
fn sorts_selected_assets() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("jackin-z.tar.gz"), []).unwrap();
    fs::write(directory.path().join("jackin-a.tar.gz"), []).unwrap();
    fs::write(directory.path().join("private.txt"), []).unwrap();
    let assets = release_assets(directory.path()).unwrap();
    let names = assets
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, ["jackin-a.tar.gz", "jackin-z.tar.gz"]);
}
