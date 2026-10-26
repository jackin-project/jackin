// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::io::{ErrorKind, Read};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use jackin_protocol::attach::{MAX_FILE_EXPORT_NAME_BYTES, MAX_FILE_EXPORT_PATH_BYTES};
use nix::fcntl::{AtFlags, OFlag, open, openat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstat, fstatat};
use sha2::{Digest, Sha256};

const MAX_EXPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct PreparedExport {
    pub(super) source: PathBuf,
    pub(super) file_name: String,
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: [u8; 32],
}

#[derive(Debug)]
pub(super) enum PrepareError {
    CandidateRejected(anyhow::Error),
    ReadFailed(anyhow::Error),
}
impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CandidateRejected(error) | Self::ReadFailed(error) => write!(f, "{error:#}"),
        }
    }
}
impl std::error::Error for PrepareError {}

struct Budget<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
}
impl Budget<'_> {
    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "file export cancelled"
        );
        ensure!(
            Instant::now() < self.deadline,
            "file export deadline expired"
        );
        Ok(())
    }
}

pub(super) fn prepare_export(
    requested_path: &str,
    workdir: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedExport, PrepareError> {
    let budget = Budget {
        cancelled,
        deadline,
    };
    prepare(requested_path, workdir, &budget)
}

fn prepare(
    requested_path: &str,
    workdir: &Path,
    budget: &Budget<'_>,
) -> Result<PreparedExport, PrepareError> {
    let mut admitted = admit(requested_path, workdir, budget).map_err(|error| {
        if budget.check().is_err() {
            PrepareError::ReadFailed(error)
        } else {
            PrepareError::CandidateRejected(error)
        }
    })?;
    let result = (|| -> Result<PreparedExport> {
        let size = usize::try_from(admitted.fingerprint.len).context("export size overflow")?;
        let mut bytes = Vec::with_capacity(size);
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 8192];
        while bytes.len() < size {
            let remaining = size - bytes.len();
            let limit = remaining.min(buffer.len());
            let count = read_once(&mut admitted.file, &mut buffer[..limit], budget)?;
            ensure!(count != 0, "export source truncated while reading");
            ensure!(
                bytes.len().checked_add(count).is_some_and(|n| n <= size),
                "export size overflow"
            );
            bytes.extend_from_slice(&buffer[..count]);
            hash.update(&buffer[..count]);
        }
        ensure!(
            read_once(&mut admitted.file, &mut [0_u8; 1], budget)? == 0,
            "export source grew while reading"
        );

        admitted.validate(budget)?;
        Ok(PreparedExport {
            source: admitted.source,
            file_name: admitted.file_name,
            bytes,
            sha256: hash.finalize().into(),
        })
    })();
    result.map_err(PrepareError::ReadFailed)
}

fn read_once(file: &mut File, bytes: &mut [u8], budget: &Budget<'_>) -> Result<usize> {
    loop {
        budget.check()?;
        match file.read(bytes) {
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            result => {
                let count = result.context("reading admitted export source")?;
                budget.check()?;
                return Ok(count);
            }
        }
    }
}

struct Directory {
    fd: OwnedFd,
    name: Option<OsString>,
}
struct Admitted {
    source: PathBuf,
    file_name: String,
    file: File,
    fingerprint: Fingerprint,
    directories: Vec<Directory>,
    member: OsString,
}

