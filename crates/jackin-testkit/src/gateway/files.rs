// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_core::{
    HostPath, ProcessStatus, ProjectRoot, SandboxPath, Sha256, TerminalText,
    sandbox_layout::RUN_START,
};
use jackin_openshell::{GatewayError, GatewayOp, SandboxFile, TransferOp};
use tokio_util::sync::CancellationToken;

use super::exec::host_path;
use super::state::Cell;
use crate::exec::HostProgram;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn protocol(message: impl ToString) -> GatewayError {
    GatewayError::Protocol {
        message: message.to_string(),
    }
}

fn writable(root: &Path, path: &SandboxPath) -> Result<PathBuf, GatewayError> {
    host_path(root, path.as_str()).ok_or_else(|| GatewayError::PermissionDenied {
        what: format!("{} is outside /sandbox", path.as_str()),
    })
}

pub(super) async fn read_files(
    root: &Path,
    installed: &BTreeMap<SandboxPath, Vec<u8>>,
    paths: &[SandboxPath],
) -> Vec<Option<Vec<u8>>> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        if let Some(bytes) = installed.get(path) {
            files.push(Some(bytes.clone()));
            continue;
        }
        let found = match host_path(root, path.as_str()) {
            Some(host) => match tokio::fs::metadata(&host).await {
                Ok(meta) if meta.is_file() => tokio::fs::read(&host).await.ok(),
                _ => None,
            },
            None => None,
        };
        files.push(found);
    }
    files
}

