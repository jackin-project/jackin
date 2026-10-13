// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn accepts_release_archive_with_exact_executable_x86_64_elf_members() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    const TARGET: &str = "x86_64-unknown-linux-gnu";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let broker = elf64_x86_64_version_binary("jackin-usage-broker", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
            ("jackin-usage-broker", &broker, 0o755, EntryType::Regular),
        ],
    );

    verify_release_archive_members(&path, TARGET, PAYLOADS[3].binaries, VERSION).unwrap();
}

#[test]
fn rejects_host_release_archive_missing_usage_broker() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
        ],
    );
    for payload in PAYLOADS.iter().filter(|payload| {
        payload.name.starts_with("jackin-") && !payload.name.starts_with("jackin-capsule-")
    }) {
        let error = verify_release_archive_members(
            &path,
            "x86_64-unknown-linux-gnu",
            payload.binaries,
            VERSION,
        )
        .expect_err("every host package requires the broker sidecar");
        assert!(error.to_string().contains("member set is incomplete"));
    }
}

#[test]
fn rejects_release_archive_missing_a_binary_member() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(&path, &[("jackin", &jackin, 0o755, EntryType::Regular)]);

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("an archive missing jackin-role must fail closed");
    assert!(error.to_string().contains("member set is incomplete"));
}

#[test]
fn rejects_release_archive_with_duplicate_binary_member() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
        ],
    );

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("duplicate archive members must fail closed");
    assert!(error.to_string().contains("contains duplicate jackin"));
}

#[test]
fn rejects_release_archive_with_unexpected_member() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
            ("README", b"extra", 0o644, EntryType::Regular),
        ],
    );

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("unexpected archive members must fail closed");
    assert!(
        error
            .to_string()
            .contains("unexpected release archive member: README")
    );
}

#[test]
fn rejects_release_archive_path_traversal_member() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o755, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
            ("../jackin-role", &role, 0o755, EntryType::Regular),
        ],
    );

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("path traversal archive members must fail closed");
    assert!(
        error
            .to_string()
            .contains("unexpected release archive member")
    );
}

#[test]
fn rejects_release_archive_directory_in_place_of_binary() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", b"", 0o755, EntryType::Directory),
            ("jackin-role", &role, 0o755, EntryType::Regular),
        ],
    );

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("directories cannot satisfy binary members");
    assert!(error.to_string().contains("is not a regular file: jackin"));
}

#[test]
fn rejects_release_archive_non_executable_binary_member() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let directory = tempfile::tempdir().unwrap();
    let jackin = elf64_x86_64_version_binary("jackin", VERSION);
    let role = elf64_x86_64_version_binary("jackin-role", VERSION);
    let path = directory.path().join("release.tar.gz");
    release_archive(
        &path,
        &[
            ("jackin", &jackin, 0o644, EntryType::Regular),
            ("jackin-role", &role, 0o755, EntryType::Regular),
        ],
    );

    let error = verify_release_archive_members(
        &path,
        "x86_64-unknown-linux-gnu",
        &["jackin", "jackin-role"],
        VERSION,
    )
    .expect_err("non-executable binaries must fail closed");
    assert!(error.to_string().contains("is not executable: jackin"));
}
