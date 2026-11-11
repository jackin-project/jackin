// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{
    DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink,
};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use rustix::fs::{AtFlags, FileType, Mode, OFlags, open, openat, readlinkat, statat};
use rustix::io::Errno;

use crate::boundary::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, LinkProblem,
    RegularFile, RegularFileProblem,
};
use crate::paths::{HostPath, RepoPath};

#[derive(Clone, Copy, Debug, Default)]
pub struct StdFileSystem {}

static TEMP_WRITES: AtomicU64 = AtomicU64::new(0);

async fn blocking<T, F>(path: &HostPath, work: F) -> Result<T, FsError>
where
    T: Send + 'static,
    F: FnOnce(&Path) -> io::Result<T> + Send + 'static,
{
    let owned = path.clone();
    match tokio::task::spawn_blocking(move || work(owned.as_path())).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(source)) => Err(FsError::io(path, &source)),
        Err(join) => Err(FsError::Io {
            path: path.clone(),
            kind: ErrorKind::Other,
            message: join.to_string(),
        }),
    }
}

fn missing_is_none<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(source),
    }
}

fn kind_of(file_type: fs::FileType) -> FileKind {
    if file_type.is_dir() {
        FileKind::Dir
    } else if file_type.is_symlink() {
        FileKind::Symlink
    } else if file_type.is_socket() {
        FileKind::Socket
    } else if file_type.is_file() {
        FileKind::File
    } else {
        FileKind::Other
    }
}

fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    missing_is_none(fs::read(path))
}

fn stat_kind(path: &Path) -> io::Result<Option<FileKind>> {
    missing_is_none(fs::symlink_metadata(path)).map(|meta| meta.map(|m| kind_of(m.file_type())))
}

fn stat_following_kind(path: &Path) -> io::Result<Option<FileKind>> {
    missing_is_none(fs::metadata(path)).map(|meta| meta.map(|m| kind_of(m.file_type())))
}

fn canonical_target(path: &Path) -> io::Result<Option<HostPath>> {
    missing_is_none(fs::canonicalize(path))?
        .map(|real| HostPath::parse(&real).map_err(io::Error::other))
        .transpose()
}

fn file_size(path: &Path) -> io::Result<Option<u64>> {
    missing_is_none(fs::metadata(path)).map(|meta| meta.map(|m| m.len()))
}

fn identity_of(path: &Path) -> io::Result<Option<FileIdentity>> {
    missing_is_none(fs::metadata(path)).map(|meta| {
        meta.map(|m| FileIdentity {
            dev: m.dev(),
            ino: m.ino(),
        })
    })
}

fn sorted_entries(dir: &Path) -> io::Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            kind: kind_of(entry.file_type()?),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn create_all(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

fn create_private_all(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

fn rename_path(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    missing_is_none(fs::remove_file(path)).map(|_removed| ())
}

fn remove_tree_if_present(path: &Path) -> io::Result<()> {
    missing_is_none(fs::remove_dir_all(path)).map(|_removed| ())
}

fn file_mode(path: &Path) -> io::Result<Option<FileMode>> {
    missing_is_none(fs::symlink_metadata(path))
        .map(|meta| meta.map(|m| FileMode(m.permissions().mode() & 0o7777)))
}

fn place_symlink(target: &str, path: &Path) -> io::Result<()> {
    symlink(target, path)
}

#[expect(
    clippy::disallowed_methods,
    reason = "runs on the blocking pool through StdFileSystem::write_new"
)]
fn create_new_file(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[expect(
    clippy::disallowed_methods,
    reason = "runs on the blocking pool through StdFileSystem::sync_dir"
)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[expect(
    clippy::disallowed_methods,
    reason = "runs on the blocking pool through StdFileSystem::lock"
)]
fn lock_file(path: &Path) -> io::Result<Result<File, ()>> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Ok(file)),
        Err(TryLockError::WouldBlock) => Ok(Err(())),
        Err(TryLockError::Error(source)) => Err(source),
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "runs on the blocking pool through StdFileSystem::read_at"
)]
fn read_range(path: &Path, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(len).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn classify(
    dir: &OwnedFd,
    name: &str,
    failure: Errno,
    last: bool,
) -> io::Result<RegularFileProblem> {
    let stat = match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(RegularFileProblem::Missing),
        Err(other) => return Err(other.into()),
    };
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::Symlink => Ok(RegularFileProblem::Symlink),
        FileType::Directory => Err(failure.into()),
        _ if !last => Ok(RegularFileProblem::Missing),
        FileType::RegularFile => Err(failure.into()),
        _ => Ok(RegularFileProblem::NotARegularFile),
    }
}

