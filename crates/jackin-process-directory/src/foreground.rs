// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};

static FOREGROUND_LEASE: AtomicBool = AtomicBool::new(false);
static CLEANUP_ERROR: AtomicI32 = AtomicI32::new(0);
static NATIVE_SPAWN: Mutex<()> = Mutex::new(());

/// Serializes managed native child creation with descriptor preparation.
///
/// Hold this guard through preparation and the native spawn, before acquiring
/// the child registry. Never acquire it in a `pre_exec` callback or retain it
/// across an await. Every managed native spawn must use the same authority.
#[derive(Debug)]
pub struct NativeSpawnGuard {
    _serialized: MutexGuard<'static, ()>,
}

/// Acquire the authority for a synchronous managed native spawn operation.
#[must_use]
pub fn native_spawn_guard() -> NativeSpawnGuard {
    NativeSpawnGuard {
        _serialized: NATIVE_SPAWN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    }
}

fn notice_pipe(spawn: &NativeSpawnGuard) -> io::Result<(OwnedFd, OwnedFd)> {
    notice_pipe_after_create(spawn, |_, _| {})
}

fn notice_pipe_after_create(
    _spawn: &NativeSpawnGuard,
    after_create: impl FnOnce(&OwnedFd, &OwnedFd),
) -> io::Result<(OwnedFd, OwnedFd)> {
    #[cfg(not(target_vendor = "apple"))]
    let (original_read, original_write) = rustix::pipe::pipe_with(
        rustix::pipe::PipeFlags::CLOEXEC | rustix::pipe::PipeFlags::NONBLOCK,
    )?;
    // Darwin has no pipe2. The shared managed-spawn authority excludes forks
    // throughout this temporary inheritable-descriptor window. Both originals
    // are closed before the authority can be released, including error paths.
    #[cfg(target_vendor = "apple")]
    let (original_read, original_write) = rustix::pipe::pipe()?;
    after_create(&original_read, &original_write);
    let read = rustix::io::fcntl_dupfd_cloexec(&original_read, 3)?;
    let write = rustix::io::fcntl_dupfd_cloexec(&original_write, 3)?;
    #[cfg(target_vendor = "apple")]
    for descriptor in [&read, &write] {
        let flags = rustix::fs::fcntl_getfl(descriptor)?;
        rustix::fs::fcntl_setfl(descriptor, flags | rustix::fs::OFlags::NONBLOCK)?;
    }
    drop(original_read);
    drop(original_write);
    Ok((read, write))
}

#[cfg(test)]
mod spawn_tests;

/// Typed terminal restoration failure containing only an operating-system code.
#[derive(Debug)]
pub struct ForegroundRestoreError {
    errno: i32,
}

impl std::fmt::Display for ForegroundRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("terminal restoration failed")
    }
}

impl std::error::Error for ForegroundRestoreError {}

fn restoration_error(error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        ForegroundRestoreError {
            errno: error
                .raw_os_error()
                .unwrap_or(nix::errno::Errno::EIO as i32),
        },
    )
}

/// Return a retained foreground restoration failure.
/// Later setup refuses to proceed until the same owner restores successfully.
#[must_use]
pub fn foreground_cleanup_error() -> Option<io::Error> {
    let errno = CLEANUP_ERROR.load(Ordering::Acquire);
    (errno != 0).then(|| restoration_error(io::Error::from_raw_os_error(errno)))
}

/// Restore the invoking process group's controlling terminal after a child.
///
/// One process-local lease excludes competing foreground transfers without
/// holding a mutex across async waits. The original group must be the caller's
/// group; background callers cannot take a terminal from another job.
#[derive(Debug)]
pub struct ForegroundGuard {
    terminal: Arc<OwnedFd>,
    original_group: rustix::process::Pid,
    original_modes: rustix::termios::Termios,
    child_notice: OwnedFd,
    child_group: Option<rustix::process::Pid>,
    restore_gate: Option<Arc<Mutex<()>>>,
    armed: bool,
}

