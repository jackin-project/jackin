// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::paths::HostPath;
use crate::secret::SecretBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvSnapshot(BTreeMap<String, String>);

#[async_trait]
pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> Timestamp;
    async fn sleep(&self, duration: Duration);
}

#[async_trait]
pub trait FileSystem: Send + Sync + Debug {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError>;
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError>;
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError>;
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn write_new(&self, path: &HostPath, bytes: &[u8], mode: FileMode)
    -> Result<(), FsError>;
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError>;
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError>;
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileMode(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    File,
    Dir,
    Symlink,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: FileKind,
}

#[derive(Debug)]
pub struct FileLock {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Program {
    Docker,
    Openshell,
    Git,
    Ssh,
    Op,
}

#[derive(Debug)]
pub struct ProcessSpec {
    pub program: Program,
    pub args: Vec<String>,
    pub cwd: Option<HostPath>,
    pub env: Vec<(String, String)>,
    pub stdin: ProcessStdin,
    pub timeout: Duration,
}

#[derive(Debug)]
pub enum ProcessStdin {
    Null,
    Bytes(SecretBytes),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: ProcessStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessStatus {
    Exited(i32),
    Signaled(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub stream: OutputStream,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

pub type LineSink = tokio::sync::mpsc::Sender<OutputLine>;

#[async_trait]
pub trait ProcessRunner: Send + Sync + Debug {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError>;
}

#[derive(Debug, Error)]
pub enum FsError {
    #[error("i/o error at {path:?}: {message}")]
    Io {
        path: HostPath,
        kind: std::io::ErrorKind,
        message: String,
    },
    #[error("already exists: {0:?}")]
    AlreadyExists(HostPath),
    #[error("lock held: {0:?}")]
    LockHeld(HostPath),
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to start {program:?}: {message}")]
    Spawn { program: Program, message: String },
    #[error("{program:?} timed out after {after:?}")]
    Timeout { program: Program, after: Duration },
    #[error("{program:?} was cancelled")]
    Cancelled { program: Program },
}
