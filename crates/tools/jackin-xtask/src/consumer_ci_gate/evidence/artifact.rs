// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::{Artifact, anchored_fs::RepositoryDirectory, managed_target::TargetDirectory};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(in crate::consumer_ci_gate) fn artifact(
    repository: &RepositoryDirectory,
    target: &TargetDirectory,
    relative: &Path,
) -> Result<Artifact> {
    digest(repository, target, relative, None)
}

pub(in crate::consumer_ci_gate) fn capsule_artifact(
    repository: &RepositoryDirectory,
    target: &TargetDirectory,
    relative: &Path,
) -> Result<Artifact> {
    digest(repository, target, relative, Some([0x7f, b'E', b'L', b'F']))
}

fn digest(
    repository: &RepositoryDirectory,
    target: &TargetDirectory,
    relative: &Path,
    expected_header: Option<[u8; 4]>,
) -> Result<Artifact> {
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "artifact path must remain repository-relative without traversal"
    );
    let mut components = relative.components();
    let mut file = if matches!(components.next(), Some(Component::Normal(name)) if name == "target")
    {
        let target_relative = relative
            .strip_prefix("target")
            .context("target artifact has no path")?;
        target.open_artifact(target_relative)?
    } else {
        repository.open_regular(relative, "build artifact")?
    };
    let hash = hash_file(&mut file, expected_header)?;
    Ok(Artifact {
        path: relative.to_string_lossy().into_owned(),
        sha256: hash,
    })
}

fn hash_file(file: &mut File, expected_header: Option<[u8; 4]>) -> Result<String> {
    let before = file
        .metadata()
        .context("checking opened artifact before hashing")?;
    ensure!(before.is_file(), "artifact is not a regular file");
    ensure!(before.nlink() == 1, "artifact file has multiple hard links");
    ensure!(
        before.len() <= MAX_ARTIFACT_BYTES,
        "artifact exceeds 2 GiB bound"
    );
    let mut hash = Sha256::new();
    let mut total = 0u64;
    if let Some(expected_header) = expected_header {
        let mut header = [0; 4];
        file.read_exact(&mut header)
            .context("reading production Capsule ELF header")?;
        ensure!(
            header == expected_header,
            "exported production Capsule is not ELF"
        );
        hash.update(header);
        total += header.len() as u64;
    }
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("artifact byte count overflow")?;
        ensure!(
            total <= MAX_ARTIFACT_BYTES,
            "artifact grew beyond 2 GiB bound"
        );
        hash.update(&buffer[..count]);
    }
    let after = file
        .metadata()
        .context("checking opened artifact after hashing")?;
    ensure!(
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.modified()? == after.modified()?
            && total == after.len(),
        "artifact changed while being hashed"
    );
    Ok(hex::encode(hash.finalize()))
}
