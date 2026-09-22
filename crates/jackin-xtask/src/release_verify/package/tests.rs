// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use sha2::{Digest, Sha256};

use super::*;

const CAPSULE_VERSION: &str = "0.6.4-preview.1+0123456";

fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = crate::cmd::command("git");
    command.arg("-C").arg(directory).args(args);
    String::from_utf8(crate::cmd::output(&mut command).unwrap())
        .unwrap()
        .trim()
        .to_owned()
}

fn source_manifest(source_commit: String) -> PackageManifest {
    PackageManifest {
        assets: Vec::new(),
        schema: MANIFEST_SCHEMA.to_owned(),
        source_commit,
        source_ref: SOURCE_REF.to_owned(),
        source_repository: SOURCE_REPOSITORY.to_owned(),
        supporting_assets: Vec::new(),
        version: String::new(),
    }
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn write_payloads(directory: &Path) -> BTreeMap<String, String> {
    PAYLOADS
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let bytes = format!("payload-{index}").into_bytes();
            let path = directory.join(payload.name);
            fs::write(&path, &bytes).unwrap();
            (payload.name.to_owned(), digest(&bytes))
        })
        .collect()
}

#[test]
fn preview_contract_has_six_payloads_and_twenty_one_supporting_assets() {
    assert_eq!(PAYLOADS.len(), 6);
    assert_eq!(SUPPORTING_ASSETS.len(), 21);
    assert_eq!(expected_file_names().len(), 29);
}

#[test]
fn rejects_a_payload_reclassified_as_supporting_asset() {
    let directory = tempfile::tempdir().unwrap();
    let payload_digests = write_payloads(directory.path());
    let mut assets = PAYLOADS
        .iter()
        .map(|payload| PackageAsset {
            name: payload.name.to_owned(),
            sha256: payload_digests.get(payload.name).unwrap().clone(),
        })
        .collect::<Vec<_>>();
    assets[5].name = "SHA256SUMS".to_owned();

    let error = verify_manifest_assets(
        directory.path(),
        &assets,
        &PAYLOADS
            .iter()
            .map(|payload| payload.name)
            .collect::<Vec<_>>(),
        "payload",
    )
    .expect_err("a supporting asset must not replace a payload");
    assert!(error.to_string().contains("unexpected payload"));
}

#[test]
fn rejects_missing_supporting_asset() {
    let directory = tempfile::tempdir().unwrap();
    let assets = SUPPORTING_ASSETS
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let bytes = format!("support-{index}").into_bytes();
            if index != 20 {
                fs::write(directory.path().join(name), &bytes).unwrap();
            }
            PackageAsset {
                name: (*name).to_owned(),
                sha256: digest(&bytes),
            }
        })
        .collect::<Vec<_>>();

    let error = verify_manifest_assets(directory.path(), &assets, &SUPPORTING_ASSETS, "support")
        .expect_err("a missing supporting asset must fail closed");
    assert!(error.to_string().contains("stating support"));
}

#[test]
fn accepts_exact_sha256_sums_for_all_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let payload_digests = write_payloads(directory.path());
    let sums = PAYLOADS
        .iter()
        .map(|payload| format!("{}  {}", payload_digests[payload.name], payload.name))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(directory.path().join("SHA256SUMS"), format!("{sums}\n")).unwrap();

    verify_sha256_sums(directory.path(), &payload_digests).unwrap();
}

