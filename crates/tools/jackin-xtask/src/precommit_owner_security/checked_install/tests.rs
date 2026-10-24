// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::super::RECEIPT_HEADER;
use super::{
    ENTRY_NAME, HELPER_NAME, OWNER_NAME, RECEIPT_LIMIT, RECEIPT_NAME, clone_checked_file,
    inspect_checked_install, read_checked_receipt,
};
use anyhow::{Context, Result, anyhow, ensure};
use jackin_process_directory::{CheckedInstallFile, CheckedInstallRoot, InstallFileKind};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom},
    os::unix::fs::{FileExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

const OWNER_BYTES: &[u8] = b"checked owner executable bytes\n";
const ENTRY_BYTES: &[u8] = b"checked entry script bytes\n";
const HELPER_BYTES: &[u8] = b"checked common helper bytes\n";
const SOURCE_CLAIM: &[u8] = b"an arbitrary source claim, not measured here";
const CARGO_CLAIM: &[u8] = b"an arbitrary cargo claim, not measured here";
const RUSTC_CLAIM: &[u8] = b"an arbitrary rustc claim, not measured here";
const TARGET_CLAIM: &[u8] = b"an arbitrary target claim, not measured here";

struct Fixture {
    temporary: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let cwd = std::env::current_dir().context("reading physical test working directory")?;
        let temporary = tempfile::Builder::new()
            .prefix(".jackin-checked-install-")
            .tempdir_in(cwd)
            .context("creating fixture below the checked-out working directory")?;
        let root = temporary.path().join("install-root");
        fs::create_dir(&root).context("creating private fixture install root")?;
        set_mode(&root, 0o700)?;
        write_file(&root.join(OWNER_NAME), OWNER_BYTES, 0o755)?;
        write_file(&root.join(ENTRY_NAME), ENTRY_BYTES, 0o400)?;
        write_file(&root.join(HELPER_NAME), HELPER_BYTES, 0o400)?;
        write_file(&root.join(RECEIPT_NAME), &valid_receipt(), 0o600)?;
        Ok(Self { temporary, root })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn rewrite_receipt(&self, bytes: &[u8]) -> Result<()> {
        write_file(&self.path(RECEIPT_NAME), bytes, 0o600)
    }
}

fn digest_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn valid_receipt() -> Vec<u8> {
    let owner = digest_hex(OWNER_BYTES);
    let entry = digest_hex(ENTRY_BYTES);
    let helper = digest_hex(HELPER_BYTES);
    let source = digest_hex(SOURCE_CLAIM);
    let cargo = digest_hex(CARGO_CLAIM);
    let rustc = digest_hex(RUSTC_CLAIM);
    let target = digest_hex(TARGET_CLAIM);
    let build = digest_hex(format!("cargo={cargo}\nrustc={rustc}\ntarget={target}\n").as_bytes());
    format!(
        "{RECEIPT_HEADER}\nowner-sha256={owner}\nentry-sha256={entry}\nhelper-sha256={helper}\nsource-sha256={source}\ncargo-sha256={cargo}\nrustc-sha256={rustc}\ntarget-sha256={target}\nbuild-sha256={build}\n"
    )
    .into_bytes()
}

fn write_file(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("writing fixture file {path:?}"))?;
    set_mode(path, mode)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting fixture mode {mode:o} on {path:?}"))
}

fn open_checked_receipt(fixture: &Fixture) -> Result<(CheckedInstallRoot, CheckedInstallFile)> {
    let root = CheckedInstallRoot::from_absolute_directory(&fixture.root)
        .context("opening fixture root through the production B2 boundary")?;
    let receipt = root
        .open_file(Path::new(RECEIPT_NAME), InstallFileKind::Receipt)
        .context("opening original checked receipt descriptor")?;
    Ok((root, receipt))
}

fn descriptor_bytes(file: &File) -> Result<Vec<u8>> {
    let length = usize::try_from(file.metadata()?.len())?;
    let mut bytes = vec![0_u8; length];
    let mut total = 0_usize;
    while total < bytes.len() {
        let offset = u64::try_from(total)?;
        let read = file.read_at(&mut bytes[total..], offset)?;
        if read == 0 {
            return Err(anyhow!(
                "fixture descriptor ended before its recorded length"
            ));
        }
        total = total
            .checked_add(read)
            .ok_or_else(|| anyhow!("fixture descriptor length overflow"))?;
    }
    Ok(bytes)
}

