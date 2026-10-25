// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Bounded no-follow reads and filesystem identity validation.

use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use sha2::{Digest, Sha256};

use super::{PATH_LIMIT, TARGET_LIMIT};

pub(super) fn read_path_record(root: &Path, name: &str) -> Result<PathBuf> {
    let bytes = read_evidence(root, name, PATH_LIMIT)?;
    ensure!(
        bytes.last() == Some(&b'\n')
            && bytes[..bytes.len() - 1]
                .iter()
                .all(|byte| *byte != b'\n' && *byte != b'\r' && *byte != 0),
        "{name} is not a single canonical path record"
    );
    let path = PathBuf::from(
        std::str::from_utf8(&bytes[..bytes.len() - 1]).context("decoding path record")?,
    );
    ensure!(path.is_absolute(), "{name} is not absolute");
    ensure!(
        fs::canonicalize(&path).context("resolving evidence path")? == path,
        "{name} is not physical"
    );
    Ok(path)
}

pub(super) fn read_evidence(root: &Path, name: &str, limit: usize) -> Result<Vec<u8>> {
    let path = root.join(name);
    let before =
        fs::symlink_metadata(&path).with_context(|| format!("checking evidence {name}"))?;
    ensure!(
        before.is_file()
            && !before.file_type().is_symlink()
            && before.uid() == uid()
            && before.nlink() == 1
            && before.permissions().mode() & 0o777 == 0o600,
        "evidence {name} has unsafe type, owner, mode, or link count"
    );
    let mut file = open_nofollow(&path).with_context(|| format!("opening evidence {name}"))?;
    let opened = file.metadata()?;
    ensure!(
        opened.dev() == before.dev() && opened.ino() == before.ino() && opened.nlink() == 1,
        "evidence {name} changed while opening"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading evidence {name}"))?;
    ensure!(
        bytes.len() <= limit,
        "evidence {name} exceeds its bounded size"
    );
    let after = fs::symlink_metadata(&path)?;
    let after_fd = file.metadata()?;
    ensure!(
        after.dev() == opened.dev()
            && after.ino() == opened.ino()
            && after.len() == opened.len()
            && after.uid() == uid()
            && after.nlink() == 1
            && after.permissions().mode() & 0o777 == 0o600
            && after_fd.dev() == opened.dev()
            && after_fd.ino() == opened.ino()
            && after_fd.len() == opened.len(),
        "evidence {name} changed while reading"
    );
    Ok(bytes)
}

pub(super) fn verify_evidence_names(root: &Path) -> Result<()> {
    let mut names = crate::fs_util::read_dir_sorted(root)
        .context("listing bootstrap evidence")?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    names.sort();
    let expected = [
        "build.argv",
        "build.jsonl",
        "build.stderr",
        "cargo.path",
        "index.snapshot",
        "request",
        "rustc.path",
        "source.inventory",
        "target.path",
    ];
    ensure!(
        names.len() == expected.len(),
        "bootstrap evidence has missing or extra files"
    );
    for (actual, expected) in names.iter().zip(expected) {
        ensure!(
            actual.to_str() == Some(expected),
            "bootstrap evidence has an unknown filename"
        );
    }
    Ok(())
}

pub(super) fn verify_bootstrap_names(root: &Path) -> Result<()> {
    let mut names = crate::fs_util::read_dir_sorted(root)
        .context("listing native hook bootstrap root")?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    names.sort();
    ensure!(
        names.len() == 2,
        "bootstrap root has missing or extra entries"
    );
    ensure!(
        names[0].to_str() == Some("evidence") && names[1].to_str() == Some("source"),
        "bootstrap root has unknown entries"
    );
    Ok(())
}

pub(super) fn physical_private_directory(path: &Path, role: &str) -> Result<PathBuf> {
    let path = physical_directory(path, role)?;
    let metadata = fs::metadata(&path)?;
    ensure!(
        metadata.uid() == uid() && metadata.permissions().mode() & 0o777 == 0o700,
        "{role} must be owned mode 0700"
    );
    Ok(path)
}

pub(in crate::precommit_provision) fn physical_directory(
    path: &Path,
    role: &str,
) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path).with_context(|| format!("resolving {role}"))?;
    ensure!(canonical == path, "{role} path is not physical");
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "{role} is not a physical directory"
    );
    Ok(canonical)
}

