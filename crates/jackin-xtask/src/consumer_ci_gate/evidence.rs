// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::Artifact;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub(super) fn artifact(root: &Path, path: &Path) -> Result<Artifact> {
    artifact_with_header(root, path, None)
}

pub(super) fn capsule_artifact(root: &Path, path: &Path) -> Result<Artifact> {
    artifact_with_header(root, path, Some([0x7f, b'E', b'L', b'F']))
}

#[expect(
    clippy::disallowed_methods,
    reason = "synchronous CLI gate validates and hashes an opened artifact outside render/runtime threads"
)]
fn artifact_with_header(
    root: &Path,
    path: &Path,
    expected_header: Option<[u8; 4]>,
) -> Result<Artifact> {
    use rustix::fs::{FileType, Mode, OFlags, fstat, open, openat};

    let relative = path
        .strip_prefix(root)
        .context("artifact escapes source root")?;
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "artifact path must stay below source root without traversal"
    );

    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = fs::File::from(
        open(root, directory_flags, Mode::empty()).context("opening anchored source root")?,
    );
    let parent = relative.parent().context("artifact has no parent")?;
    for component in parent.components() {
        let Component::Normal(name) = component else {
            unreachable!("artifact components were validated above");
        };
        directory = fs::File::from(
            openat(&directory, Path::new(name), directory_flags, Mode::empty())
                .with_context(|| format!("opening artifact directory {name:?} without symlinks"))?,
        );
    }
    let name = relative.file_name().context("artifact has no filename")?;
    let descriptor = openat(
        &directory,
        Path::new(name),
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .context("opening artifact without symlink traversal")?;
    let mut file = fs::File::from(descriptor);
    let metadata = fstat(&file).context("checking opened artifact type")?;
    ensure!(
        FileType::from_raw_mode(metadata.st_mode).is_file(),
        "artifact must be a regular file: {}",
        path.display()
    );

    let mut hash = Sha256::new();
    if let Some(expected_header) = expected_header {
        let mut header = [0; 4];
        file.read_exact(&mut header)
            .context("reading production Capsule ELF header")?;
        ensure!(
            header == expected_header,
            "exported production Capsule is not ELF"
        );
        hash.update(header);
    }
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(Artifact {
        path: relative.to_string_lossy().into_owned(),
        sha256: hex::encode(hash.finalize()),
    })
}

pub(super) fn fresh_destination(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        relative.starts_with("target/consumer-ci")
            && relative
                .extension()
                .is_some_and(|extension| extension == "json"),
        "consumer reports belong under ignored target/consumer-ci and must be JSON"
    );
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "report path must be repository-relative without traversal"
    );
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(!metadata.file_type().is_symlink(), "symlink in report path"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(!current.exists(), "preexisting acceptance report rejected");
    Ok(current)
}

pub(super) fn write_fresh(root: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    use rustix::fs::{Mode, OFlags, mkdirat, open, openat};
    use std::io::Write;
    let relative = path
        .strip_prefix(root)
        .context("report escapes source root")?;
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "invalid report path"
    );
    let name = relative.file_name().context("report has no filename")?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory =
        open(root, flags, Mode::empty()).context("opening anchored source directory")?;
    for part in relative
        .parent()
        .context("report has no parent")?
        .components()
    {
        let child = part.as_os_str();
        directory = match openat(&directory, child, flags, Mode::empty()) {
            Ok(opened) => opened,
            Err(error) if error == rustix::io::Errno::NOENT => {
                match mkdirat(&directory, child, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                    Ok(()) => {}
                    Err(error) if error == rustix::io::Errno::EXIST => {}
                    Err(error) => return Err(error.into()),
                }
                openat(&directory, child, flags, Mode::empty())
                    .context("opening new report directory without symlink traversal")?
            }
            Err(error) => {
                return Err(error).context("opening report directory without symlink traversal");
            }
        };
    }
    let descriptor = openat(
        &directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .context("creating fresh anchored report")?;
    let mut file = fs::File::from(descriptor);
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