#[derive(Debug, PartialEq, Eq)]
struct Fingerprint {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    len: u64,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<&Metadata> for Fingerprint {
    fn from(value: &Metadata) -> Self {
        Self {
            dev: value.dev(),
            ino: value.ino(),
            mode: value.mode(),
            uid: value.uid(),
            gid: value.gid(),
            len: value.len(),
            links: value.nlink(),
            modified: (value.mtime(), value.mtime_nsec()),
            changed: (value.ctime(), value.ctime_nsec()),
        }
    }
}

fn resolve(requested: &str, workdir: &Path, budget: &Budget<'_>) -> Result<(PathBuf, String)> {
    budget.check()?;
    ensure!(!requested.trim().is_empty(), "path is empty");
    let raw = Path::new(requested.trim());
    let candidate = if raw.is_absolute() {
        raw.to_owned()
    } else {
        workdir.join(raw)
    };
    let source = candidate
        .canonicalize()
        .context("resolving export source")?;
    budget.check()?;
    let workspace = workdir
        .canonicalize()
        .context("resolving export workspace")?;
    budget.check()?;
    let run = Path::new(jackin_core::container_paths::RUN_DIR);
    ensure!(
        (source.starts_with(&workspace) && source != workspace)
            || (source.starts_with(run) && source != run),
        "path must be inside the workspace or {}",
        run.display()
    );
    ensure!(
        source.display().to_string().len() <= MAX_FILE_EXPORT_PATH_BYTES,
        "resolved path exceeds export protocol cap"
    );
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .context("file has no UTF-8 file name")?
        .to_owned();
    ensure!(
        name.len() <= MAX_FILE_EXPORT_NAME_BYTES,
        "file name exceeds export protocol cap"
    );
    Ok((source, name))
}

fn admit(requested: &str, workdir: &Path, budget: &Budget<'_>) -> Result<Admitted> {
    let (source, file_name) = resolve(requested, workdir, budget)?;

    let parent = source.parent().context("export source has no parent")?;
    let directories = pin_directories(parent, budget)?;
    let directory = &directories.last().context("export parent missing")?.fd;
    let member = source
        .file_name()
        .context("export source has no member")?
        .to_owned();
    budget.check()?;
    let before = fstatat(directory, member.as_os_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
    ensure!(
        kind(&before, SFlag::S_IFREG),
        "only regular files can be exported"
    );

    budget.check()?;
    let fd = openat(directory, member.as_os_str(), file_flags(), Mode::empty())?;
    budget.check()?;
    let opened = fstat(&fd)?;
    ensure!(
        kind(&opened, SFlag::S_IFREG) && same_object(&before, &opened),
        "export source changed during admission"
    );
    let file = File::from(fd);
    let metadata = file.metadata()?;
    ensure!(
        metadata.len() <= MAX_EXPORT_FILE_BYTES,
        "file is {} bytes; current export cap is {MAX_EXPORT_FILE_BYTES} bytes",
        metadata.len()
    );
    let admitted = Admitted {
        source,
        file_name,
        file,
        fingerprint: Fingerprint::from(&metadata),
        directories,
        member,
    };
    admitted.validate(budget)?;
    Ok(admitted)
}

fn file_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC
}
fn kind(stat: &FileStat, expected: SFlag) -> bool {
    stat.st_mode & SFlag::S_IFMT.bits() == expected.bits()
}
fn same_object(a: &FileStat, b: &FileStat) -> bool {
    a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && a.st_mode == b.st_mode
        && a.st_uid == b.st_uid
        && a.st_gid == b.st_gid
}

fn pin_directories(path: &Path, budget: &Budget<'_>) -> Result<Vec<Directory>> {
    budget.check()?;
    let mut directories = vec![Directory {
        fd: open(
            Path::new("/"),
            file_flags() | OFlag::O_DIRECTORY,
            Mode::empty(),
        )?,
        name: None,
    }];
    for component in path.components() {
        budget.check()?;
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            _ => anyhow::bail!("export path has a non-normal component"),
        };
        let parent = &directories.last().context("export ancestor missing")?.fd;
        let before = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
        ensure!(
            kind(&before, SFlag::S_IFDIR),
            "export ancestor is not a directory"
        );
        budget.check()?;
        let fd = openat(
            parent,
            name,
            file_flags() | OFlag::O_DIRECTORY,
            Mode::empty(),
        )?;
        budget.check()?;
        ensure!(
            same_object(&before, &fstat(&fd)?),
            "export ancestor changed while opening"
        );
        directories.push(Directory {
            fd,
            name: Some(name.to_owned()),
        });
    }
    Ok(directories)
}

impl Admitted {
    fn validate(&self, budget: &Budget<'_>) -> Result<()> {
        budget.check()?;
        for pair in self.directories.windows(2) {
            budget.check()?;
            let name = pair[1]
                .name
                .as_ref()
                .context("export ancestor name missing")?;
            let entry = fstatat(&pair[0].fd, name.as_os_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
            ensure!(
                kind(&entry, SFlag::S_IFDIR) && same_object(&entry, &fstat(&pair[1].fd)?),
                "export ancestor binding changed"
            );
        }
        let parent = &self.directories.last().context("export parent missing")?.fd;
        let entry = fstatat(
            parent,
            self.member.as_os_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )?;
        let current = fstat(&self.file)?;
        ensure!(
            kind(&entry, SFlag::S_IFREG) && same_object(&entry, &current),
            "export member binding changed"
        );
        ensure!(
            Fingerprint::from(&self.file.metadata()?) == self.fingerprint,
            "export source mutated after admission"
        );
        budget.check()
    }
}
