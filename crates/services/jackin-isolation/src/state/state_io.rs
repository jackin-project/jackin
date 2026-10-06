// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Descriptor-bound state transactions. Cooperating readers/writers hold the
//! same isolation lock; independent instance-manifest writers are protected by
//! before/after snapshot witnesses. Recovery directories are private host-owned namespaces; arbitrary
//! hostile writers with the same UID are outside that transaction contract.

use anyhow::{Context, ensure};
use nix::errno::Errno;
use nix::fcntl::{AtFlags, Flock, FlockArg, OFlag, open, openat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstat, fstatat, mkdirat};
use nix::unistd::{UnlinkatFlags, unlinkat};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;
static TRANSACTION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct CommitHooks<'a> {
    before_validation: Option<&'a dyn Fn()>,
    before_install: Option<&'a dyn Fn()>,
    after_install: Option<&'a dyn Fn()>,
}

pub(super) struct FileSnapshot {
    pub(super) bytes: Vec<u8>,
    identity: FileStat,
    _file: File,
}

struct Edge {
    parent: OwnedFd,
    name: OsString,
    identity: FileStat,
}

pub(super) struct StateDirectory {
    directory: OwnedFd,
    edges: Vec<Edge>,
    _lock: Flock<File>,
}

fn directory_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

fn same_file(a: &FileStat, b: &FileStat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}

