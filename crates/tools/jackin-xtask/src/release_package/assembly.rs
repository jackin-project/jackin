// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::handoff::{TaskInput, TaskRequest};
use anyhow::{Result, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
};

pub(super) fn assemble(output: &Path, handoff: &TaskRequest) -> Result<()> {
    for input in &handoff.inputs {
        copy_input(input, output)?;
    }
    for input in &handoff.inputs {
        ensure!(
            crate::release_sbom::archive_sha256(Path::new(&input.path))? == input.sha256,
            "admitted input changed during assembly"
        );
    }
    Ok(())
}

fn copy_input(input: &TaskInput, output: &Path) -> Result<()> {
    let source = Path::new(&input.path);
    reject_links(source)?;
    let fd = open(
        source,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let mut file = fs::File::from(fd);
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() == input.size,
        "admitted input is not the exact regular file"
    );
    let destination = output.join(&input.name);
    let fd = open(
        &destination,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )?;
    let mut copied = fs::File::from(fd);
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= input.size)
            .ok_or_else(|| anyhow::anyhow!("admitted input grew during copy"))?;
        hash.update(&buffer[..read]);
        copied.write_all(&buffer[..read])?;
    }
    copied.sync_all()?;
    let after = file.metadata()?;
    let named = fs::symlink_metadata(source)?;
    ensure!(
        total == input.size && hex::encode(hash.finalize()) == input.sha256,
        "admitted input length/digest changed"
    );
    ensure!(
        named.is_file()
            && before.dev() == named.dev()
            && before.ino() == named.ino()
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec(),
        "admitted input identity changed"
    );
    ensure!(
        crate::release_sbom::archive_sha256(&destination)? == input.sha256,
        "copied input digest differs"
    );
    Ok(())
}

fn reject_links(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "admitted input path must be absolute");
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "admitted path contains a symlink"
        );
    }
    Ok(())
}
