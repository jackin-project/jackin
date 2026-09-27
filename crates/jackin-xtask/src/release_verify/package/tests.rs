// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};
use tar::{Builder as TarBuilder, EntryType, Header};

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

fn release_archive(path: &Path, members: &[(&str, &[u8], u32, EntryType)]) {
    let encoder = GzEncoder::new(fs::File::create(path).unwrap(), Compression::default());
    let mut archive = TarBuilder::new(encoder);
    for (name, bytes, mode, entry_type) in members {
        let mut header = Header::new_gnu();
        let path = name.as_bytes();
        assert!(
            path.len() < 100,
            "test member path must fit the tar name field"
        );
        header.as_mut_bytes()[..path.len()].copy_from_slice(path);
        header.set_size(bytes.len() as u64);
        header.set_mode(*mode);
        header.set_mtime(0);
        header.set_entry_type(*entry_type);
        header.set_cksum();
        archive.append(&header, *bytes).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
}

/// Small static ELF64 that prints the matching package version on Linux.
/// It carries real `x86_64` ELF metadata and cannot run on macOS.
fn elf64_x86_64_version_binary(binary: &str, version: &str) -> Vec<u8> {
    const ELF_HEADER_SIZE: usize = 64;
    const PROGRAM_HEADER_SIZE: usize = 56;
    const CODE_OFFSET: usize = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;

    let message = format!("{binary} {version}\n").into_bytes();
    let message_len = u8::try_from(message.len()).expect("ELF fixture message fits in u8");
    let code = [
        0x48,
        0x8d,
        0x35,
        0x1a,
        0x00,
        0x00,
        0x00, // lea message(%rip), %rsi
        0xba,
        message_len,
        0x00,
        0x00,
        0x00, // mov message length, %edx
        0xbf,
        0x01,
        0x00,
        0x00,
        0x00, // mov 1, %edi
        0xb8,
        0x01,
        0x00,
        0x00,
        0x00, // mov write syscall, %eax
        0x0f,
        0x05, // syscall
        0xb8,
        0x3c,
        0x00,
        0x00,
        0x00, // mov exit syscall, %eax
        0x31,
        0xff, // xor %edi, %edi
        0x0f,
        0x05, // syscall
    ];
    let mut bytes = vec![0_u8; CODE_OFFSET];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2; // ELFCLASS64
    bytes[5] = 1; // little-endian
    bytes[6] = 1; // current ELF version
    bytes[16..18].copy_from_slice(&2_u16.to_le_bytes()); // ET_EXEC
    bytes[18..20].copy_from_slice(&62_u16.to_le_bytes()); // EM_X86_64
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[24..32].copy_from_slice(&(0x400000_u64 + CODE_OFFSET as u64).to_le_bytes());
    bytes[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    bytes[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
    bytes[54..56].copy_from_slice(&(PROGRAM_HEADER_SIZE as u16).to_le_bytes());
    bytes[56..58].copy_from_slice(&1_u16.to_le_bytes());
    bytes[58..60].copy_from_slice(&64_u16.to_le_bytes());

    let program_header = ELF_HEADER_SIZE;
    bytes[program_header..program_header + 4].copy_from_slice(&1_u32.to_le_bytes()); // PT_LOAD
    bytes[program_header + 4..program_header + 8].copy_from_slice(&5_u32.to_le_bytes()); // R|X
    bytes[program_header + 16..program_header + 24].copy_from_slice(&0x400000_u64.to_le_bytes());
    bytes[program_header + 24..program_header + 32].copy_from_slice(&0x400000_u64.to_le_bytes());
    let file_size = CODE_OFFSET + code.len() + message.len();
    bytes[program_header + 32..program_header + 40]
        .copy_from_slice(&(file_size as u64).to_le_bytes());
    bytes[program_header + 40..program_header + 48]
        .copy_from_slice(&(file_size as u64).to_le_bytes());
    bytes[program_header + 48..program_header + 56].copy_from_slice(&0x1000_u64.to_le_bytes());

    bytes.extend_from_slice(&code);
    bytes.extend_from_slice(&message);
    bytes
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
fn accepts_release_archive_with_exact_executable_x86_64_elf_members() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    const TARGET: &str = "x86_64-unknown-linux-gnu";
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

    verify_release_archive_members(&path, TARGET, &["jackin", "jackin-role"], VERSION).unwrap();
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

#[cfg(unix)]
#[test]
fn verifies_native_version_probe_output_exactly() {
    const VERSION: &str = "0.6.4-preview.1+0123456";
    let matching = format!("#!/bin/sh\nprintf 'jackin {VERSION}\\n'\n").into_bytes();
    verify_runnable_version(&matching, "jackin", VERSION).unwrap();

    let mismatching = format!("#!/bin/sh\nprintf 'jackin {VERSION}-wrong\\n'\n").into_bytes();
    let error = verify_runnable_version(&mismatching, "jackin", VERSION)
        .expect_err("a binary with a wrong --version response must fail closed");
    assert!(error.to_string().contains("output does not equal"));
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
fn source_checkout_accepts_admitted_old_sha_after_origin_main_advances() {
    let directory = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "--quiet"]);
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
    git(
        directory.path(),
        &[
            "config",
            &format!("url.file://{}.insteadOf", remote.path().display()),
            "https://github.com/jackin-project/jackin.git",
        ],
    );
    let commit = git(directory.path(), &["rev-parse", "HEAD"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );
    git(
        directory.path(),
        &["checkout", "--quiet", "--detach", &commit],
    );

    verify_source_checkout(directory.path(), &source_manifest(commit.clone())).unwrap();

    fs::write(directory.path().join("untracked"), "must fail\n").unwrap();
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("untracked source changes must fail closed");
    assert!(error.to_string().contains("not clean"));
    fs::remove_file(directory.path().join("untracked")).unwrap();

    fs::write(directory.path().join("tracked"), "hidden modification\n").unwrap();
    git(
        directory.path(),
        &["update-index", "--assume-unchanged", "tracked"],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("assume-unchanged tracked changes must fail closed");
    assert!(error.to_string().contains("assume-unchanged"));
    git(
        directory.path(),
        &["update-index", "--no-assume-unchanged", "tracked"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();

    fs::write(directory.path().join("tracked"), "hidden skip-worktree\n").unwrap();
    git(
        directory.path(),
        &["update-index", "--skip-worktree", "tracked"],
    );
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("skip-worktree tracked changes must fail closed");
    assert!(error.to_string().contains("skip-worktree"));
    git(
        directory.path(),
        &["update-index", "--no-skip-worktree", "tracked"],
    );
    fs::write(directory.path().join("tracked"), "tracked\n").unwrap();

    git(
        directory.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "advance"],
    );
    let advanced = git(directory.path(), &["rev-parse", "HEAD"]);
    git(
        directory.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );
    git(
        directory.path(),
        &["checkout", "--quiet", "--detach", &commit],
    );

    // A queued preview remains bound to its admitted source commit after main
    // advances. The producer's identity check must not substitute the latest
    // branch tip for the event SHA.
    verify_source_checkout(directory.path(), &source_manifest(commit.clone())).unwrap();
    let error = verify_source_checkout(directory.path(), &source_manifest(advanced))
        .expect_err("a manifest for another commit must still fail closed");
    assert!(
        error
            .to_string()
            .contains("does not match source checkout HEAD")
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
    let error = verify_source_checkout(directory.path(), &source_manifest(commit.clone()))
        .expect_err("HTTP GitHub remotes must fail closed");
    assert!(error.to_string().contains("GitHub repository URL"));
}