#[test]
fn checked_install_accepts_all_artifacts_without_changing_descriptor_offsets() -> Result<()> {
    let fixture = Fixture::new()?;
    let inspected = inspect_checked_install(&fixture.root)?;
    let mut offset_probes = [
        inspected.owner.file().try_clone()?,
        inspected.receipt.file().try_clone()?,
        inspected.entry.file().try_clone()?,
        inspected.helper.file().try_clone()?,
    ];
    let offsets = [3_u64, 5, 7, 9];
    for (probe, offset) in offset_probes.iter_mut().zip(offsets) {
        probe.seek(SeekFrom::Start(offset))?;
    }

    inspected.recheck()?;

    for (probe, offset) in offset_probes.iter_mut().zip(offsets) {
        assert_eq!(probe.stream_position()?, offset);
    }
    assert_eq!(inspected.source_hex(), digest_hex(SOURCE_CLAIM));
    Ok(())
}

#[test]
fn checked_receipt_rejects_malformed_and_oversized_original_descriptors() -> Result<()> {
    let malformed_fixture = Fixture::new()?;
    let mut malformed = valid_receipt();
    malformed.truncate(malformed.len() - 1);
    malformed_fixture.rewrite_receipt(&malformed)?;
    let (_root, checked) = open_checked_receipt(&malformed_fixture)?;
    let opened = clone_checked_file(&checked, "receipt", RECEIPT_LIMIT as u64)?;
    let error = read_checked_receipt(&checked, &opened)
        .err()
        .ok_or_else(|| anyhow!("malformed receipt read through checked descriptor was accepted"))?;
    assert!(format!("{error:#}").contains("final newline"));
    assert_eq!(fs::read(malformed_fixture.path(RECEIPT_NAME))?, malformed);
    assert_eq!(fs::read(malformed_fixture.path(OWNER_NAME))?, OWNER_BYTES);

    let oversized_fixture = Fixture::new()?;
    let oversized = vec![b'x'; RECEIPT_LIMIT + 1];
    oversized_fixture.rewrite_receipt(&oversized)?;
    let (_root, checked) = open_checked_receipt(&oversized_fixture)?;
    let error = clone_checked_file(&checked, "receipt", RECEIPT_LIMIT as u64)
        .err()
        .ok_or_else(|| anyhow!("oversized checked receipt descriptor was accepted"))?;
    assert!(format!("{error:#}").contains("byte limit"));
    assert_eq!(fs::read(oversized_fixture.path(RECEIPT_NAME))?, oversized);
    assert_eq!(fs::read(oversized_fixture.path(ENTRY_NAME))?, ENTRY_BYTES);
    Ok(())
}

#[test]
fn checked_artifact_digest_mismatches_identify_each_fixed_role() -> Result<()> {
    for (field, expected, role) in [
        ("owner-sha256", digest_hex(OWNER_BYTES), "owner"),
        ("entry-sha256", digest_hex(ENTRY_BYTES), "entry"),
        ("helper-sha256", digest_hex(HELPER_BYTES), "helper"),
    ] {
        let fixture = Fixture::new()?;
        let receipt = String::from_utf8(valid_receipt())?;
        let original = format!("{field}={expected}");
        let replacement = format!("{field}={}", "0".repeat(64));
        let mismatched = receipt.replacen(&original, &replacement, 1);
        ensure!(mismatched != receipt, "fixture did not contain {field}");
        fixture.rewrite_receipt(mismatched.as_bytes())?;

        let error = inspect_checked_install(&fixture.root)
            .err()
            .ok_or_else(|| anyhow!("mismatched {role} digest was accepted"))?;
        assert!(
            format!("{error:#}").contains(&format!("{role} artifact digest")),
            "mismatch did not identify {role}: {error:#}"
        );
        assert_eq!(fs::read(fixture.path(OWNER_NAME))?, OWNER_BYTES);
        assert_eq!(fs::read(fixture.path(ENTRY_NAME))?, ENTRY_BYTES);
        assert_eq!(fs::read(fixture.path(HELPER_NAME))?, HELPER_BYTES);
    }
    Ok(())
}