impl StateDirectory {
    pub(super) fn open(path: &Path, create: bool) -> anyhow::Result<Option<Self>> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        // Only the platform's immutable system aliases are normalized. Owned
        // state components and arbitrary configured aliases are never followed.
        #[cfg(target_os = "macos")]
        let absolute = if let Ok(rest) = absolute.strip_prefix("/var") {
            Path::new("/private/var").join(rest)
        } else if let Ok(rest) = absolute.strip_prefix("/tmp") {
            Path::new("/private/tmp").join(rest)
        } else {
            absolute
        };
        let mut directory = open(Path::new("/"), directory_flags(), Mode::empty())?;
        let mut edges = Vec::new();
        let mut components: Vec<OsString> = Vec::new();
        for component in absolute.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => components.push(name.to_owned()),
                _ => anyhow::bail!("state directory cannot contain parent traversal"),
            }
        }
        components.push(OsString::from(".jackin"));
        for name in components {
            let child = match openat(
                &directory,
                Path::new(&name),
                directory_flags(),
                Mode::empty(),
            ) {
                Ok(child) => child,
                Err(Errno::ENOENT) if !create => return Ok(None),
                Err(Errno::ENOENT) => {
                    match mkdirat(&directory, Path::new(&name), Mode::S_IRWXU) {
                        Ok(()) | Err(Errno::EEXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    openat(
                        &directory,
                        Path::new(&name),
                        directory_flags(),
                        Mode::empty(),
                    )?
                }
                Err(error) => {
                    return Err(error)
                        .context("state directory contains an alias or invalid component");
                }
            };
            let identity = fstat(&child)?;
            ensure!(
                same_file(
                    &identity,
                    &fstatat(&directory, Path::new(&name), AtFlags::AT_SYMLINK_NOFOLLOW)?
                ),
                "state directory changed during pinning"
            );
            edges.push(Edge {
                parent: directory,
                name,
                identity,
            });
            directory = child;
        }
        // Lock the pinned directory inode, so pure reads require no writable
        // lock file and never create metadata in a fresh/read-only state dir.
        let lock: File = directory.try_clone()?.into();
        let lock = match Flock::lock(lock, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => lock,
            Err((_, Errno::EWOULDBLOCK)) => anyhow::bail!(
                "instance isolation state busy; retry after the current state transaction completes"
            ),
            Err((_, error)) => return Err(error.into()),
        };
        let pinned = Self {
            directory,
            edges,
            _lock: lock,
        };
        pinned.validate_namespace()?;
        Ok(Some(pinned))
    }

    fn validate_namespace(&self) -> anyhow::Result<()> {
        for edge in &self.edges {
            ensure!(
                same_file(
                    &edge.identity,
                    &fstatat(
                        &edge.parent,
                        Path::new(&edge.name),
                        AtFlags::AT_SYMLINK_NOFOLLOW
                    )?
                ),
                "state directory namespace changed; preserved original state"
            );
        }
        Ok(())
    }

    pub(super) fn read_file(&self, name: &str) -> anyhow::Result<Option<FileSnapshot>> {
        validate_name(name)?;
        self.validate_namespace()?;
        let expected = match fstatat(&self.directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(expected) => expected,
            Err(Errno::ENOENT) => {
                self.validate_namespace()?;
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        ensure!(
            SFlag::from_bits_truncate(expected.st_mode) == SFlag::S_IFREG,
            "state metadata is not a regular file"
        );
        let fd = openat(
            &self.directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )?;
        ensure!(
            same_file(&expected, &fstat(&fd)?),
            "state file changed while opening"
        );
        let snapshot = snapshot(fd)?;
        ensure!(
            same_file(
                &snapshot.identity,
                &fstatat(&self.directory, name, AtFlags::AT_SYMLINK_NOFOLLOW)?
            ),
            "state file changed while reading"
        );
        self.validate_namespace()?;
        Ok(Some(snapshot))
    }

    fn matches(&self, name: &str, expected: &FileSnapshot) -> anyhow::Result<bool> {
        Ok(self.read_file(name)?.is_some_and(|current| {
            same_file(&expected.identity, &current.identity) && current.bytes == expected.bytes
        }))
    }

    pub(super) fn validate_files_unchanged(
        &self,
        witnesses: &[(&str, &FileSnapshot)],
    ) -> anyhow::Result<()> {
        self.validate_namespace()?;
        for (name, expected) in witnesses {
            ensure!(
                self.matches(name, expected)?,
                "state admission witness changed while reading"
            );
        }
        self.validate_namespace()
    }

    pub(super) fn write_file(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let expected = self.read_file(name)?;
        self.commit(name, bytes, expected.as_ref(), &[], CommitHooks::default())
    }

    pub(super) fn create_file(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        validate_name(name)?;
        ensure!(self.read_file(name)?.is_none(), "state file already exists");
        self.commit(name, bytes, None, &[], CommitHooks::default())
    }

    pub(super) fn remove_file(&self, name: &str) -> anyhow::Result<()> {
        validate_name(name)?;
        let Some(expected) = self.read_file(name)? else {
            return Ok(());
        };
        self.validate_namespace()?;
        ensure!(
            self.matches(name, &expected)?,
            "state file changed before removal"
        );
        unlinkat(&self.directory, name, UnlinkatFlags::NoRemoveDir)?;
        nix::unistd::fsync(&self.directory)?;
        self.validate_namespace()
    }

    pub(super) fn write_file_if_unchanged(
        &self,
        name: &str,
        bytes: &[u8],
        expected: &FileSnapshot,
        witnesses: &[(&str, &FileSnapshot)],
    ) -> anyhow::Result<()> {
        self.commit(
            name,
            bytes,
            Some(expected),
            witnesses,
            CommitHooks::default(),
        )
    }

    fn commit(
        &self,
        name: &str,
        bytes: &[u8],
        expected: Option<&FileSnapshot>,
        witnesses: &[(&str, &FileSnapshot)],
        hooks: CommitHooks<'_>,
    ) -> anyhow::Result<()> {
        validate_name(name)?;
        ensure!(
            bytes.len() as u64 <= MAX_STATE_BYTES,
            "state file exceeds 16 MiB limit"
        );
        self.validate_namespace()?;
        let (transaction_name, transaction, transaction_identity) = self.create_transaction()?;
        let candidate_identity = create_candidate(&transaction, bytes, &transaction_name)?;
        if let Some(hook) = hooks.before_validation {
            hook();
        }
        if let Err(error) = self.validate_commit_sources(name, expected, witnesses) {
            self.remove_uninstalled_candidate(&transaction, &transaction_name)
                .with_context(|| {
                    format!(
                        "state validation failed ({error:#}); cleanup incomplete in {transaction_name}"
                    )
                })?;
            return Err(error);
        }

        self.install_candidate(
            CandidateInstall {
                name,
                transaction_name,
                transaction,
                transaction_identity,
                candidate_identity,
                expected,
                witnesses,
            },
            &hooks,
        )
    }

    fn create_transaction(&self) -> anyhow::Result<(String, OwnedFd, FileStat)> {
        let mut transaction_name = None;
        for _ in 0..16 {
            let candidate = format!(
                ".isolation-txn-{}-{}",
                std::process::id(),
                TRANSACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            match mkdirat(&self.directory, candidate.as_str(), Mode::S_IRWXU) {
                Ok(()) => {
                    transaction_name = Some(candidate);
                    break;
                }
                Err(Errno::EEXIST) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let transaction_name = transaction_name.context("state transaction namespace exhausted")?;
        let transaction = openat(
            &self.directory,
            transaction_name.as_str(),
            directory_flags(),
            Mode::empty(),
        )
        .with_context(|| format!("cannot pin private state transaction {transaction_name}"))?;
        let identity = fstat(&transaction)?;
        ensure!(
            same_file(
                &identity,
                &fstatat(
                    &self.directory,
                    transaction_name.as_str(),
                    AtFlags::AT_SYMLINK_NOFOLLOW
                )?
            ),
            "private state transaction directory changed while pinning"
        );
        Ok((transaction_name, transaction, identity))
    }

    fn validate_commit_sources(
        &self,
        name: &str,
        expected: Option<&FileSnapshot>,
        witnesses: &[(&str, &FileSnapshot)],
    ) -> anyhow::Result<()> {
        self.validate_namespace()?;
        for (witness_name, witness) in witnesses {
            ensure!(
                self.matches(witness_name, witness)?,
                "instance manifest changed during identity recovery"
            );
        }
        if let Some(expected) = expected {
            ensure!(
                self.matches(name, expected)?,
                "isolation source changed during identity recovery"
            );
        }
        Ok(())
    }

    fn remove_uninstalled_candidate(
        &self,
        transaction: &OwnedFd,
        transaction_name: &str,
    ) -> anyhow::Result<()> {
        unlinkat(transaction, "candidate", UnlinkatFlags::NoRemoveDir)?;
        unlinkat(&self.directory, transaction_name, UnlinkatFlags::RemoveDir)?;
        Ok(())
    }

    fn install_candidate(
        &self,
        candidate: CandidateInstall<'_, '_>,
        hooks: &CommitHooks<'_>,
    ) -> anyhow::Result<()> {
        let CandidateInstall {
            name,
            transaction_name,
            transaction,
            transaction_identity,
            candidate_identity,
            expected,
            witnesses,
        } = candidate;
        let flags = if expected.is_some() {
            rustix::fs::RenameFlags::EXCHANGE
        } else {
            rustix::fs::RenameFlags::NOREPLACE
        };
        if let Some(hook) = hooks.before_install {
            hook();
        }
        ensure!(
            same_file(
                &transaction_identity,
                &fstatat(
                    &self.directory,
                    transaction_name.as_str(),
                    AtFlags::AT_SYMLINK_NOFOLLOW
                )?
            ),
            "private state transaction directory changed; retained recovery artifacts in {transaction_name}"
        );
        ensure!(
            same_file(
                &candidate_identity,
                &fstatat(&transaction, "candidate", AtFlags::AT_SYMLINK_NOFOLLOW)?
            ),
            "private state candidate changed; retained recovery artifacts in {transaction_name}"
        );
        if let Err(error) =
            rustix::fs::renameat_with(&transaction, "candidate", &self.directory, name, flags)
        {
            self.remove_uninstalled_candidate(&transaction, &transaction_name)
                .with_context(|| {
                    format!(
                        "atomic state install failed ({error}); cleanup incomplete in {transaction_name}"
                    )
                })?;
            return Err(error).context("atomic conditional state install unsupported or failed");
        }
        if let Some(hook) = hooks.after_install {
            hook();
        }
        if let Err(error) = self.validate_installed_candidate(
            name,
            &transaction,
            &candidate_identity,
            expected,
            witnesses,
        ) {
            if let Err(rollback_error) = self.rollback_candidate(
                name,
                &transaction_name,
                &transaction,
                &candidate_identity,
                expected,
            ) {
                return Err(error).context(format!(
                    "state rollback failed ({rollback_error}); retained recovery artifacts in {transaction_name}"
                ));
            }
            return Err(error).context(format!(
                "state transaction refused; retained recovery artifacts in {transaction_name}"
            ));
        }
        // Keep the displaced original until both namespaces are durable.
        nix::unistd::fsync(&transaction)
            .and_then(|()| nix::unistd::fsync(&self.directory))
            .with_context(|| format!("state install durability unconfirmed; retained recovery artifacts in {transaction_name}"))?;
        if expected.is_some() {
            unlinkat(&transaction, "candidate", UnlinkatFlags::NoRemoveDir).with_context(|| {
                format!("state installed; original recovery retained in {transaction_name}")
            })?;
        }
        unlinkat(
            &self.directory,
            transaction_name.as_str(),
            UnlinkatFlags::RemoveDir,
        )
        .with_context(|| {
            format!("state installed; transaction cleanup incomplete in {transaction_name}")
        })?;
        Ok(())
    }

    fn validate_installed_candidate(
        &self,
        name: &str,
        transaction: &OwnedFd,
        candidate_identity: &FileStat,
        expected: Option<&FileSnapshot>,
        witnesses: &[(&str, &FileSnapshot)],
    ) -> anyhow::Result<()> {
        self.validate_namespace()?;
        ensure!(
            same_file(
                candidate_identity,
                &fstatat(&self.directory, name, AtFlags::AT_SYMLINK_NOFOLLOW)?
            ),
            "installed state changed during identity recovery"
        );
        for (witness_name, witness) in witnesses {
            ensure!(
                self.matches(witness_name, witness)?,
                "instance manifest changed during identity recovery"
            );
        }
        if let Some(expected) = expected {
            let displaced = snapshot(openat(
                transaction,
                "candidate",
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
                Mode::empty(),
            )?)?;
            ensure!(
                same_file(&expected.identity, &displaced.identity)
                    && expected.bytes == displaced.bytes,
                "different isolation state displaced during identity recovery"
            );
        }
        Ok(())
    }

    fn rollback_candidate(
        &self,
        name: &str,
        transaction_name: &str,
        transaction: &OwnedFd,
        candidate_identity: &FileStat,
        expected: Option<&FileSnapshot>,
    ) -> anyhow::Result<()> {
        let installed_is_ours = fstatat(&self.directory, name, AtFlags::AT_SYMLINK_NOFOLLOW)
            .is_ok_and(|identity| same_file(candidate_identity, &identity));
        if installed_is_ours {
            let rollback = if expected.is_some() {
                rustix::fs::renameat_with(
                    transaction,
                    "candidate",
                    &self.directory,
                    name,
                    rustix::fs::RenameFlags::EXCHANGE,
                )
            } else {
                rustix::fs::renameat_with(
                    &self.directory,
                    name,
                    transaction,
                    "candidate",
                    rustix::fs::RenameFlags::NOREPLACE,
                )
            };
            rollback?;
        }
        nix::unistd::fsync(transaction)
            .and_then(|()| nix::unistd::fsync(&self.directory))
            .with_context(|| {
                format!(
                    "state transaction rollback durability unconfirmed; retained artifacts in {transaction_name}"
                )
            })
    }
}

struct CandidateInstall<'a, 'snapshot> {
    name: &'a str,
    transaction_name: String,
    transaction: OwnedFd,
    transaction_identity: FileStat,
    candidate_identity: FileStat,
    expected: Option<&'snapshot FileSnapshot>,
    witnesses: &'snapshot [(&'snapshot str, &'snapshot FileSnapshot)],
}

fn create_candidate(
    transaction: &OwnedFd,
    bytes: &[u8],
    transaction_name: &str,
) -> anyhow::Result<FileStat> {
    let mut candidate: File = openat(
        transaction,
        "candidate",
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )?
    .into();
    candidate.write_all(bytes).with_context(|| {
        format!("cannot prepare state candidate; retained artifacts in {transaction_name}")
    })?;
    candidate.sync_all().with_context(|| {
        format!("cannot synchronize state candidate; retained artifacts in {transaction_name}")
    })?;
    Ok(fstat(&candidate)?)
}

fn validate_name(name: &str) -> anyhow::Result<()> {
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\\'),
        "invalid state file name"
    );
    Ok(())
}

fn snapshot(fd: OwnedFd) -> anyhow::Result<FileSnapshot> {
    let file: File = fd.into();
    let identity = fstat(&file)?;
    ensure!(
        SFlag::from_bits_truncate(identity.st_mode) == SFlag::S_IFREG,
        "state metadata is not a regular file"
    );
    ensure!(
        identity.st_size >= 0 && identity.st_size.cast_unsigned() <= MAX_STATE_BYTES,
        "state file exceeds 16 MiB limit"
    );
    let mut bytes = Vec::new();
    (&file).take(MAX_STATE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_STATE_BYTES,
        "state file exceeds 16 MiB limit"
    );
    Ok(FileSnapshot {
        bytes,
        identity,
        _file: file,
    })
}

#[cfg(test)]
mod tests;
