// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Files-backend exact ref deletion without pathname-based Git mutations.
//! Git writers are excluded with the ordinary loose and packed ref locks.
//! Every filesystem operation is relative to a pinned, no-follow descriptor.
//! A removed entry is quarantined and checked *after* moving it: an unexpected
//! concurrent replacement is restored exclusively, or retained for recovery.
//! HEAD and unrelated reflogs are never touched. Callers must positively establish the
//! `files` backend before preparing this transaction.
//! The isolated actor must be stopped and the private transaction namespace
//! must remain trusted. Git locks exclude Git writers, not hostile raw writes
//! by processes with our UID.

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context as _, ensure};
use nix::errno::Errno;
use nix::fcntl::{AtFlags, Flock, FlockArg, OFlag, openat, renameat};
use nix::sys::stat::{FileStat, Mode, SFlag, fchmod, fstat, fstatat, mkdirat};
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

const MAX_METADATA: u64 = 16 * 1024 * 1024;
static NEXT_TRANSACTION: AtomicU64 = AtomicU64::new(0);

struct Snapshot {
    bytes: Vec<u8>,
    identity: FileStat,
}

struct Edge {
    parent: OwnedFd,
    name: String,
    identity: FileStat,
}

impl Edge {
    fn verify(&self) -> anyhow::Result<()> {
        let stat = fstatat(
            &self.parent,
            self.name.as_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )?;
        ensure!(
            same_file(&stat, &self.identity),
            "ref directory {} changed; record retained",
            self.name
        );
        Ok(())
    }
}

struct Reflog {
    parent: OwnedFd,
    leaf: String,
    snapshot: Snapshot,
}

/// A durable transaction journal. Conventional Git lock paths are hard links
/// to staged owner records here, so a crash never leaves an unidentifiable
/// empty `.lock` file. The lease flock distinguishes live owners from crashes.
struct RecoveryDir {
    root: OwnedFd,
    name: String,
    fd: OwnedFd,
    files: Vec<String>,
    preserve: Arc<AtomicBool>,
    _lease: Flock<File>,
}