pub(in crate::precommit_provision) fn read_executable(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid()
            && metadata.nlink() == 1
            && metadata.len() <= TARGET_LIMIT as u64
            && metadata.permissions().mode() & 0o111 != 0,
        "target executable has unsafe custody"
    );
    let mut file = open_nofollow(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.dev() == metadata.dev() && opened.ino() == metadata.ino() && opened.nlink() == 1,
        "executable changed while opening"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(TARGET_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= TARGET_LIMIT,
        "target executable exceeds its size bound"
    );
    let after = fs::symlink_metadata(path)?;
    let after_fd = file.metadata()?;
    ensure!(
        after.dev() == opened.dev()
            && after.ino() == opened.ino()
            && after.len() == opened.len()
            && after.uid() == uid()
            && after.nlink() == 1
            && after.permissions().mode() & 0o111 != 0
            && after_fd.dev() == opened.dev()
            && after_fd.ino() == opened.ino()
            && after_fd.len() == opened.len(),
        "executable changed while reading"
    );
    Ok(bytes)
}

pub(super) fn hash_executable(path: &Path) -> Result<String> {
    Ok(sha256(&read_executable(path)?))
}

pub(super) fn read_physical_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.nlink() == 1
            && metadata.len() <= limit as u64,
        "reviewed source file has unsafe custody"
    );
    let mut file = open_nofollow(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.dev() == metadata.dev() && opened.ino() == metadata.ino(),
        "reviewed source file changed while opening"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "reviewed source file exceeds its bound"
    );
    let after = fs::symlink_metadata(path)?;
    ensure!(
        after.dev() == opened.dev() && after.ino() == opened.ino() && after.len() == opened.len(),
        "reviewed source file changed while reading"
    );
    Ok(bytes)
}

pub(super) fn current_executable() -> Result<PathBuf> {
    Ok(fs::canonicalize(std::env::current_exe()?)?)
}

fn open_nofollow(path: &Path) -> Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?;
    Ok(File::from(descriptor))
}

pub(super) fn hash_if_regular(path: &Path, limit: usize) -> Result<Option<String>> {
    let before = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => metadata,
    };
    ensure!(
        before.is_file() && !before.file_type().is_symlink() && before.len() <= limit as u64,
        "index identity input is not a bounded regular file"
    );
    let mut file = open_nofollow(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.dev() == before.dev() && opened.ino() == before.ino(),
        "index identity input changed while opening"
    );
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 32 * 1024].into_boxed_slice();
    let mut total = 0usize;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count).context("index size overflow")?;
        ensure!(total <= limit, "index identity input exceeds its bound");
        hasher.update(&buffer[..count]);
    }
    let after_fd = file.metadata()?;
    let after_path = fs::symlink_metadata(path)?;
    ensure!(
        after_fd.dev() == opened.dev()
            && after_fd.ino() == opened.ino()
            && after_fd.len() == opened.len()
            && after_path.dev() == opened.dev()
            && after_path.ino() == opened.ino()
            && after_path.len() == opened.len(),
        "index identity input changed while hashing"
    );
    Ok(Some(hex::encode(hasher.finalize())))
}

pub(super) fn validate_oid(value: &str) -> Result<()> {
    ensure!(
        [40, 64].contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Git object ID is not canonical"
    );
    Ok(())
}

pub(super) fn validate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "digest is not canonical SHA-256"
    );
    Ok(())
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn uid() -> u32 {
    nix::unistd::geteuid().as_raw()
}
