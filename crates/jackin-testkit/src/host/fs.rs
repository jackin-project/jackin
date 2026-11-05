// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, RegularFile, RegularFileProblem, RepoPath,
};

use crate::sync::locked;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FsOp {
    Read,
    Stat,
    StatFollowing,
    FileLen,
    Identity,
    List,
    CreateDirAll,
    CreatePrivateDirAll,
    WriteNew,
    Rename,
    RemoveFile,
    RemoveDirAll,
    Mode,
    SyncDir,
    Lock,
    ReadAt,
}

#[derive(Debug, Default)]
pub struct MemFs {
    state: Arc<Mutex<FsState>>,
}

#[derive(Debug, Default)]
struct FsState {
    files: BTreeMap<PathBuf, MemFile>,
    dirs: BTreeMap<PathBuf, FileMode>,
    locked: BTreeSet<PathBuf>,
    inodes: BTreeMap<PathBuf, u64>,
    next_inode: u64,
    calls: HashMap<FsOp, u32>,
    faults: Vec<FsFault>,
}

#[derive(Debug)]
struct MemFile {
    bytes: Vec<u8>,
    mode: FileMode,
}

#[derive(Debug)]
struct FsFault {
    op: FsOp,
    nth: u32,
    error: Option<FsError>,
}

#[derive(Debug)]
struct Held {
    state: Arc<Mutex<FsState>>,
    path: PathBuf,
}

impl Drop for Held {
    fn drop(&mut self) {
        locked(&self.state).locked.remove(&self.path);
    }
}

fn io_error(path: &HostPath, kind: ErrorKind, message: &str) -> FsError {
    FsError::Io {
        path: path.clone(),
        kind,
        message: message.to_owned(),
    }
}

impl FsState {
    fn is_dir(&self, path: &Path) -> bool {
        path == Path::new("/") || self.dirs.contains_key(path)
    }

    fn occupied(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.is_dir(path)
    }

    fn parent_dir(&self, path: &HostPath) -> Result<(), FsError> {
        match path.as_path().parent() {
            Some(parent) if !self.is_dir(parent) => Err(io_error(
                path,
                ErrorKind::NotFound,
                "the parent directory does not exist",
            )),
            _ => Ok(()),
        }
    }
}

fn make_dirs(state: &mut FsState, dir: &HostPath, mode: FileMode) -> Result<(), FsError> {
    let mut missing = Vec::new();
    for ancestor in dir.as_path().ancestors() {
        if state.files.contains_key(ancestor) {
            return Err(io_error(dir, ErrorKind::NotADirectory, "not a directory"));
        }
        if !state.is_dir(ancestor) {
            missing.push(ancestor.to_path_buf());
        }
    }
    state
        .dirs
        .extend(missing.into_iter().map(|path| (path, mode)));
    Ok(())
}

impl MemFs {
    pub fn new() -> MemFs {
        MemFs::default()
    }

    pub fn fail_at(&self, op: FsOp, nth: u32, error: FsError) {
        let mut state = locked(&self.state);
        let due = state.calls.get(&op).copied().unwrap_or(0) + nth;
        state.faults.push(FsFault {
            op,
            nth: due,
            error: Some(error),
        });
    }

    pub fn read_string(&self, path: &HostPath) -> Option<String> {
        locked(&self.state)
            .files
            .get(path.as_path())
            .map(|file| String::from_utf8_lossy(&file.bytes).into_owned())
    }

    fn enter(&self, op: FsOp) -> Result<MutexGuard<'_, FsState>, FsError> {
        let mut state = locked(&self.state);
        let count = state.calls.entry(op).or_insert(0);
        *count += 1;
        let nth = *count;
        let fault = state
            .faults
            .iter_mut()
            .find(|fault| fault.op == op && fault.nth == nth)
            .and_then(|fault| fault.error.take());
        match fault {
            Some(error) => Err(error),
            None => Ok(state),
        }
    }
}