#[test]
fn checked_leaf_replacement_is_detected_without_losing_original_descriptor() -> Result<()> {
    let fixture = Fixture::new()?;
    let inspected = inspect_checked_install(&fixture.root)?;
    let entry_path = fixture.path(ENTRY_NAME);
    let original_path = fixture.path("original-entry");
    fs::rename(&entry_path, &original_path)?;
    write_file(&entry_path, b"replacement entry bytes\n", 0o400)?;

    let error = inspected
        .recheck()
        .err()
        .ok_or_else(|| anyhow!("replacement at the checked entry name was accepted"))?;
    assert!(format!("{error:#}").contains("entry installation name"));
    assert_eq!(descriptor_bytes(inspected.entry.file())?, ENTRY_BYTES);
    assert_eq!(fs::read(&original_path)?, ENTRY_BYTES);
    assert_eq!(fs::read(&entry_path)?, b"replacement entry bytes\n");
    Ok(())
}

#[test]
fn rejected_install_namespaces_preserve_original_files() -> Result<()> {
    let linked_root = Fixture::new()?;
    let root_alias = linked_root.temporary.path().join("root-alias");
    symlink(&linked_root.root, &root_alias)?;
    let error = inspect_checked_install(&root_alias)
        .err()
        .ok_or_else(|| anyhow!("symlink physical root was accepted"))?;
    assert!(format!("{error:#}").contains("root"));
    assert_eq!(fs::read(linked_root.path(OWNER_NAME))?, OWNER_BYTES);
    assert_eq!(fs::read(linked_root.path(RECEIPT_NAME))?, valid_receipt());

    let unsafe_root = Fixture::new()?;
    set_mode(&unsafe_root.root, 0o770)?;
    let error = inspect_checked_install(&unsafe_root.root)
        .err()
        .ok_or_else(|| anyhow!("group-writable physical root was accepted"))?;
    assert!(format!("{error:#}").contains("mode"));
    assert_eq!(fs::read(unsafe_root.path(OWNER_NAME))?, OWNER_BYTES);
    assert_eq!(fs::read(unsafe_root.path(RECEIPT_NAME))?, valid_receipt());
    assert_eq!(fs::read(unsafe_root.path(ENTRY_NAME))?, ENTRY_BYTES);
    assert_eq!(fs::read(unsafe_root.path(HELPER_NAME))?, HELPER_BYTES);

    let hard_linked = Fixture::new()?;
    let external_link = hard_linked.temporary.path().join("owner-alias");
    fs::hard_link(hard_linked.path(OWNER_NAME), &external_link)?;
    let error = inspect_checked_install(&hard_linked.root)
        .err()
        .ok_or_else(|| anyhow!("hard-linked owner artifact was accepted"))?;
    assert!(format!("{error:#}").contains("hard link"));
    assert_eq!(fs::read(hard_linked.path(OWNER_NAME))?, OWNER_BYTES);
    assert_eq!(fs::read(external_link)?, OWNER_BYTES);

    let linked_leaf = Fixture::new()?;
    let target = linked_leaf.temporary.path().join("entry-target");
    fs::write(&target, ENTRY_BYTES)?;
    let entry_path = linked_leaf.path(ENTRY_NAME);
    fs::remove_file(&entry_path)?;
    symlink(&target, &entry_path)?;
    let error = inspect_checked_install(&linked_leaf.root)
        .err()
        .ok_or_else(|| anyhow!("symlink entry artifact was accepted"))?;
    assert!(format!("{error:#}").contains("entry"));
    assert!(fs::symlink_metadata(&entry_path)?.file_type().is_symlink());
    assert_eq!(fs::read(&target)?, ENTRY_BYTES);
    assert_eq!(fs::read(&entry_path)?, ENTRY_BYTES);
    assert_eq!(fs::read(linked_leaf.path(OWNER_NAME))?, OWNER_BYTES);
    Ok(())
}

#[test]
fn self_consistent_source_and_build_claims_remain_inspection_only() -> Result<()> {
    let fixture = Fixture::new()?;
    let inspected = inspect_checked_install(&fixture.root)?;
    let canonical_build = format!(
        "cargo={}\nrustc={}\ntarget={}\n",
        digest_hex(CARGO_CLAIM),
        digest_hex(RUSTC_CLAIM),
        digest_hex(TARGET_CLAIM)
    );

    assert_eq!(
        inspected.receipt_content.source.as_hex(),
        digest_hex(SOURCE_CLAIM)
    );
    assert_eq!(
        inspected.receipt_content.build.as_hex(),
        digest_hex(canonical_build.as_bytes())
    );
    let limitations = String::from(super::super::INSTALL_INSPECTION_LIMITATIONS);
    assert!(limitations.contains("source closure"));
    assert!(limitations.contains("build authority"));
    assert!(limitations.contains("running image"));
    assert!(limitations.contains("execution trust"));
    Ok(())
}