impl RecoveryDir {
    fn create(root: BorrowedFd<'_>) -> anyhow::Result<Self> {
        let (name, fd) = recovery_directory(root)?;
        let lease: File = openat(
            &fd,
            "lease",
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into();
        let lease =
            Flock::lock(lease, FlockArg::LockExclusiveNonblock).map_err(|(_, error)| error)?;
        let mut owner: File = openat(
            &fd,
            "owner",
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into();
        owner.write_all(b"jackin-ref-transaction-v1\n")?;
        owner.sync_all()?;
        sync_directory(fd.as_fd())?;
        Ok(Self {
            root: root.try_clone_to_owned()?,
            name,
            fd,
            files: vec!["lease".into(), "owner".into()],
            preserve: Arc::new(AtomicBool::new(false)),
            _lease: lease,
        })
    }

    fn create_lock_candidate(
        &mut self,
        parent: BorrowedFd<'_>,
        leaf: &str,
    ) -> anyhow::Result<(String, File, FileStat, Vec<u8>)> {
        let sequence = NEXT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
        let candidate_name = format!("lock-{sequence}");
        let parent_stat = fstat(parent)?;
        let bytes = format!(
            "jackin-git-lock-v1\t{}\t{}\t{}\t{}\t{}\n",
            self.name, candidate_name, leaf, parent_stat.st_dev, parent_stat.st_ino
        )
        .into_bytes();
        let mut candidate: File = openat(
            &self.fd,
            candidate_name.as_str(),
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into();
        candidate.write_all(&bytes)?;
        candidate.sync_all()?;
        let identity = fstat(&candidate)?;
        self.files.push(candidate_name.clone());
        sync_directory(self.fd.as_fd())?;
        Ok((candidate_name, candidate, identity, bytes))
    }

    fn register_file(&mut self, name: &str) {
        self.files.push(name.to_owned());
    }

    fn preserve(&self) {
        self.preserve.store(true, Ordering::Release);
    }

    fn allow_cleanup(&self) {
        self.preserve.store(false, Ordering::Release);
    }
}

impl Drop for RecoveryDir {
    fn drop(&mut self) {
        if !self.preserve.load(Ordering::Acquire) {
            best_effort_unlink(
                self.fd.as_fd(),
                "exchange-probe",
                UnlinkatFlags::NoRemoveDir,
            );
            best_effort_unlink(
                self.fd.as_fd(),
                "packed-candidate",
                UnlinkatFlags::NoRemoveDir,
            );
            for name in &self.files {
                best_effort_unlink(self.fd.as_fd(), name, UnlinkatFlags::NoRemoveDir);
            }
            best_effort_unlink(self.root.as_fd(), &self.name, UnlinkatFlags::RemoveDir);
        }
    }
}

fn best_effort_unlink(parent: BorrowedFd<'_>, name: &str, flags: UnlinkatFlags) {
    match unlinkat(parent, name, flags) {
        Ok(()) | Err(_) => {}
    }
}

struct Lock {
    recovery_fd: OwnedFd,
    recovery_name: String,
    candidate_name: String,
    parent: OwnedFd,
    name: String,
    marker: Vec<u8>,
    _file: File,
    identity: FileStat,
    preserve: Arc<AtomicBool>,
    released: bool,
}

impl Lock {
    fn acquire(
        recovery: &mut RecoveryDir,
        parent: BorrowedFd<'_>,
        name: String,
    ) -> anyhow::Result<Self> {
        let (candidate_name, file, identity, marker) =
            recovery.create_lock_candidate(parent, &name)?;
        // Finish every fallible operation before exposing the owner record at
        // a Git lock path. Otherwise an FD clone error could strand a lock
        // whose recovery candidate the journal then removes on drop.
        let recovery_fd = recovery.fd.as_fd().try_clone_to_owned()?;
        let recovery_name = recovery.name.clone();
        let candidate_path = candidate_name.clone();
        let parent_fd = parent.try_clone_to_owned()?;
        let lock_name = name.clone();
        let preserve = Arc::clone(&recovery.preserve);
        if let Err(error) = linkat(
            &recovery.fd,
            candidate_name.as_str(),
            parent,
            name.as_str(),
            AtFlags::empty(),
        ) {
            if error == Errno::EEXIST
                && reclaim_abandoned_lock(recovery.root.as_fd(), parent, &name)?
            {
                linkat(
                    &recovery.fd,
                    candidate_name.as_str(),
                    parent,
                    name.as_str(),
                    AtFlags::empty(),
                )
                .with_context(|| format!("cannot exclusively acquire {name}; record retained"))?;
            } else {
                return Err(error).with_context(|| {
                    format!("cannot exclusively acquire {name}; record retained")
                });
            }
        }
        if let Err(error) = sync_directory(parent) {
            preserve.store(true, Ordering::Release);
            return Err(error).with_context(|| {
                format!("cannot synchronize acquired Git lock {name}; record retained")
            });
        }
        Ok(Self {
            recovery_fd,
            recovery_name,
            candidate_name: candidate_path,
            parent: parent_fd,
            name: lock_name,
            marker,
            _file: file,
            identity,
            preserve,
            released: false,
        })
    }

    fn verify(&self) -> anyhow::Result<()> {
        let stat = fstatat(
            &self.parent,
            self.name.as_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )?;
        ensure!(
            same_file(&stat, &self.identity),
            "Git lock replaced; record retained"
        );
        Ok(())
    }
}

impl Lock {
    fn release(mut self) -> anyhow::Result<()> {
        let result = self.cleanup();
        if result.is_err() {
            self.preserve.store(true, Ordering::Release);
        }
        self.released = true;
        result
    }

    fn cleanup(&self) -> anyhow::Result<()> {
        let current = read_regular(self.parent.as_fd(), &self.name)?;
        ensure!(
            current.as_ref().is_some_and(|current| {
                same_file(&current.identity, &self.identity) && current.bytes == self.marker
            }),
            "Git lock replaced or disappeared; recovery .git/{}; record retained",
            self.recovery_name
        );
        unlinkat(&self.parent, self.name.as_str(), UnlinkatFlags::NoRemoveDir)?;
        // Persist disappearance of the conventional lock before removing
        // its journaled hard-link witness. A crash can then always identify
        // any lock path that survived.
        sync_directory(self.parent.as_fd())?;
        unlinkat(
            &self.recovery_fd,
            self.candidate_name.as_str(),
            UnlinkatFlags::NoRemoveDir,
        )?;
        sync_directory(self.recovery_fd.as_fd())?;
        Ok(())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if !self.released && self.cleanup().is_err() {
            self.preserve.store(true, Ordering::Release);
        }
    }
}

/// Reclaim only a lock that is an exact hard link to a recorded Jackin owner
/// file and whose journal lease is no longer held. Unknown Git lock files stay
/// untouched. The old journal remains on disk as evidence if it contains
/// displaced ref or reflog data from an interrupted commit.
fn reclaim_abandoned_lock(
    root: BorrowedFd<'_>,
    parent: BorrowedFd<'_>,
    leaf: &str,
) -> anyhow::Result<bool> {
    let Some(lock) = read_regular(parent, leaf)? else {
        return Ok(false);
    };
    let Ok(text) = std::str::from_utf8(&lock.bytes) else {
        return Ok(false);
    };
    let Some(text) = text.strip_suffix('\n') else {
        return Ok(false);
    };
    let fields: Vec<_> = text.split('\t').collect();
    ensure!(fields.len() == 6, "malformed Jackin lock owner record");
    if fields[0] != "jackin-git-lock-v1" {
        return Ok(false);
    }
    let [
        _,
        journal_name,
        candidate_name,
        recorded_leaf,
        recorded_device,
        recorded_inode,
    ] = fields.as_slice()
    else {
        anyhow::bail!("malformed Jackin lock owner record");
    };
    ensure!(
        journal_name.starts_with("jackin-ref-recovery-")
            && journal_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && candidate_name
                .strip_prefix("lock-")
                .is_some_and(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())),
        "malformed Jackin lock owner identity"
    );
    let parent_identity = fstat(parent)?;
    ensure!(
        *recorded_leaf == leaf
            && *recorded_device == parent_identity.st_dev.to_string()
            && *recorded_inode == parent_identity.st_ino.to_string(),
        "Jackin lock owner does not match this Git lock path"
    );

    let journal = open_directory(root, journal_name, false)
        .context("Jackin lock recovery journal is missing; preserve lock and record")?;
    let owner = read_regular(journal.as_fd(), "owner")?
        .context("Jackin lock recovery journal has no owner record")?;
    ensure!(
        owner.bytes == b"jackin-ref-transaction-v1\n",
        "Jackin lock recovery journal has an invalid owner record"
    );
    let candidate = read_regular(journal.as_fd(), candidate_name)?
        .context("Jackin lock recovery candidate is missing")?;
    ensure!(
        same_file(&candidate.identity, &lock.identity) && candidate.bytes == lock.bytes,
        "Git lock is not the journaled Jackin lock candidate"
    );

    let lease_fd = openat(
        &journal,
        "lease",
        OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    ensure!(
        same_file(
            &fstat(&lease_fd)?,
            &fstatat(&journal, "lease", AtFlags::AT_SYMLINK_NOFOLLOW)?
        ),
        "Jackin recovery journal lease changed"
    );
    let lease: File = lease_fd.into();
    let _abandoned = match Flock::lock(lease, FlockArg::LockExclusiveNonblock) {
        Ok(lease) => lease,
        Err((_, Errno::EWOULDBLOCK)) => {
            anyhow::bail!("Git lock is held by a live Jackin cleanup transaction; record retained")
        }
        Err((_, error)) => return Err(error.into()),
    };

    let current = read_regular(parent, leaf)?;
    ensure!(
        current.as_ref().is_some_and(|current| {
            same_file(&current.identity, &lock.identity) && current.bytes == lock.bytes
        }),
        "Git lock changed during crash recovery; record retained"
    );
    unlinkat(parent, leaf, UnlinkatFlags::NoRemoveDir)?;
    unlinkat(&journal, *candidate_name, UnlinkatFlags::NoRemoveDir)?;
    sync_directory(parent)?;
    sync_directory(journal.as_fd())?;
    Ok(true)
}

/// Holds all checkout HEAD locks and the associated durable journal. Cleanup
/// takes this before any async Git inventory and moves it into the ref
/// transaction, so a checkout cannot change after it has been admitted.
pub(crate) struct CheckoutInventory {
    locks: Vec<Lock>,
    recovery: RecoveryDir,
}

impl CheckoutInventory {
    pub(crate) fn begin(git: BorrowedFd<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            locks: Vec::new(),
            recovery: RecoveryDir::create(git)?,
        })
    }

    pub(crate) fn lock_head(&mut self, parent: BorrowedFd<'_>) -> anyhow::Result<()> {
        self.locks.push(Lock::acquire(
            &mut self.recovery,
            parent,
            "HEAD.lock".into(),
        )?);
        Ok(())
    }

    /// Keep Git's per-worktree `locked` marker present until refs and the
    /// registration are removed. An existing regular marker already prevents
    /// pruning; malformed or linked metadata fails closed.
    pub(crate) fn lock_registration(&mut self, parent: BorrowedFd<'_>) -> anyhow::Result<()> {
        if let Some(existing) = read_regular(parent, "locked")? {
            if !existing.bytes.starts_with(b"jackin-git-lock-v1\t") {
                anyhow::bail!(
                    "worktree registration already has an external lock; record retained"
                );
            }
            if !reclaim_abandoned_lock(self.recovery.root.as_fd(), parent, "locked")? {
                anyhow::bail!("worktree registration lock changed; record retained");
            }
        }
        self.locks
            .push(Lock::acquire(&mut self.recovery, parent, "locked".into())?);
        Ok(())
    }

    pub(crate) fn verify(&self) -> anyhow::Result<()> {
        for lock in &self.locks {
            lock.verify()?;
        }
        Ok(())
    }
}

pub(crate) struct PreparedRefDeletion {
    git: OwnedFd,
    parent: OwnedFd,
    leaf: String,
    full_ref: String,
    oid_len: usize,
    loose: Option<Snapshot>,
    packed: Option<Snapshot>,
    packed_candidate: Option<Snapshot>,
    edges: Vec<Edge>,
    reflog: Option<Reflog>,
    locks: Vec<Lock>,
    recovery: RecoveryDir,
}

pub(crate) struct CommittedRefDeletion {
    locks: Vec<Lock>,
    recovery: RecoveryDir,
}

impl CommittedRefDeletion {
    pub(crate) fn verify(&self) -> anyhow::Result<()> {
        for lock in &self.locks {
            lock.verify()?;
        }
        Ok(())
    }

