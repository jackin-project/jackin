// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

use super::*;
use anyhow::Context;
use jackin_process_directory::{CheckedInstallRoot, InstallFileKind};

fn private_install_directory() -> Result<tempfile::TempDir> {
    let home = std::env::var_os("HOME").context("HOME is required for the install fixture")?;
    let home = std::path::PathBuf::from(home).canonicalize()?;
    Ok(tempfile::Builder::new()
        .prefix("jackin-install-test-")
        .tempdir_in(home)?)
}

#[test]
fn generation_id_binds_request_and_owner_bytes() {
    let request = b"reviewed request\n";
    let first = generation_id(request, &"a".repeat(64));
    assert_eq!(first, generation_id(request, &"a".repeat(64)));
    assert_ne!(
        first,
        generation_id(b"different request\n", &"a".repeat(64))
    );
    assert_ne!(first, generation_id(request, &"b".repeat(64)));
}

#[test]
fn copied_owner_must_match_the_admitted_target_digest() -> Result<()> {
    let admitted = sha256(b"admitted executable");
    validate_owner_digest(&sha256(b"admitted executable"), &admitted)?;
    assert!(validate_owner_digest(&sha256(b"swapped executable"), &admitted).is_err());
    Ok(())
}

#[test]
fn record_publication_is_atomic_and_keeps_the_requested_mode() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let directory = super::fs::open_path(temporary.path())?;
    publish_record(&directory, "record", "first\n", 0o600)?;
    assert_eq!(fs::read(temporary.path().join("record"))?, b"first\n");
    let metadata = fs::symlink_metadata(temporary.path().join("record"))?;
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    publish_record(&directory, "record", "second\n", 0o600)?;
    assert_eq!(fs::read(temporary.path().join("record"))?, b"second\n");
    Ok(())
}

#[test]
fn failed_record_publication_preserves_destination_and_cleans_its_temporary() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let directory = super::fs::open_path(temporary.path())?;
    fs::create_dir(temporary.path().join("record"))?;
    assert!(publish_record(&directory, "record", "new record\n", 0o600).is_err());
    assert!(temporary.path().join("record").is_dir());
    let names = crate::fs_util::read_dir_sorted(temporary.path())?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    assert_eq!(names, [std::ffi::OsString::from("record")]);
    Ok(())
}

#[test]
fn descriptor_reads_reject_symlink_and_exclusive_file_creation_preserves_existing_data()
-> Result<()> {
    let temporary = tempfile::tempdir()?;
    let directory = super::fs::open_path(temporary.path())?;
    let target = temporary.path().join("target");
    fs::write(&target, b"protected")?;
    symlink(&target, temporary.path().join("linked"))?;
    read_file(&directory, "linked", 32, 0o600).expect_err("symlink reads must fail");
    assert!(create_file(&directory, "target", b"replacement", 0o600).is_err());
    assert_eq!(fs::read(target)?, b"protected");
    Ok(())
}

#[test]
fn generation_publication_is_idempotent_and_rejects_conflicting_content() -> Result<()> {
    let temporary = private_install_directory()?;
    fs::create_dir(temporary.path().join("generations"))?;
    fs::set_permissions(
        temporary.path().join("generations"),
        fs::Permissions::from_mode(0o700),
    )?;
    let root = temporary.path().join("generations");
    let root_handle = super::fs::open_path(&root)?;
    let owner = b"immutable owner executable";
    let owner_hash = sha256(owner);
    let generation = "c".repeat(64);
    let manifest = format!(
        "{MANIFEST_HEADER}\ngeneration={generation}\nowner-sha256={owner_hash}\nsource-tree={}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n",
        "a".repeat(40),
        "b".repeat(40),
        "d".repeat(64),
        "e".repeat(64),
        "f".repeat(64),
        "0".repeat(64),
        "1".repeat(64),
    )
    .into_bytes();
    publish_generation(&root_handle, &generation, owner, &manifest, &owner_hash)?;
    let canonical_root = fs::canonicalize(&root)?;
    let consumer = CheckedInstallRoot::from_absolute_directory(&canonical_root)?;
    consumer.open_file(
        std::path::Path::new(&format!("{generation}/install.manifest")),
        InstallFileKind::Script,
    )?;
    consumer.open_file(
        std::path::Path::new(&format!("{generation}/owner")),
        InstallFileKind::SealedCopy,
    )?;
    publish_generation(&root_handle, &generation, owner, &manifest, &owner_hash)?;
    assert!(
        publish_generation(
            &root_handle,
            &generation,
            b"different owner",
            &manifest,
            &owner_hash
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn generation_publication_does_not_create_or_replace_legacy_active_data() -> Result<()> {
    let temporary = private_install_directory()?;
    let directories = InstallDirectories::open(temporary.path())?;
    let active_path = temporary.path().join("jackin-precommit-owner/active");
    assert!(!active_path.exists());

    let owner = b"immutable owner executable";
    let owner_hash = sha256(owner);
    let source_tree = "a".repeat(40);
    let manifest_for = |generation: &str| {
        format!(
            "{MANIFEST_HEADER}\ngeneration={generation}\nowner-sha256={owner_hash}\nsource-tree={source_tree}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n",
            "b".repeat(40),
            "c".repeat(64),
            "d".repeat(64),
            "e".repeat(64),
            "f".repeat(64),
            "0".repeat(64),
        )
        .into_bytes()
    };

    let first_generation = "1".repeat(64);
    let first_manifest = manifest_for(&first_generation);
    publish_install_artifacts(
        &directories,
        &first_generation,
        owner,
        &first_manifest,
        &owner_hash,
        &source_tree,
    )?;
    assert!(!active_path.exists());

    let legacy_record = b"legacy generation pointer\n";
    fs::write(&active_path, legacy_record)?;
    fs::set_permissions(&active_path, fs::Permissions::from_mode(0o600))?;
    let second_generation = "2".repeat(64);
    let second_manifest = manifest_for(&second_generation);
    publish_install_artifacts(
        &directories,
        &second_generation,
        owner,
        &second_manifest,
        &owner_hash,
        &source_tree,
    )?;
    assert_eq!(fs::read(&active_path)?, legacy_record);
    Ok(())
}
