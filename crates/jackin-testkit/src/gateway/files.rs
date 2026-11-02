// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_core::{
    HostPath, ProcessStatus, SandboxPath, Sha256, TerminalText, sandbox_layout::RUN_START,
};
use jackin_openshell::{
    GatewayError, ProjectUpload, SandboxFile, TransferOp, sanitized_git_config,
};
use tokio_util::sync::CancellationToken;

use super::exec::host_path;
use super::state::Cell;
use crate::exec::HostProgram;
use crate::host::TestRunner;

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

async fn tracked_and_untracked(source: &Path) -> Result<Vec<String>, GatewayError> {
    let output = HostProgram::Git
        .command()
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
        .map(|name| String::from_utf8_lossy(name).into_owned())
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

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            replace_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(super) async fn upload_project(
    root: &Path,
    upload: &ProjectUpload,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let dest = writable(root, &upload.dest)?;
    let source = upload.source.path().as_path().to_path_buf();
    let git_dir = source.join(".git");
    if !tokio::fs::symlink_metadata(&git_dir)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        return Err(GatewayError::UnsupportedGitDirectory {
            path: HostPath::parse(&git_dir).map_err(protocol)?,
        });
    }
    let config = HostPath::parse(&git_dir.join("config")).map_err(protocol)?;
    let sanitized = sanitized_git_config(&TestRunner::new(), &config, cancel).await?;
    let names = tracked_and_untracked(&source).await?;
    if cancel.is_cancelled() {
        return Err(GatewayError::Cancelled {
            op: jackin_openshell::GatewayOp::Upload,
        });
    }
    let git_config = dest.join(".git").join("config");
    tokio::task::spawn_blocking(move || {
        for name in names {
            let from = source.join(&name);
            if from.is_file() {
                replace_file(&from, &dest.join(&name))?;
            }
        }
        copy_tree(&git_dir, &dest.join(".git"))
    })
    .await
    .map_err(protocol)?
    .map_err(protocol)?;
    let temp = git_config.with_file_name(format!(
        ".config.jackin-{:016x}",
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::write(&temp, sanitized.as_bytes())
        .await
        .map_err(protocol)?;
    tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o644))
        .await
        .map_err(protocol)?;
    tokio::fs::rename(&temp, &git_config)
        .await
        .map_err(protocol)
}

pub(super) fn upload_refused() -> GatewayError {
    GatewayError::Transfer {
        op: TransferOp::UploadFiltered,
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
) -> Result<(), GatewayError> {
    let source = host_path(root, remote.as_str());
    let bytes = match source {
        Some(path) => tokio::fs::read(path).await.ok(),
        None => None,
    };
    let Some(mut bytes) = bytes else {
        return Err(GatewayError::Transfer {
            op: TransferOp::Download,
            status: ProcessStatus::Exited(1),
            tail: TerminalText::sanitize(format!("{} is not a file", remote.as_str()).as_bytes()),
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
        Ok(())
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