#[async_trait]
impl FileSystem for MemFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let state = self.enter(FsOp::Read)?;
        if state.is_dir(path.as_path()) {
            return Err(io_error(path, ErrorKind::IsADirectory, "is a directory"));
        }
        Ok(state
            .files
            .get(path.as_path())
            .map(|file| file.bytes.clone()))
    }

    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        let state = self.enter(FsOp::Stat)?;
        if state.files.contains_key(path.as_path()) {
            Ok(Some(FileKind::File))
        } else if state.is_dir(path.as_path()) {
            Ok(Some(FileKind::Dir))
        } else {
            Ok(None)
        }
    }

    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        let state = self.enter(FsOp::StatFollowing)?;
        if state.files.contains_key(path.as_path()) {
            Ok(Some(FileKind::File))
        } else if state.is_dir(path.as_path()) {
            Ok(Some(FileKind::Dir))
        } else {
            Ok(None)
        }
    }

    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        let state = locked(&self.state);
        Ok(
            (state.files.contains_key(path.as_path()) || state.is_dir(path.as_path()))
                .then(|| path.clone()),
        )
    }

    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        let state = self.enter(FsOp::FileLen)?;
        if state.is_dir(path.as_path()) {
            return Err(io_error(path, ErrorKind::IsADirectory, "is a directory"));
        }
        Ok(state
            .files
            .get(path.as_path())
            .map(|file| file.bytes.len() as u64))
    }

    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        let mut state = self.enter(FsOp::Identity)?;
        if !state.occupied(path.as_path()) {
            return Ok(None);
        }
        let key = path.as_path().to_path_buf();
        let ino = if let Some(known) = state.inodes.get(&key) {
            *known
        } else {
            state.next_inode += 1;
            let fresh = state.next_inode;
            state.inodes.insert(key, fresh);
            fresh
        };
        Ok(Some(FileIdentity { dev: 1, ino }))
    }

    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        let state = self.enter(FsOp::List)?;
        if state.files.contains_key(dir.as_path()) {
            return Err(io_error(dir, ErrorKind::NotADirectory, "not a directory"));
        }
        if !state.is_dir(dir.as_path()) {
            return Err(io_error(dir, ErrorKind::NotFound, "no such directory"));
        }
        let child = |path: &PathBuf, kind: FileKind| {
            (path.parent() == Some(dir.as_path())).then(|| DirEntry {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                kind,
            })
        };
        let mut entries: Vec<DirEntry> = state
            .files
            .keys()
            .filter_map(|path| child(path, FileKind::File))
            .chain(
                state
                    .dirs
                    .keys()
                    .filter_map(|path| child(path, FileKind::Dir)),
            )
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::CreateDirAll)?;
        make_dirs(&mut state, dir, FileMode(0o755))
    }

    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::CreatePrivateDirAll)?;
        make_dirs(&mut state, dir, FileMode(0o700))?;
        if let Some(mode) = state.dirs.get_mut(dir.as_path()) {
            *mode = FileMode(0o700);
        }
        Ok(())
    }

    async fn write_new(
        &self,
        path: &HostPath,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::WriteNew)?;
        state.parent_dir(path)?;
        if state.occupied(path.as_path()) {
            return Err(FsError::AlreadyExists(path.clone()));
        }
        state.files.insert(
            path.as_path().to_path_buf(),
            MemFile {
                bytes: bytes.to_vec(),
                mode: FileMode(mode.0 & 0o7777),
            },
        );
        Ok(())
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::Rename)?;
        if state.is_dir(from.as_path()) {
            return Err(io_error(
                from,
                ErrorKind::Unsupported,
                "MemFs does not rename directories",
            ));
        }
        if !state.files.contains_key(from.as_path()) {
            return Err(io_error(from, ErrorKind::NotFound, "no such file"));
        }
        state.parent_dir(to)?;
        if state.is_dir(to.as_path()) {
            return Err(io_error(to, ErrorKind::IsADirectory, "is a directory"));
        }
        if let Some(file) = state.files.remove(from.as_path()) {
            state.files.insert(to.as_path().to_path_buf(), file);
        }
        state.inodes.remove(to.as_path());
        if let Some(ino) = state.inodes.remove(from.as_path()) {
            state.inodes.insert(to.as_path().to_path_buf(), ino);
        }
        Ok(())
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::RemoveFile)?;
        if state.is_dir(path.as_path()) {
            return Err(io_error(path, ErrorKind::IsADirectory, "is a directory"));
        }
        state.files.remove(path.as_path());
        state.inodes.remove(path.as_path());
        Ok(())
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        let mut state = self.enter(FsOp::RemoveDirAll)?;
        if state.files.contains_key(path.as_path()) {
            return Err(io_error(path, ErrorKind::NotADirectory, "not a directory"));
        }
        let root = path.as_path();
        state.files.retain(|file, _| !file.starts_with(root));
        state.dirs.retain(|dir, _| !dir.starts_with(root));
        state.inodes.retain(|node, _| !node.starts_with(root));
        Ok(())
    }

    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError> {
        let state = self.enter(FsOp::Mode)?;
        if let Some(file) = state.files.get(path.as_path()) {
            Ok(Some(file.mode))
        } else if let Some(mode) = state.dirs.get(path.as_path()) {
            Ok(Some(*mode))
        } else if state.is_dir(path.as_path()) {
            Ok(Some(FileMode(0o755)))
        } else {
            Ok(None)
        }
    }

    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        let state = self.enter(FsOp::SyncDir)?;
        if state.is_dir(dir.as_path()) {
            Ok(())
        } else {
            Err(io_error(dir, ErrorKind::NotFound, "no such directory"))
        }
    }

    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        let mut state = self.enter(FsOp::Lock)?;
        state.parent_dir(path)?;
        if state.is_dir(path.as_path()) {
            return Err(io_error(path, ErrorKind::IsADirectory, "is a directory"));
        }
        if !state.locked.insert(path.as_path().to_path_buf()) {
            return Err(FsError::LockHeld(path.clone()));
        }
        state
            .files
            .entry(path.as_path().to_path_buf())
            .or_insert_with(|| MemFile {
                bytes: Vec::new(),
                mode: FileMode(0o600),
            });
        Ok(FileLock::from_guard(
            path.clone(),
            Held {
                state: Arc::clone(&self.state),
                path: path.as_path().to_path_buf(),
            },
        ))
    }

    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        let state = self.enter(FsOp::ReadAt)?;
        if state.is_dir(path.as_path()) {
            return Err(io_error(path, ErrorKind::IsADirectory, "is a directory"));
        }
        let Some(file) = state.files.get(path.as_path()) else {
            return Err(io_error(path, ErrorKind::NotFound, "no such file"));
        };
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(file.bytes.len());
        let end = start.saturating_add(len).min(file.bytes.len());
        Ok(file
            .bytes
            .get(start..end)
            .map(<[u8]>::to_vec)
            .unwrap_or_default())
    }

    async fn read_link(
        &self,
        root: &HostPath,
        _file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        Err(FsError::Io {
            path: root.clone(),
            kind: ErrorKind::Unsupported,
            message: "read_link is unsupported by MemFs".to_owned(),
        })
    }

    async fn write_symlink(&self, path: &HostPath, _target: &str) -> Result<(), FsError> {
        Err(FsError::Io {
            path: path.clone(),
            kind: ErrorKind::Unsupported,
            message: "write_symlink is unsupported by MemFs".to_owned(),
        })
    }

    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let state = self.enter(FsOp::Read)?;
        let mut current = root.as_path().to_path_buf();
        let mut names = file.as_str().split('/').peekable();
        while let Some(name) = names.next() {
            current.push(name);
            let last = names.peek().is_none();
            if let Some(found) = state.files.get(&current) {
                return Ok(if last {
                    Ok(RegularFile {
                        bytes: found
                            .bytes
                            .iter()
                            .take(max_bytes.saturating_add(1))
                            .copied()
                            .collect(),
                        mode: found.mode,
                    })
                } else {
                    Err(RegularFileProblem::Missing)
                });
            }
            if !state.is_dir(&current) {
                return Ok(Err(RegularFileProblem::Missing));
            }
            if last {
                return Ok(Err(RegularFileProblem::NotARegularFile));
            }
        }
        Ok(Err(RegularFileProblem::Missing))
    }
}