fn open_below(
    dir: &OwnedFd,
    name: &str,
    flags: OFlags,
    last: bool,
) -> io::Result<Result<OwnedFd, RegularFileProblem>> {
    match openat(
        dir,
        name,
        flags | OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => Ok(Ok(fd)),
        Err(Errno::NOENT) => Ok(Err(RegularFileProblem::Missing)),
        Err(failure) => classify(dir, name, failure, last).map(Err),
    }
}

fn open_parent<'a>(
    root: &Path,
    file: &'a RepoPath,
) -> io::Result<Result<(OwnedFd, &'a str), RegularFileProblem>> {
    let mut dir = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut names = file.as_str().split('/').peekable();
    while let Some(name) = names.next() {
        if names.peek().is_none() {
            return Ok(Ok((dir, name)));
        }
        match open_below(&dir, name, OFlags::DIRECTORY, false)? {
            Ok(next) => dir = next,
            Err(problem) => return Ok(Err(problem)),
        }
    }
    Ok(Err(RegularFileProblem::Missing))
}

fn read_below(
    root: &Path,
    file: &RepoPath,
    max_bytes: usize,
) -> io::Result<Result<RegularFile, RegularFileProblem>> {
    let (dir, name) = match open_parent(root, file)? {
        Ok(parent) => parent,
        Err(problem) => return Ok(Err(problem)),
    };
    let leaf = match open_below(&dir, name, OFlags::NONBLOCK, true)? {
        Ok(leaf) => leaf,
        Err(problem) => return Ok(Err(problem)),
    };
    let mut opened = File::from(leaf);
    let meta = opened.metadata()?;
    if !meta.is_file() {
        return Ok(Err(RegularFileProblem::NotARegularFile));
    }
    let mut bytes = Vec::new();
    let limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    (&mut opened).take(limit).read_to_end(&mut bytes)?;
    Ok(Ok(RegularFile {
        bytes,
        mode: FileMode(meta.permissions().mode() & 0o7777),
    }))
}

fn read_link_below(root: &Path, file: &RepoPath) -> io::Result<Result<String, LinkProblem>> {
    let (dir, name) = match open_parent(root, file)? {
        Ok(parent) => parent,
        Err(RegularFileProblem::Symlink) => return Ok(Err(LinkProblem::Symlink)),
        Err(RegularFileProblem::Missing | RegularFileProblem::NotARegularFile) => {
            return Ok(Err(LinkProblem::Missing));
        }
    };
    match readlinkat(&dir, name, Vec::new()) {
        Ok(target) => Ok(target.into_string().map_err(|_| LinkProblem::NonUtf8)),
        Err(Errno::NOENT) => Ok(Err(LinkProblem::Missing)),
        Err(Errno::INVAL) => Ok(Err(LinkProblem::NotALink)),
        Err(other) => Err(other.into()),
    }
}

#[async_trait]
impl FileSystem for StdFileSystem {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        blocking(path, read_file).await
    }

    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        blocking(path, stat_kind).await
    }

    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        blocking(path, stat_following_kind).await
    }

    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        blocking(path, canonical_target).await
    }

    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        blocking(path, file_size).await
    }

    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        blocking(path, identity_of).await
    }

    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        blocking(dir, sorted_entries).await
    }

    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        blocking(dir, create_all).await
    }

    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        blocking(dir, create_private_all).await
    }

    async fn write_new(
        &self,
        path: &HostPath,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), FsError> {
        let bytes = bytes.to_vec();
        blocking(path, move |p| create_new_file(p, &bytes, mode.0))
            .await
            .map_err(|error| match error {
                FsError::Io {
                    path,
                    kind: ErrorKind::AlreadyExists,
                    ..
                } => FsError::AlreadyExists(path),
                other => other,
            })
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        let target = to.clone();
        blocking(from, move |p| rename_path(p, target.as_path())).await
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        blocking(path, remove_if_present).await
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        blocking(path, remove_tree_if_present).await
    }

    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError> {
        blocking(path, file_mode).await
    }

    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        blocking(dir, sync_directory).await
    }

    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        match blocking(path, lock_file).await? {
            Ok(file) => Ok(FileLock::new(path.clone(), file)),
            Err(()) => Err(FsError::LockHeld(path.clone())),
        }
    }

    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let file = file.clone();
        blocking(root, move |p| read_below(p, &file, max_bytes)).await
    }

    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        let file = file.clone();
        blocking(root, move |p| read_link_below(p, &file)).await
    }

    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        let target = target.to_owned();
        blocking(path, move |p| place_symlink(&target, p))
            .await
            .map_err(|error| match error {
                FsError::Io {
                    path,
                    kind: ErrorKind::AlreadyExists,
                    ..
                } => FsError::AlreadyExists(path),
                other => other,
            })
    }

    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        blocking(path, move |p| read_range(p, offset, len)).await
    }
}

