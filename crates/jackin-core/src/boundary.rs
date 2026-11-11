// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::{self, Debug};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::display::{plain, plain_duration};
use crate::ids::EnvName;
use crate::paths::{HostPath, RepoPath};
use crate::secret::{Secret, SecretBytes};

const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub fn rfc3339(self) -> String {
        let days = self.0.div_euclid(SECONDS_PER_DAY);
        let seconds = self.0.rem_euclid(SECONDS_PER_DAY);
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            seconds / 3_600,
            seconds % 3_600 / 60,
            seconds % 60
        )
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot(BTreeMap<String, String>);

impl Debug for EnvSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvSnapshot")
            .field("names", &self.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[async_trait]
pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> Timestamp;
    fn utc_offset_secs(&self) -> i32;
    async fn sleep(&self, duration: Duration);
}

pub const UNBOUNDED_READ: usize = usize::MAX;

#[async_trait]
pub trait FileSystem: Send + Sync + Debug {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError>;
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError>;
    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError>;
    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError>;
    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError>;
    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError>;
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError>;
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn write_new(&self, path: &HostPath, bytes: &[u8], mode: FileMode)
    -> Result<(), FsError>;
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError>;
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError>;
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError>;
    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError>;
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError>;
    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError>;
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError>;
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError>;
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct RegularFile {
    pub bytes: Vec<u8>,
    pub mode: FileMode,
}

impl Debug for RegularFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegularFile")
            .field("len", &self.bytes.len())
            .field("mode", &self.mode)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegularFileProblem {
    Symlink,
    NotARegularFile,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkProblem {
    Symlink,
    NotALink,
    Missing,
    NonUtf8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileMode(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub dev: u64,
    pub ino: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    File,
    Dir,
    Symlink,
    Socket,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: FileKind,
}

#[derive(Debug)]
pub struct FileLock {
    path: HostPath,
    _guard: Box<dyn Any + Send + Sync>,
}

impl FileLock {
    pub fn from_guard<G: Send + Sync + 'static>(path: HostPath, guard: G) -> FileLock {
        FileLock {
            path,
            _guard: Box::new(guard),
        }
    }

    pub(crate) fn new(path: HostPath, file: std::fs::File) -> FileLock {
        FileLock::from_guard(path, file)
    }

    pub fn path(&self) -> &HostPath {
        &self.path
    }
}

impl EnvSnapshot {
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> EnvSnapshot {
        EnvSnapshot(pairs.into_iter().collect())
    }

    pub fn get(&self, name: &EnvName) -> Option<&str> {
        self.0.get(name.as_str()).map(String::as_str)
    }
}

impl FsError {
    pub(crate) fn io(path: &HostPath, source: &std::io::Error) -> FsError {
        FsError::Io {
            path: path.clone(),
            kind: source.kind(),
            message: source.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Program {
    Docker(DockerSubcommand),
    Openshell,
    Git,
    Ssh,
    Op,
    Security,
    Gh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerSubcommand {
    BuildxBuild,
    BuildxCreate,
    BuildxInspect,
    ImageInspect,
    Push,
}

impl DockerSubcommand {
    pub fn argv(self) -> &'static [&'static str] {
        match self {
            DockerSubcommand::BuildxBuild => &["buildx", "build"],
            DockerSubcommand::BuildxCreate => &["buildx", "create"],
            DockerSubcommand::BuildxInspect => &["buildx", "inspect"],
            DockerSubcommand::ImageInspect => &["image", "inspect"],
            DockerSubcommand::Push => &["push"],
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.binary())?;
        for argument in self.leading_args() {
            write!(f, " {argument}")?;
        }
        Ok(())
    }
}

impl Program {
    pub fn binary(self) -> &'static str {
        match self {
            Self::Docker(_) => "docker",
            Self::Openshell => "openshell",
            Self::Git => "git",
            Self::Ssh => "ssh",
            Self::Op => "op",
            Self::Security => "security",
            Self::Gh => "gh",
        }
    }

    pub fn leading_args(self) -> &'static [&'static str] {
        match self {
            Self::Docker(subcommand) => subcommand.argv(),
            Self::Openshell | Self::Git | Self::Ssh | Self::Op | Self::Security | Self::Gh => &[],
        }
    }
}

#[derive(Debug)]
pub struct ProcessSpec {
    pub program: Program,
    pub args: Vec<String>,
    pub cwd: Option<HostPath>,
    pub env: Vec<(String, EnvValue)>,
    pub env_remove_prefixes: Vec<String>,
    pub stdin: ProcessStdin,
    pub timeout: Duration,
}

#[derive(Debug)]
pub enum EnvValue {
    Plain(String),
    Secret(Secret),
}

#[derive(Debug)]
pub enum ProcessStdin {
    Null,
    Bytes(SecretBytes),
    File { path: HostPath, offset: u64 },
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: ProcessStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Debug for ProcessOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcessOutput")
            .field("status", &self.status)
            .field("stdout_len", &self.stdout.len())
            .field("stderr_len", &self.stderr.len())
            .finish()
    }
}

pub struct SecretOutput {
    pub status: ProcessStatus,
    pub stdout: SecretBytes,
    pub stderr: Vec<u8>,
}

impl Debug for SecretOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretOutput")
            .field("status", &self.status)
            .field("stdout", &self.stdout)
            .field("stderr_len", &self.stderr.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessStatus {
    Exited(i32),
    Signaled(i32),
}

impl fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessStatus::Exited(code) => write!(f, "exit code {code}"),
            ProcessStatus::Signaled(signal) => write!(f, "signal {signal}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct OutputLine {
    pub stream: OutputStream,
    pub text: String,
}

impl Debug for OutputLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputLine")
            .field("stream", &self.stream)
            .field("text_len", &self.text.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl fmt::Display for OutputStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OutputStream::Stdout => "stdout",
            OutputStream::Stderr => "stderr",
        })
    }
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

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError>;
}