impl ForegroundGuard {
    /// Observe a reserved child exit without releasing its kernel PID identity.
    /// The caller must own the child and prevent every other reaper from
    /// consuming its status until the final process-group signal is sent.
    ///
    /// # Errors
    /// Returns invalid PID or operating-system observation errors.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn child_exited_without_reaping(pid: u32) -> io::Result<bool> {
        let pid = i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid child PID"))?;
        let options = rustix::process::WaitIdOptions::EXITED
            | rustix::process::WaitIdOptions::NOHANG
            | rustix::process::WaitIdOptions::NOWAIT;
        rustix::process::waitid(rustix::process::WaitId::Pid(pid), options)
            .map(|status| status.is_some())
            .map_err(io::Error::from)
    }

    /// Observe a child without reaping on supported Linux/macOS hosts.
    /// # Errors
    /// Other Unix platforms have no reviewed non-reaping lifecycle contract.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn child_exited_without_reaping(_pid: u32) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "foreground child observation is unsupported on this platform",
        ))
    }

    /// Configure a private child group and move it to the terminal before exec.
    ///
    /// Only use for commands inheriting stdin. A non-terminal stdin needs no
    /// restoration guard, but still receives a private process group. The
    /// retained descriptor is duplicated above fd 2 with CLOEXEC so child stdio
    /// setup cannot replace it. Keep the resulting foreground guard through
    /// spawn and every wait; dropping it also restores after a failed exec.
    /// The native-spawn authority must remain held from this call through
    /// `Command::spawn`; acquire it before the child registry.
    ///
    /// # Errors
    /// Rejects background callers, competing transfers, retained cleanup
    /// failures, and terminal/descriptor errors. Child setup errors propagate
    /// through `Command::spawn`.
    #[expect(
        unsafe_code,
        reason = "audited pre_exec boundary invokes only async-signal-safe process group, signal mask and terminal syscalls"
    )]
    pub fn prepare(command: &mut Command, spawn: &NativeSpawnGuard) -> io::Result<Option<Self>> {
        if let Some(error) = foreground_cleanup_error() {
            return Err(error);
        }
        let stdin = io::stdin();
        match rustix::termios::tcgetattr(&stdin) {
            Ok(_) => {}
            Err(rustix::io::Errno::NOTTY) => {
                command.process_group(0);
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        }
        if FOREGROUND_LEASE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "another child owns terminal foreground transfer",
            ));
        }
        // A prior owner's Drop may have recorded failure between our first
        // error check and its lease release. Recheck under the new lease.
        if let Some(error) = foreground_cleanup_error() {
            FOREGROUND_LEASE.store(false, Ordering::Release);
            return Err(error);
        }
        let prepared = (|| {
            let terminal = Arc::new(rustix::io::fcntl_dupfd_cloexec(&stdin, 3)?);
            let original_modes = rustix::termios::tcgetattr(terminal.as_ref())?;
            let original_group = rustix::termios::tcgetpgrp(terminal.as_ref())?;
            if original_group != rustix::process::getpgrp() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "caller does not own terminal foreground",
                ));
            }
            let (notice_read, notice_write) = notice_pipe(spawn)?;
            Ok((
                Self {
                    terminal,
                    original_group,
                    original_modes,
                    child_notice: notice_read,
                    child_group: None,
                    restore_gate: None,
                    armed: true,
                },
                notice_write,
            ))
        })();
        let (guard, child_notice) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                FOREGROUND_LEASE.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let child_terminal = Arc::clone(&guard.terminal);
        let original_group = guard.original_group;
        let mut blocked = SigSet::empty();
        blocked.add(Signal::SIGTTOU);
        let empty_mask = SigSet::empty();
        // SAFETY: All descriptors, signal sets and captures are prepared above.
        // The child callback only performs setpgid, getpid, pthread_sigmask,
        // tcgetpgrp, write and tcsetpgrp, converts errno without allocation,
        // and copies fixed-size signal sets and PID bytes.
        // It does not lock, allocate, touch Arc counts, change global signal
        // dispositions, or run captured destructors. The duplicated CLOEXEC
        // descriptor remains valid even after child standard-stream setup.
        unsafe {
            command.pre_exec(move || {
                rustix::process::setpgid(None, None)?;
                let mut previous = empty_mask;
                pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&blocked), Some(&mut previous))?;
                let transfer = (|| {
                    let child_group = rustix::process::getpid();
                    // Do not publish restore intent unless this child still
                    // owns the foreground group it is authorized to transfer.
                    if rustix::termios::tcgetpgrp(child_terminal.as_ref())? != original_group {
                        return Err(io::Error::from_raw_os_error(
                            nix::errno::Errno::EPERM as i32,
                        ));
                    }
                    let notice = child_group.as_raw_pid().to_ne_bytes();
                    if rustix::io::write(&child_notice, &notice)? != notice.len() {
                        return Err(io::Error::from_raw_os_error(nix::errno::Errno::EIO as i32));
                    }
                    rustix::termios::tcsetpgrp(child_terminal.as_ref(), child_group)
                        .map_err(io::Error::from)
                })();
                let unmask = pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&previous), None)
                    .map_err(io::Error::from);
                transfer.and(unmask)
            });
        }
        Ok(Some(guard))
    }

    /// Serialize later restoration with the caller's terminal ownership gate.
    /// Attach after the synchronous setup gate is released. Initial spawn-error
    /// rollback runs inside that gate; a failed rollback's retry owner attaches
    /// the gate before attempting later restoration.
    pub fn set_restore_gate(&mut self, gate: Arc<Mutex<()>>) {
        self.restore_gate = Some(gate);
    }

    /// Restore foreground ownership and release the transfer lease.
    ///
    /// # Errors
    /// Returns terminal or calling-thread signal-mask failures. Ownership stays
    /// armed on failure so destruction retries and retains an infallible error.
    pub fn restore(&mut self) -> io::Result<()> {
        self.restore_inner().map_err(|error| {
            let errno = error
                .raw_os_error()
                .unwrap_or(nix::errno::Errno::EIO as i32);
            let _retained =
                CLEANUP_ERROR.compare_exchange(0, errno, Ordering::AcqRel, Ordering::Acquire);
            restoration_error(error)
        })
    }

    fn restore_inner(&mut self) -> io::Result<()> {
        if !self.armed {
            return Ok(());
        }
        let gate = self.restore_gate.clone();
        let _gate = gate.as_ref().map(|gate| {
            gate.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        });
        if self.child_group.is_none() {
            let mut notice = [0u8; 4];
            match rustix::io::read(&self.child_notice, &mut notice) {
                Ok(4) => {
                    let pid = i32::from_ne_bytes(notice);
                    if pid <= 0 {
                        return Err(io::Error::from_raw_os_error(nix::errno::Errno::EIO as i32));
                    }
                    self.child_group = rustix::process::Pid::from_raw(pid);
                }
                Ok(0) | Err(rustix::io::Errno::AGAIN) => {
                    // No notice means the child never attempted a transfer.
                    // In particular, failed authorization must not restore
                    // over the unrelated job which caused its rejection.
                    self.armed = false;
                    CLEANUP_ERROR.store(0, Ordering::Release);
                    FOREGROUND_LEASE.store(false, Ordering::Release);
                    return Ok(());
                }
                Ok(_) => {
                    return Err(io::Error::from_raw_os_error(nix::errno::Errno::EIO as i32));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let mut blocked = SigSet::empty();
        blocked.add(Signal::SIGTTOU);
        let mut previous = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&blocked), Some(&mut previous))?;
        let authorized = rustix::termios::tcgetpgrp(self.terminal.as_ref())
            .map_err(io::Error::from)
            .and_then(|current| {
                if current == self.original_group || Some(current) == self.child_group {
                    Ok(())
                } else {
                    Err(io::Error::from_raw_os_error(
                        nix::errno::Errno::EPERM as i32,
                    ))
                }
            });
        let restored = authorized.and_then(|()| {
            rustix::termios::tcsetpgrp(self.terminal.as_ref(), self.original_group)
                .map_err(io::Error::from)
        });
        // A forcibly terminated interactive client cannot undo its raw mode.
        // Restore exactly the caller's prior settings before readers resume.
        let modes = if restored.is_ok() {
            rustix::termios::tcsetattr(
                self.terminal.as_ref(),
                rustix::termios::OptionalActions::Now,
                &self.original_modes,
            )
            .map_err(io::Error::from)
        } else {
            Ok(())
        };
        let unmask = pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&previous), None)
            .map_err(io::Error::from);
        restored.and(modes).and(unmask)?;
        self.armed = false;
        // Only the still-owning guard may clear a failure after it proves
        // complete restoration. A new spawn never acknowledges old failures.
        CLEANUP_ERROR.store(0, Ordering::Release);
        FOREGROUND_LEASE.store(false, Ordering::Release);
        Ok(())
    }
}

impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            let errno = error
                .get_ref()
                .and_then(|error| error.downcast_ref::<ForegroundRestoreError>())
                .map_or(nix::errno::Errno::EIO as i32, |error| error.errno);
            let _retained =
                CLEANUP_ERROR.compare_exchange(0, errno, Ordering::AcqRel, Ordering::Acquire);
            FOREGROUND_LEASE.store(false, Ordering::Release);
        }
    }
}
