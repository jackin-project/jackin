// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared file machinery behind the construct-boundary claims.
//!
//! Lock, generation counter, state files, and pending-claim files
//! under the universe coordination authority. Split out of
//! `jackin-runtime-universe` with the claim types (S7 split 83);
//! the observation side (`universe`) keeps using these helpers.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_core::JackinPaths;

static CLAIM_COUNTER: AtomicU64 = AtomicU64::new(0);

// Never unlink this file: replacing its inode would split concurrent locks.
pub fn boundary_lock(authority: &Path) -> std::io::Result<std::fs::File> {
    let file =
        jackin_runtime_coordination::coordination::open_in_namespace(authority, "universe-lock")?;
    file.lock()?;
    Ok(file)
}

pub async fn boundary_work<T: Send + 'static>(
    authority: &Path,
    action: impl FnOnce(&Path) -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let authority = authority.to_owned();
    blocking_work(move || action(&authority)).await
}

pub async fn universe_authority(paths: &JackinPaths) -> std::io::Result<PathBuf> {
    let paths = paths.clone();
    blocking_work(move || jackin_runtime_coordination::coordination::universe_dir(&paths)).await
}

async fn blocking_work<T: Send + 'static>(
    action: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        tracing::dispatcher::with_default(&dispatcher, || {
            let _span = span.enter();
            action()
        })
    })
    .await
    .map_err(std::io::Error::other)?
}

pub fn generation(authority: &Path) -> std::io::Result<Option<String>> {
    state_read(authority, "universe-generation")?
        .map(|value| String::from_utf8(value).map_err(std::io::Error::other))
        .transpose()
}

pub fn advance_generation(authority: &Path) -> std::io::Result<String> {
    let value = claim_token();
    state_write(authority, "universe-generation", value.as_bytes())?;
    Ok(value)
}

pub fn pending_dir(authority: &Path) -> PathBuf {
    authority.join("universe-pending")
}

pub fn pending_path(authority: &Path, token: &str) -> PathBuf {
    pending_dir(authority).join(token)
}

pub fn state_read(directory: &Path, key: &str) -> std::io::Result<Option<Vec<u8>>> {
    let mut file = match jackin_runtime_coordination::coordination::open_state_in_namespace(
        directory, key, false,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok(Some(value))
}

pub fn state_write(directory: &Path, key: &str, value: &[u8]) -> std::io::Result<()> {
    let mut file =
        jackin_runtime_coordination::coordination::open_state_in_namespace(directory, key, true)?;
    // The shared opener validates the owned private regular inode before
    // truncation; an existing symlink/nonregular file cannot redirect writes.
    file.set_len(0)?;
    file.write_all(value)
}

pub fn pending_exists(path: &Path) -> std::io::Result<bool> {
    let Some((directory, key)) = path.parent().zip(path.file_name()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid pending claim",
        ));
    };
    let key = key.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid pending claim key",
        )
    })?;
    match jackin_runtime_coordination::coordination::open_state_in_namespace(directory, key, false)
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn state_remove(directory: &Path, key: &str) -> std::io::Result<()> {
    let _file =
        jackin_runtime_coordination::coordination::open_state_in_namespace(directory, key, false)?;
    let parent =
        jackin_runtime_coordination::coordination::open_directory_in_namespace(directory, false)?;
    #[cfg(unix)]
    {
        nix::unistd::unlinkat(&parent, key, nix::unistd::UnlinkatFlags::NoRemoveDir)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "private state removal requires Unix",
        ))
    }
}

pub fn pending_remove(path: &Path) -> std::io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid pending claim")
    })?;
    let key = path
        .file_name()
        .and_then(|key| key.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid pending claim key",
            )
        })?;
    state_remove(directory, key)
}

pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

pub fn claim_token() -> String {
    let counter = CLAIM_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{counter}", std::process::id(), now_millis())
}