#[test]
fn parses_capsule_manifest_and_binds_exact_payload_targets_before_bundle_check() {
    let directory = tempfile::tempdir().unwrap();
    let payload_digests = write_payloads(directory.path());
    let expected = expected_capsule_targets(&payload_digests).unwrap();
    let manifest_path = directory.path().join("capsule-manifest.json");
    let bundle_path = directory.path().join("capsule-manifest.json.bundle");
    fs::write(
        &manifest_path,
        serde_json::json!({"targets": expected, "version": CAPSULE_VERSION}).to_string(),
    )
    .unwrap();
    fs::write(&bundle_path, b"deterministic bundle fixture").unwrap();

    let mut verified_paths = None;
    verify_capsule_manifest_with(
        directory.path(),
        CAPSULE_VERSION,
        &payload_digests,
        |manifest, bundle| {
            verified_paths = Some((manifest.to_owned(), bundle.to_owned()));
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(
        verified_paths,
        Some((manifest_path, bundle_path)),
        "bundle verification must receive the parsed manifest and its exact sidecar"
    );
}

#[test]
fn rejects_capsule_manifest_schema_extensions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capsule-manifest.json");
    fs::write(
        &path,
        serde_json::json!({
            "targets": {},
            "version": CAPSULE_VERSION,
            "unexpected": "must be rejected",
        })
        .to_string(),
    )
    .unwrap();

    let error = read_capsule_manifest(&path).expect_err("unknown manifest fields must fail closed");
    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn rejects_tampered_sha256_sums() {
    let directory = tempfile::tempdir().unwrap();
    let payload_digests = write_payloads(directory.path());
    let sums = PAYLOADS
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let checksum = if index == 0 {
                "0000000000000000000000000000000000000000000000000000000000000000"
            } else {
                &payload_digests[payload.name]
            };
            format!("{checksum}  {}", payload.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(directory.path().join("SHA256SUMS"), format!("{sums}\n")).unwrap();

    let error = verify_sha256_sums(directory.path(), &payload_digests)
        .expect_err("tampered SHA256SUMS must fail closed");
    assert!(error.to_string().contains("SHA256SUMS digest disagrees"));
}

#[test]
fn rejects_capsule_target_digest_swap() {
    let expected = BTreeMap::from([
        (
            "aarch64-unknown-linux-gnu".to_owned(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        ),
        (
            "x86_64-unknown-linux-gnu".to_owned(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        ),
    ]);
    let capsule = CapsuleManifest {
        version: "0.6.4-preview.1+0123456".to_owned(),
        targets: BTreeMap::from([
            (
                "aarch64-unknown-linux-gnu".to_owned(),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            ),
            (
                "x86_64-unknown-linux-gnu".to_owned(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            ),
        ]),
    };

    let error = validate_capsule_manifest(&capsule, &capsule.version, &expected)
        .expect_err("capsule target mapping swap must fail closed");
    assert!(error.to_string().contains("target mapping"));
}

#[test]
fn checks_cross_arch_elf_metadata_without_running_it() {
    let mut bytes = vec![0_u8; 20];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());

    verify_binary_metadata(&bytes, "aarch64-unknown-linux-gnu").unwrap();
    assert!(!target_is_runnable("aarch64-unknown-linux-gnu") || cfg!(target_arch = "aarch64"));
}

#[test]
fn rejects_cross_arch_binary_metadata_mismatch() {
    let mut bytes = vec![0_u8; 20];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[18..20].copy_from_slice(&62_u16.to_le_bytes());

    let error = verify_binary_metadata(&bytes, "aarch64-unknown-linux-gnu")
        .expect_err("x86_64 metadata must not pass the arm64 target check");
    assert!(error.to_string().contains("does not match target"));
}

#[test]
fn validates_source_bound_preview_version() {
    validate_preview_version(
        "0.6.4-preview.123+0123456",
        "0123456789abcdef0123456789abcdef01234567",
    )
    .unwrap();
    assert!(
        validate_preview_version(
            "0.6.4-preview.123+fedcba9",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .is_err()
    );
}

#[test]
fn source_checkout_requires_clean_tree_and_fetched_main_identity() {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "test"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();
    git(directory.path(), &["add", "tracked"]);
    git(directory.path(), &["commit", "--quiet", "-m", "seed"]);
    git(
        directory.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/jackin-project/jackin.git",
        ],
    );
    let commit = git(directory.path(), &["rev-parse", "HEAD"]);
    git(
        directory.path(),
        &["update-ref", "refs/remotes/origin/main", &commit],
    );

    verify_source_checkout(directory.path(), &source_manifest(commit.clone())).unwrap();

    fs::write(directory.path().join("untracked"), "must fail\n").unwrap();
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("untracked source changes must fail closed");
    assert!(error.to_string().contains("not clean"));
    fs::remove_file(directory.path().join("untracked")).unwrap();

    git(
        directory.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "advance"],
    );
    let advanced = git(directory.path(), &["rev-parse", "HEAD"]);
    let error = verify_source_checkout(directory.path(), &source_manifest(advanced.clone()))
        .expect_err("source not equal to fetched origin/main must fail closed");
    assert!(error.to_string().contains("origin/main"));
    git(
        directory.path(),
        &["update-ref", "refs/remotes/origin/main", &advanced],
    );

    git(
        directory.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "http://github.com/jackin-project/jackin.git",
        ],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(advanced))
        .expect_err("HTTP GitHub remotes must fail closed");
    assert!(error.to_string().contains("GitHub repository URL"));
}