    /// Release the target worktree's lock files immediately before removing
    /// its registration. The branch ref is already absent while its ref locks
    /// remain held, so it cannot be checked out in the handoff window.
    pub(crate) fn release_registration(
        &mut self,
        registration: BorrowedFd<'_>,
    ) -> anyhow::Result<()> {
        let registration_identity = fstat(registration)?;
        let mut index = 0;
        while index < self.locks.len() {
            let lock = &self.locks[index];
            let parent_identity = fstat(&lock.parent)?;
            if same_file(&registration_identity, &parent_identity)
                && matches!(lock.name.as_str(), "HEAD.lock" | "locked")
            {
                self.locks.remove(index).release()?;
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    /// Release checkout/ref locks only after cleanup removes the target
    /// registration, while its journal and state record still authorize retry.
    pub(crate) fn finish(mut self) -> anyhow::Result<()> {
        while let Some(lock) = self.locks.pop() {
            lock.release()?;
        }
        self.recovery.allow_cleanup();
        Ok(())
    }
}

/// Prepare before deleting the worktree. Both Git locks remain held until
/// commit or drop. Missing parents are created using no-follow descriptor walks.
#[cfg(test)]
pub(crate) fn prepare(
    git: BorrowedFd<'_>,
    full_ref: &str,
    expected_oid: &str,
) -> anyhow::Result<PreparedRefDeletion> {
    prepare_with_checkout(CheckoutInventory::begin(git)?, git, full_ref, expected_oid)
}

pub(crate) fn prepare_with_checkout(
    checkout: CheckoutInventory,
    git: BorrowedFd<'_>,
    full_ref: &str,
    expected_oid: &str,
) -> anyhow::Result<PreparedRefDeletion> {
    validate_ref(full_ref)?;
    ensure!(valid_oid(expected_oid), "malformed expected object ID");
    let expected = (!expected_oid.bytes().all(|b| b == b'0')).then(|| expected_oid.to_owned());
    let parts: Vec<_> = full_ref.split('/').collect();
    let mut recovery = checkout.recovery;
    let mut locks = checkout.locks;
    let mut parent = git.try_clone_to_owned()?;
    let mut edges = Vec::new();
    for name in &parts[..parts.len() - 1] {
        let child = open_directory(parent.as_fd(), name, true)?;
        edges.push(Edge {
            parent,
            name: (*name).into(),
            identity: fstat(&child)?,
        });
        parent = child;
    }
    let leaf = parts[parts.len() - 1].to_owned();
    // Same lock order as Git's files backend: loose, then packed.
    locks.push(Lock::acquire(
        &mut recovery,
        parent.as_fd(),
        format!("{leaf}.lock"),
    )?);
    locks.push(Lock::acquire(
        &mut recovery,
        git,
        "packed-refs.lock".into(),
    )?);
    let loose = read_regular(parent.as_fd(), &leaf)?;
    let loose_oid = loose
        .as_ref()
        .map(|snapshot| loose_oid(&snapshot.bytes))
        .transpose()?;
    let packed = read_regular(git, "packed-refs")?;
    let (packed_oid, packed_without_ref) = packed.as_ref().map_or_else(
        || Ok((None, Vec::new())),
        |snapshot| parse_packed(&snapshot.bytes, full_ref, expected_oid.len()),
    )?;
    let actual = loose_oid.or(packed_oid.as_deref());
    ensure!(
        actual == expected.as_deref(),
        "scratch ref changed; record retained"
    );
    let mut log_parent = git.try_clone_to_owned()?;
    let mut log_missing = false;
    for name in std::iter::once(&"logs").chain(parts[..parts.len() - 1].iter()) {
        match open_directory(log_parent.as_fd(), name, false) {
            Ok(child) => {
                edges.push(Edge {
                    parent: log_parent,
                    name: (*name).into(),
                    identity: fstat(&child)?,
                });
                log_parent = child;
            }
            Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => {
                log_missing = true;
                break;
            }
            Err(error) => return Err(error).context("cannot validate exact scratch reflog"),
        }
    }
    let reflog = if log_missing {
        None
    } else {
        read_regular(log_parent.as_fd(), &leaf)?.map(|snapshot| Reflog {
            parent: log_parent,
            leaf: leaf.clone(),
            snapshot,
        })
    };
    let packed_candidate = if packed_oid.is_some() {
        // Prove filesystem exchange support before deleting the worktree.
        let candidate = prepare_candidate(
            recovery.fd.as_fd(),
            &packed_without_ref,
            packed.as_ref().context("packed snapshot missing")?,
        )
        .with_context(|| "cannot prepare packed transaction; record retained")?;
        recovery.register_file("packed-candidate");
        Some(candidate)
    } else {
        None
    };
    Ok(PreparedRefDeletion {
        git: git.try_clone_to_owned()?,
        parent,
        leaf,
        full_ref: full_ref.into(),
        oid_len: expected_oid.len(),
        loose,
        packed,
        packed_candidate,
        edges,
        reflog,
        locks,
        recovery,
    })
}

impl PreparedRefDeletion {
    pub(crate) fn commit(mut self) -> anyhow::Result<CommittedRefDeletion> {
        for lock in &self.locks {
            lock.verify()?;
        }
        self.verify_snapshots()?;
        // No files to delete: the held locks prove an absent ref remains absent.
        if self.loose.is_none() && self.packed_candidate.is_none() && self.reflog.is_none() {
            return Ok(CommittedRefDeletion {
                locks: self.locks,
                recovery: self.recovery,
            });
        }
        let recovery_fd = self.recovery.fd.as_fd().try_clone_to_owned()?;
        self.recovery.preserve();
        let outcome = self.commit_into(recovery_fd.as_fd());
        if let Err(error) = outcome {
            anyhow::bail!(
                "{error:#}; recovery directory .git/{}; record retained",
                self.recovery.name
            );
        }
        self.recovery.allow_cleanup();
        Ok(CommittedRefDeletion {
            locks: self.locks,
            recovery: self.recovery,
        })
    }

    fn verify_snapshots(&self) -> anyhow::Result<()> {
        for edge in &self.edges {
            edge.verify()?;
        }
        verify_snapshot(self.parent.as_fd(), &self.leaf, self.loose.as_ref())?;
        verify_snapshot(self.git.as_fd(), "packed-refs", self.packed.as_ref())?;
        if let Some(log) = &self.reflog {
            verify_snapshot(log.parent.as_fd(), &log.leaf, Some(&log.snapshot))?;
        } else {
            ensure!(
                current_reflog(self.git.as_fd(), &self.full_ref)?.is_none(),
                "scratch reflog changed; record retained"
            );
        }
        Ok(())
    }

    fn commit_into(&mut self, recovery: BorrowedFd<'_>) -> anyhow::Result<()> {
        if let Some(candidate) = &self.packed_candidate {
            verify_snapshot(recovery, "packed-candidate", Some(candidate))?;
            exchange(
                recovery,
                "packed-candidate",
                self.git.as_fd(),
                "packed-refs",
            )?;
            // Exchange preserves the displaced entry without a missing-table gap.
            // Never remove an unexpected displaced object or overwrite a newer
            // entry by attempting a speculative rollback.
            verify_snapshot(recovery, "packed-candidate", self.packed.as_ref())
                .context("packed refs changed during exchange; displaced entry retained")?;
            verify_snapshot(self.git.as_fd(), "packed-refs", Some(candidate))?;
            sync_directory(self.git.as_fd())?;
            sync_directory(recovery)?;
        }
        if let Some(loose) = &self.loose {
            quarantine(
                self.parent.as_fd(),
                &self.leaf,
                recovery,
                "loose-ref",
                loose,
            )?;
        }
        if let Some(log) = &self.reflog {
            quarantine(
                log.parent.as_fd(),
                &log.leaf,
                recovery,
                "reflog",
                &log.snapshot,
            )?;
        }
        sync_directory(self.parent.as_fd())?;
        if let Some(log) = &self.reflog {
            sync_directory(log.parent.as_fd())?;
        }
        sync_directory(recovery)?;
        for edge in &self.edges {
            edge.verify()?;
        }
        ensure!(
            current_reflog(self.git.as_fd(), &self.full_ref)?.is_none(),
            "scratch reflog reappeared; record retained"
        );
        let loose = read_regular(self.parent.as_fd(), &self.leaf)?;
        let packed = read_regular(self.git.as_fd(), "packed-refs")?;
        let packed_oid = packed
            .as_ref()
            .map(|s| parse_packed(&s.bytes, &self.full_ref, self.oid_len))
            .transpose()?;
        ensure!(
            loose.is_none() && packed_oid.is_none_or(|(oid, _)| oid.is_none()),
            "scratch ref reappeared; record retained"
        );
        if self.loose.is_some() {
            unlinkat(recovery, "loose-ref", UnlinkatFlags::NoRemoveDir)?;
        }
        if self.packed_candidate.is_some() {
            unlinkat(recovery, "packed-candidate", UnlinkatFlags::NoRemoveDir)?;
        }
        if self.reflog.is_some() {
            unlinkat(recovery, "reflog", UnlinkatFlags::NoRemoveDir)?;
        }
        Ok(())
    }
}

fn current_reflog(git: BorrowedFd<'_>, full_ref: &str) -> anyhow::Result<Option<Snapshot>> {
    let parts: Vec<_> = full_ref.split('/').collect();
    let mut parent = git.try_clone_to_owned()?;
    for name in std::iter::once(&"logs").chain(parts[..parts.len() - 1].iter()) {
        match open_directory(parent.as_fd(), name, false) {
            Ok(child) => parent = child,
            Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(error).context("cannot attest scratch reflog absence"),
        }
    }
    read_regular(parent.as_fd(), parts[parts.len() - 1])
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn exchange(
    a: BorrowedFd<'_>,
    a_name: &str,
    b: BorrowedFd<'_>,
    b_name: &str,
) -> anyhow::Result<()> {
    rustix::fs::renameat_with(a, a_name, b, b_name, rustix::fs::RenameFlags::EXCHANGE)
        .context("secure atomic ref exchange unsupported or failed")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn exchange(_: BorrowedFd<'_>, _: &str, _: BorrowedFd<'_>, _: &str) -> anyhow::Result<()> {
    anyhow::bail!("secure atomic ref exchange unsupported on this host")
}

fn prepare_candidate(
    recovery: BorrowedFd<'_>,
    bytes: &[u8],
    packed: &Snapshot,
) -> anyhow::Result<Snapshot> {
    let create = |name: &str| -> anyhow::Result<File> {
        Ok(openat(
            recovery,
            name,
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into())
    };
    let mut candidate = create("packed-candidate")?;
    candidate.write_all(bytes)?;
    fchmod(
        &candidate,
        Mode::from_bits_truncate(packed.identity.st_mode),
    )?;
    candidate.sync_all()?;
    let probe = create("exchange-probe")?;
    exchange(recovery, "packed-candidate", recovery, "exchange-probe")?;
    exchange(recovery, "packed-candidate", recovery, "exchange-probe")?;
    ensure!(
        same_file(
            &fstat(&probe)?,
            &fstatat(recovery, "exchange-probe", AtFlags::AT_SYMLINK_NOFOLLOW)?
        ),
        "exchange probe identity changed"
    );
    unlinkat(recovery, "exchange-probe", UnlinkatFlags::NoRemoveDir)?;
    sync_directory(recovery)?;
    Ok(Snapshot {
        bytes: bytes.to_vec(),
        identity: fstat(&candidate)?,
    })
}

fn sync_directory(fd: BorrowedFd<'_>) -> anyhow::Result<()> {
    File::from(fd.try_clone_to_owned()?)
        .sync_all()
        .context("cannot synchronize ref transaction directory")
}

fn quarantine(
    source: BorrowedFd<'_>,
    name: &str,
    recovery: BorrowedFd<'_>,
    saved: &str,
    expected: &Snapshot,
) -> anyhow::Result<()> {
    renameat(source, name, recovery, saved)?;
    if let Err(error) = verify_snapshot(recovery, saved, Some(expected)) {
        restore(source, name, recovery, saved);
        return Err(error).context("ref changed while moving to recovery");
    }
    Ok(())
}

fn restore(source: BorrowedFd<'_>, name: &str, recovery: BorrowedFd<'_>, saved: &str) {
    // linkat does not follow a symlink source and refuses an occupied destination.
    // If restoration fails, preserve the moved entry in the recovery directory.
    if linkat(recovery, saved, source, name, AtFlags::empty()).is_ok() {
        best_effort_unlink(recovery, saved, UnlinkatFlags::NoRemoveDir);
    }
}

fn recovery_directory(git: BorrowedFd<'_>) -> anyhow::Result<(String, OwnedFd)> {
    for _ in 0..32 {
        let name = format!(
            "jackin-ref-recovery-{}-{}",
            std::process::id(),
            NEXT_TRANSACTION.fetch_add(1, Ordering::Relaxed)
        );
        match mkdirat(git, name.as_str(), Mode::S_IRWXU) {
            Ok(()) => return Ok((name.clone(), open_directory(git, &name, false)?)),
            Err(Errno::EEXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("cannot allocate private ref recovery directory")
}

fn open_directory(parent: BorrowedFd<'_>, name: &str, create: bool) -> anyhow::Result<OwnedFd> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    match openat(parent, name, flags, Mode::empty()) {
        Ok(fd) => Ok(fd),
        Err(Errno::ENOENT) if create => {
            match mkdirat(parent, name, Mode::S_IRWXU) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(error) => return Err(error.into()),
            }
            openat(parent, name, flags, Mode::empty()).map_err(Into::into)
        }
        Err(error) => Err(error.into()),
    }
}

fn read_regular(parent: BorrowedFd<'_>, name: &str) -> anyhow::Result<Option<Snapshot>> {
    let fd = match openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read ref metadata {name}"));
        }
    };
    let identity = fstat(&fd)?;
    ensure!(
        SFlag::from_bits_truncate(identity.st_mode) == SFlag::S_IFREG,
        "ref metadata {name} is not a regular file"
    );
    let mut bytes = Vec::new();
    File::from(fd)
        .take(MAX_METADATA + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA,
        "ref metadata exceeds supported size"
    );
    Ok(Some(Snapshot { bytes, identity }))
}

fn verify_snapshot(
    parent: BorrowedFd<'_>,
    name: &str,
    expected: Option<&Snapshot>,
) -> anyhow::Result<()> {
    let actual = read_regular(parent, name)?;
    ensure!(
        match (actual.as_ref(), expected) {
            (None, None) => true,
            (Some(a), Some(e)) => same_file(&a.identity, &e.identity) && a.bytes == e.bytes,
            _ => false,
        },
        "ref metadata {name} changed; record retained"
    );
    Ok(())
}

fn same_file(a: &FileStat, b: &FileStat) -> bool {
    a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && SFlag::from_bits_truncate(a.st_mode) == SFlag::from_bits_truncate(b.st_mode)
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn loose_oid(bytes: &[u8]) -> anyhow::Result<&str> {
    let text = std::str::from_utf8(bytes)?.trim_end_matches(['\r', '\n']);
    ensure!(
        valid_oid(text) && !text.bytes().all(|b| b == b'0'),
        "symbolic or malformed loose ref; record retained"
    );
    Ok(text)
}

fn validate_ref(name: &str) -> anyhow::Result<()> {
    ensure!(
        name.starts_with("refs/heads/")
            && !name.contains("..")
            && !name.contains("@{")
            && !name
                .bytes()
                .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\".contains(&b))
            && name.split('/').all(|part| !part.is_empty()
                && !part.starts_with('.')
                && !part.ends_with('.')
                && !part.as_bytes().ends_with(b".lock")),
        "invalid exact branch ref name"
    );
    Ok(())
}

fn parse_packed(
    bytes: &[u8],
    target: &str,
    oid_len: usize,
) -> anyhow::Result<(Option<String>, Vec<u8>)> {
    let mut names = std::collections::HashSet::new();
    let mut found = None;
    let mut output = Vec::new();
    let mut previous_removed = false;
    let mut can_peel = false;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let content = line.strip_suffix(b"\n").unwrap_or(line);
        let content = content.strip_suffix(b"\r").unwrap_or(content);
        if content.starts_with(b"#") {
            output.extend_from_slice(line);
            previous_removed = false;
            can_peel = false;
            continue;
        }
        if let Some(oid) = content.strip_prefix(b"^") {
            let oid = std::str::from_utf8(oid)?;
            ensure!(
                can_peel && oid.len() == oid_len && valid_oid(oid),
                "malformed packed peeled ref"
            );
            if !previous_removed {
                output.extend_from_slice(line);
            }
            can_peel = false;
            continue;
        }
        let separator = content
            .iter()
            .position(|byte| *byte == b' ')
            .context("malformed packed ref")?;
        let oid = std::str::from_utf8(&content[..separator])?;
        let name = &content[separator + 1..];
        ensure!(
            oid.len() == oid_len
                && valid_oid(oid)
                && !oid.bytes().all(|b| b == b'0')
                && name.starts_with(b"refs/")
                && !name.iter().any(|byte| *byte <= b' ' || *byte == 127)
                && names.insert(name),
            "malformed or duplicate packed ref"
        );
        previous_removed = name == target.as_bytes();
        can_peel = true;
        if previous_removed {
            found = Some(oid.into());
        } else {
            output.extend_from_slice(line);
        }
    }
    Ok((found, output))
}

#[cfg(test)]
mod tests;
