// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Durable immutable-generation publication for the native hook owner.

use std::{fs::File, os::unix::fs::PermissionsExt};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use super::bundle::{self, VerifiedBootstrapInstall};
mod fs;
use self::fs::{
    InstallDirectories, create_directory, create_file, exists, open_directory, open_lock,
    read_file, remove_directory, remove_file, rename, sync, validate_acl_sealed_directory,
};

#[cfg(test)]
mod tests;

const OWNER_LIMIT: u64 = 256 * 1024 * 1024;
const MANIFEST_HEADER: &str = "jackin-native-precommit-install/1";

pub(super) fn validate_hook_config_custody(
    common_path: &std::path::Path,
    selected_path: &std::path::Path,
) -> Result<()> {
    fs::validate_hook_config_custody(common_path, selected_path)
}

pub(super) fn install(verified: VerifiedBootstrapInstall) -> Result<String> {
    bundle::recheck_live(&verified)?;
    let directories = InstallDirectories::open(&verified.common_git_dir)?;
    let lock = open_lock(&directories.root)?;
    lock.lock().context("locking native hook installation")?;
    bundle::recheck_live(&verified)?;
    let owner = read_source_owner(&verified.target_path)?;
    let owner_sha256 = sha256(&owner);
    validate_owner_digest(&owner_sha256, &verified.target_sha256)?;
    let generation = generation_id(&verified.request_bytes, &owner_sha256);
    let manifest = render_manifest(&verified, &generation, &owner_sha256);
    crate::precommit_snapshot::validate_provision_manifest(&manifest)?;
    publish_generation(
        &directories.generations,
        &generation,
        &owner,
        &manifest,
        &owner_sha256,
    )?;
    publish_pending(&directories.pending, &generation, &verified.source_tree)?;
    bundle::recheck_live(&verified)?;
    validate_current_active(&directories.root, &directories.generations)?;
    publish_record(
        &directories.root,
        "active",
        &format!("{generation}\n"),
        0o600,
    )?;
    sync(&directories.root).context("syncing native hook owner root")?;
    Ok(generation)
}

fn validate_owner_digest(actual: &str, admitted: &str) -> Result<()> {
    ensure!(
        actual == admitted,
        "copied hook owner differs from the admitted build artifact"
    );
    Ok(())
}

fn read_source_owner(path: &std::path::Path) -> Result<Vec<u8>> {
    let owner = bundle::read_executable(path)?;
    ensure!(!owner.is_empty(), "native hook owner executable is empty");
    Ok(owner)
}

fn publish_generation(
    root: &File,
    generation: &str,
    owner: &[u8],
    manifest: &[u8],
    owner_hash: &str,
) -> Result<()> {
    if exists(root, generation)? {
        return validate_existing_generation(root, generation, owner_hash, owner, manifest);
    }
    let staging = format!(".staging-{}", uuid::Uuid::new_v4());
    let staging_dir = create_directory(root, &staging, 0o700)?;
    let mut published = false;
    let result = (|| {
        create_file(&staging_dir, "owner", owner, 0o500)?;
        create_file(&staging_dir, "install.manifest", manifest, 0o400)?;
        sync(&staging_dir).context("syncing staged generation")?;
        staging_dir.set_permissions(std::fs::Permissions::from_mode(0o500))?;
        validate_acl_sealed_directory(&staging_dir)?;
        sync(&staging_dir).context("syncing sealed generation")?;
        rename(root, &staging, generation)
            .context("publishing immutable native hook generation")?;
        published = true;
        sync(root).context("syncing generations directory")?;
        validate_existing_generation(root, generation, owner_hash, owner, manifest)
    })();
    if let Err(error) = result {
        if !published {
            return match cleanup_staging(root, &staging, &staging_dir) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(error.context(format!(
                    "unpublished generation cleanup also failed: {cleanup:#}"
                ))),
            };
        }
        return Err(error);
    }
    Ok(())
}