pub const CREDENTIAL_FILE_MAX_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CredentialFileProblem {
    Missing,
    NotARegularFile,
    TooLarge,
}

pub async fn read_credential_file(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<Result<Vec<u8>, CredentialFileProblem>, FsError> {
    let Some(target) = fs.canonicalize(path).await? else {
        return Ok(Err(CredentialFileProblem::Missing));
    };
    let Some(dir) = target.parent() else {
        return Ok(Err(CredentialFileProblem::NotARegularFile));
    };
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| RepoPath::parse(name).ok())
        .ok_or_else(|| FsError::Io {
            path: target.clone(),
            kind: ErrorKind::InvalidInput,
            message: "credential file name is not usable".to_owned(),
        })?;
    match fs
        .read_regular(&dir, &name, CREDENTIAL_FILE_MAX_BYTES)
        .await
    {
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        })
        | Ok(Err(RegularFileProblem::Missing)) => Ok(Err(CredentialFileProblem::Missing)),
        Ok(Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile)) => {
            Ok(Err(CredentialFileProblem::NotARegularFile))
        }
        Ok(Ok(file)) if file.bytes.len() > CREDENTIAL_FILE_MAX_BYTES => {
            Ok(Err(CredentialFileProblem::TooLarge))
        }
        Ok(Ok(file)) => Ok(Ok(file.bytes)),
        Err(other) => Err(other),
    }
}

pub fn lock_path(target: &HostPath) -> Result<HostPath, FsError> {
    let Some(name) = target.file_name() else {
        return Err(FsError::Io {
            path: target.clone(),
            kind: ErrorKind::InvalidInput,
            message: "path has no file name".to_owned(),
        });
    };
    let mut lock_name = name.to_os_string();
    lock_name.push(".lock");
    Ok(target.with_file_name(&lock_name))
}

pub async fn write_atomic_locked(
    fs: &dyn FileSystem,
    path: &HostPath,
    bytes: &[u8],
    mode: FileMode,
    held: &FileLock,
) -> Result<(), FsError> {
    if *held.path() != lock_path(path)? {
        return Err(FsError::WrongLock {
            lock: held.path().clone(),
            target: path.clone(),
        });
    }
    sweep_stale_temps(fs, path).await;
    write_atomic(fs, path, bytes, mode).await
}

async fn sweep_stale_temps(fs: &dyn FileSystem, path: &HostPath) {
    let (Some(dir), Some(name)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return;
    };
    let Ok(entries) = fs.list(&dir).await else {
        return;
    };
    for entry in entries {
        if entry.kind == FileKind::File && is_temp_of(name, &entry.name) {
            let _swept = fs.remove_file(&dir.join_name(&entry.name)).await;
        }
    }
}

fn is_temp_of(target: &str, candidate: &str) -> bool {
    let counters = candidate
        .strip_prefix('.')
        .and_then(|rest| rest.strip_prefix(target))
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(".tmp"));
    counters
        .and_then(|counters| counters.split_once('-'))
        .is_some_and(|(pid, count)| all_digits(pid) && all_digits(count))
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub async fn write_atomic(
    fs: &dyn FileSystem,
    path: &HostPath,
    bytes: &[u8],
    mode: FileMode,
) -> Result<(), FsError> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(FsError::Io {
            path: path.clone(),
            kind: ErrorKind::InvalidInput,
            message: "path has no parent directory or file name".to_owned(),
        });
    };
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        TEMP_WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    if fs.stat(path).await? == Some(FileKind::Symlink) {
        return Err(FsError::SymlinkTarget(path.clone()));
    }
    let temp = path.with_file_name(&temp_name);
    fs.remove_file(&temp).await?;
    let written = match fs.write_new(&temp, bytes, mode).await {
        Ok(()) => fs.rename(&temp, path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = written {
        let _cleanup = fs.remove_file(&temp).await;
        return Err(error);
    }
    fs.sync_dir(&dir).await.map_err(|error| FsError::Unsynced {
        path: path.clone(),
        message: error.to_string(),
    })
}