pub(super) async fn write_file(cell: &Arc<Cell>, file: &SandboxFile) -> Result<(), GatewayError> {
    let root = cell.root();
    let target = writable(&root, &file.path)?;
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return Err(protocol("the target has no directory"));
    };
    let temp = dir.join(format!(
        ".{}.jackin-{:016x}",
        name.to_string_lossy(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::create_dir_all(dir).await.map_err(protocol)?;
    tokio::fs::write(&temp, file.content.expose())
        .await
        .map_err(protocol)?;
    tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(file.mode.0))
        .await
        .map_err(protocol)?;
    tokio::fs::rename(&temp, &target).await.map_err(protocol)?;
    if file.path == RUN_START {
        let mut state = cell.state();
        if let Some(supervisor) = &mut state.supervisor {
            supervisor.request_written(&root);
        }
    }
    Ok(())
}

async fn tracked_and_untracked(
    source: &Path,
    home: Option<&Path>,
) -> Result<Vec<Vec<u8>>, GatewayError> {
    let mut command = HostProgram::Git.command();
    if let Some(home) = home {
        command.env("HOME", home);
    }
    let output = command
        .args(["ls-files", "-z", "-co", "--exclude-standard"])
        .current_dir(source)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(protocol)?;
    if !output.status.success() {
        return Err(GatewayError::Transfer {
            op: TransferOp::UploadFiltered,
            status: ProcessStatus::Exited(output.status.code().unwrap_or(-1)),
            tail: TerminalText::sanitize(&output.stderr),
        });
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(<[u8]>::to_vec)
        .collect())
}

fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let (Some(parent), Some(name)) = (to.parent(), to.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.jackin-{:016x}",
        name.to_string_lossy(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::copy(from, &temp)?;
    std::fs::rename(&temp, to)
}

fn replace_symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    let (Some(parent), Some(name)) = (to.parent(), to.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.jackin-{:016x}",
        name.to_string_lossy(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::os::unix::fs::symlink(std::fs::read_link(from)?, &temp)?;
    std::fs::rename(&temp, to)
}

fn copy_entry(from: &Path, to: &Path) -> std::io::Result<()> {
    let kind = std::fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        replace_symlink(from, to)
    } else if kind.is_dir() {
        copy_tree(from, to)
    } else if kind.is_file() {
        replace_file(from, to)
    } else {
        Ok(())
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_entry(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

pub(super) async fn upload_tree(
    root: &Path,
    home: Option<&Path>,
    source: &ProjectRoot,
    dest: &SandboxPath,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    if cancel.is_cancelled() {
        return Err(GatewayError::Cancelled {
            op: GatewayOp::UploadTree,
        });
    }
    let dest = writable(root, dest)?;
    let source = source.path().as_path().to_path_buf();
    let names = tracked_and_untracked(&source, home).await?;
    tokio::task::spawn_blocking(move || {
        for name in names {
            let relative = Path::new(OsStr::from_bytes(&name));
            let from = source.join(relative);
            if std::fs::symlink_metadata(&from).is_ok() {
                copy_entry(&from, &dest.join(relative))?;
            }
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(protocol)?
    .map_err(protocol)
}

pub(super) async fn upload_file(
    root: &Path,
    source: &HostPath,
    dest: &SandboxPath,
    corrupt: bool,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    if cancel.is_cancelled() {
        return Err(GatewayError::Cancelled {
            op: GatewayOp::UploadFile,
        });
    }
    let target = writable(root, dest)?;
    let sent = tokio::fs::read(source.as_path()).await.map_err(|error| {
        digest_failure(
            TransferOp::UploadFile,
            &format!("the file cannot be read: {error}"),
        )
    })?;
    let from = source.as_path().to_path_buf();
    let to = target.clone();
    tokio::task::spawn_blocking(move || replace_file(&from, &to))
        .await
        .map_err(protocol)?
        .map_err(protocol)?;
    if corrupt {
        let mut landed = tokio::fs::read(&target).await.map_err(protocol)?;
        match landed.first_mut() {
            Some(first) => *first ^= 0xff,
            None => landed.push(0xff),
        }
        tokio::fs::write(&target, landed).await.map_err(protocol)?;
    }
    let expected = Sha256::of(&sent);
    let actual = Sha256::of(&tokio::fs::read(&target).await.map_err(protocol)?);
    if actual == expected {
        Ok(())
    } else {
        Err(digest_failure(
            TransferOp::Digest,
            &format!(
                "sha256 mismatch: the sandbox has {}, the upload has {}",
                actual.as_str(),
                expected.as_str()
            ),
        ))
    }
}

pub(super) async fn edit_host(edit: &super::HostEdit) -> Result<(), GatewayError> {
    match edit {
        super::HostEdit::Write { path, bytes } => tokio::fs::write(path.as_path(), bytes)
            .await
            .map_err(protocol),
        super::HostEdit::Chmod { path, mode } => {
            tokio::fs::set_permissions(path.as_path(), std::fs::Permissions::from_mode(mode.0))
                .await
                .map_err(protocol)
        }
    }
}

pub(super) fn upload_refused(op: TransferOp) -> GatewayError {
    GatewayError::Transfer {
        op,
        status: ProcessStatus::Exited(1),
        tail: TerminalText::sanitize(b"sandbox not found"),
    }
}

fn digest_failure(op: TransferOp, note: &str) -> GatewayError {
    GatewayError::Transfer {
        op,
        status: ProcessStatus::Exited(0),
        tail: TerminalText::sanitize(note.as_bytes()),
    }
}

pub(super) async fn download_file(
    root: &Path,
    remote: &SandboxPath,
    local: &HostPath,
    corrupt: bool,
) -> Result<Sha256, GatewayError> {
    let Some(source) = host_path(root, remote.as_str()) else {
        return Err(GatewayError::Transfer {
            op: TransferOp::Download,
            status: ProcessStatus::Exited(1),
            tail: TerminalText::sanitize(
                format!("{} is outside /sandbox", remote.as_str()).as_bytes(),
            ),
        });
    };
    let Ok(mut bytes) = tokio::fs::read(source).await else {
        return Err(GatewayError::Transfer {
            op: TransferOp::Size,
            status: ProcessStatus::Exited(1),
            tail: TerminalText::sanitize(
                format!("stat: cannot stat '{}': No such file", remote.as_str()).as_bytes(),
            ),
        });
    };
    let expected = Sha256::of(&bytes);
    if corrupt {
        match bytes.first_mut() {
            Some(first) => *first ^= 0xff,
            None => bytes.push(0xff),
        }
    }
    if let Some(parent) = local.as_path().parent() {
        tokio::fs::create_dir_all(parent).await.map_err(protocol)?;
    }
    tokio::fs::write(local.as_path(), &bytes)
        .await
        .map_err(protocol)?;
    let written = tokio::fs::read(local.as_path()).await.map_err(protocol)?;
    let actual = Sha256::of(&written);
    if actual == expected {
        Ok(expected)
    } else {
        Err(digest_failure(
            TransferOp::Digest,
            &format!(
                "sha256 mismatch: the sandbox has {}, the download has {}",
                expected.as_str(),
                actual.as_str()
            ),
        ))
    }
}