fn cleanup_staging(root: &File, staging: &str, directory: &File) -> Result<()> {
    let mut failures = Vec::new();
    if let Err(error) = directory.set_permissions(std::fs::Permissions::from_mode(0o700)) {
        failures.push(format!("restoring staging permissions: {error}"));
    }
    for name in ["install.manifest", "owner"] {
        if let Err(error) = remove_if_present(directory, name) {
            failures.push(format!("removing staging {name}: {error:#}"));
        }
    }
    if let Err(error) = remove_directory(root, staging) {
        failures.push(format!("removing staging directory: {error:#}"));
    }
    ensure!(failures.is_empty(), "{}", failures.join("; "));
    Ok(())
}

fn remove_if_present(directory: &File, name: &str) -> Result<()> {
    if exists(directory, name)? {
        remove_file(directory, name)?;
    }
    Ok(())
}

fn validate_existing_generation(
    generations: &File,
    generation: &str,
    owner_hash: &str,
    owner: &[u8],
    manifest: &[u8],
) -> Result<()> {
    let directory = open_directory(generations, generation, 0o500)?;
    let existing_owner = read_file(&directory, "owner", OWNER_LIMIT, 0o500)?;
    let existing_manifest = read_file(&directory, "install.manifest", 4096, 0o400)?;
    ensure!(
        existing_owner == owner && existing_manifest == manifest,
        "existing native hook generation conflicts with verified build"
    );
    crate::precommit_snapshot::validate_provision_manifest(&existing_manifest)?;
    ensure!(
        sha256(&existing_owner) == owner_hash,
        "existing generation owner digest mismatch"
    );
    Ok(())
}

fn publish_pending(directory: &File, generation: &str, tree: &str) -> Result<()> {
    if exists(directory, generation)? {
        ensure!(
            read_file(directory, generation, 65, 0o600)? == format!("{tree}\n").as_bytes(),
            "existing pending tree conflicts with this bootstrap"
        );
        return Ok(());
    }
    publish_record(directory, generation, &format!("{tree}\n"), 0o600)
}

fn publish_record(directory: &File, name: &str, contents: &str, mode: u32) -> Result<()> {
    let temporary = format!(".{name}.{}", uuid::Uuid::new_v4());
    create_file(directory, &temporary, contents.as_bytes(), mode)?;
    if let Err(error) = rename(directory, &temporary, name) {
        return match remove_if_present(directory, &temporary) {
            Ok(()) => Err(error).with_context(|| format!("atomically publishing {name}")),
            Err(cleanup) => Err(error).context(format!(
                "atomically publishing {name} failed; temporary cleanup also failed: {cleanup:#}"
            )),
        };
    }
    sync(directory).context("syncing native hook pointer directory")
}

fn validate_current_active(root: &File, generations: &File) -> Result<()> {
    if !exists(root, "active")? {
        return Ok(());
    }
    let active = read_file(root, "active", 65, 0o600)?;
    ensure!(
        active.len() == 65
            && active.last() == Some(&b'\n')
            && active[..64]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)),
        "active native hook pointer is malformed"
    );
    let generation = std::str::from_utf8(&active[..64])?;
    drop(open_directory(generations, generation, 0o500)?);
    Ok(())
}

fn render_manifest(
    verified: &VerifiedBootstrapInstall,
    generation: &str,
    owner_hash: &str,
) -> Vec<u8> {
    format!(
        "{MANIFEST_HEADER}\ngeneration={generation}\nowner-sha256={owner_hash}\nsource-tree={}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n",
        verified.source_tree,
        verified.build_commit,
        verified.source_sha256,
        verified.cargo_sha256,
        verified.rustc_sha256,
        verified.target_sha256,
        verified.build_sha256,
    )
    .into_bytes()
}

fn generation_id(request: &[u8], owner_hash: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"jackin-native-precommit-generation/1\n");
    digest.update(request);
    digest.update(format!("owner-sha256={owner_hash}\n").as_bytes());
    hex::encode(digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