#[derive(Debug, Error)]
pub enum FsError {
    #[error("i/o error at {path}: {}", plain(.message))]
    Io {
        path: HostPath,
        kind: std::io::ErrorKind,
        message: String,
    },
    #[error("already exists: {0}")]
    AlreadyExists(HostPath),
    #[error("lock held: {0}")]
    LockHeld(HostPath),
    #[error("the lock {lock} does not guard {target}")]
    WrongLock { lock: HostPath, target: HostPath },
    #[error("refusing the symbolic link {0}")]
    SymlinkTarget(HostPath),
    #[error("{path} was replaced, but syncing its directory failed: {}", plain(.message))]
    Unsynced { path: HostPath, message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum GroupFault {
    #[error("processes of its group were still alive after {}", plain_duration(.after))]
    Alive { after: Duration },
    #[error("cannot stop the processes of its group: {}", plain(.message))]
    Unreachable { message: String },
}

impl GroupFault {
    #[must_use]
    pub fn note(group: Option<&GroupFault>) -> String {
        group.map_or_else(String::new, |fault| format!("; {fault}"))
    }
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to start {program}: {}", plain(.message))]
    Spawn { program: Program, message: String },
    #[error("{program} timed out after {}{}", plain_duration(.after), GroupFault::note(.group.as_ref()))]
    Timeout {
        program: Program,
        after: Duration,
        group: Option<GroupFault>,
    },
    #[error("{program} was cancelled{}", GroupFault::note(.group.as_ref()))]
    Cancelled {
        program: Program,
        group: Option<GroupFault>,
    },
    #[error("{program} wrote more than {limit} bytes to {stream}")]
    OutputTooLarge {
        program: Program,
        stream: OutputStream,
        limit: usize,
    },
    #[error("reading {stream} of {program} failed: {}", plain(.message))]
    Read {
        program: Program,
        stream: OutputStream,
        message: String,
    },
    #[error("{program}: {fault}")]
    Group { program: Program, fault: GroupFault },
}
