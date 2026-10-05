// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Registered child handles. Native handles never escape the ownership guard.

use std::io;
use std::process::{ExitStatus, Output};

#[cfg(unix)]
use crate::child_ownership::ChildRegistration;

/// Synchronous child whose exit status is reserved for its owner.
#[derive(Debug)]
pub struct SyncChild {
    child: Option<std::process::Child>,
    #[cfg(unix)]
    registration: Option<ChildRegistration>,
    /// Writable child standard input.
    pub stdin: Option<std::process::ChildStdin>,
    /// Readable child standard output.
    pub stdout: Option<std::process::ChildStdout>,
    /// Readable child standard error.
    pub stderr: Option<std::process::ChildStderr>,
    terminate_on_drop: bool,
    reaped: bool,
    pid: u32,
    #[cfg(unix)]
    group: Option<nix::unistd::Pid>,
}

impl SyncChild {
    pub(crate) fn new(
        mut child: std::process::Child,
        #[cfg(unix)] registration: Option<ChildRegistration>,
        owns_group: bool,
    ) -> Self {
        #[cfg(not(unix))]
        let _owns_group = owns_group;
        Self {
            #[cfg(unix)]
            group: owns_group
                .then(|| child.id())
                .and_then(|pid| i32::try_from(pid).ok())
                .map(nix::unistd::Pid::from_raw),
            pid: child.id(),
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            child: Some(child),
            #[cfg(unix)]
            registration,
            terminate_on_drop: true,
            reaped: false,
        }
    }

    /// Child process identifier.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Wait and release the exit-status reservation.
    /// # Errors
    /// Returns operating-system wait failures.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.stdin = None;
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("child ownership transferred"))?
            .wait()?;
        self.reaped = true;
        #[cfg(unix)]
        {
            self.group = None;
            self.registration = None;
        }
        Ok(status)
    }

    /// Poll exit and release the reservation only after reaping.
    /// # Errors
    /// Returns operating-system wait failures.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("child ownership transferred"))?
            .try_wait()?;
        if status.is_some() {
            self.reaped = true;
            #[cfg(unix)]
            {
                self.group = None;
                self.registration = None;
            }
        }
        Ok(status)
    }

    /// Terminate the child. Waiting remains the owner's responsibility.
    /// # Errors
    /// Returns operating-system signal failures.
    pub fn kill(&mut self) -> io::Result<()> {
        if self.reaped {
            return Ok(());
        }
        #[cfg(unix)]
        let group_result = kill_process_group(self.group);
        #[cfg(not(unix))]
        let group_result = Ok(());
        let child_result = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("child ownership transferred"))?
            .kill();
        group_result.and(child_result)
    }

    /// Capture remaining output and reap the child.
    /// # Errors
    /// Returns pipe-read or wait failures.
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        self.stdin = None;
        let stdout = self.stdout.take();
        let stderr = self.stderr.take();
        // Keep the native child inside its guard even when a pipe read or a
        // waiter fails. Unwinding/error then retains the cleanup owner.
        std::thread::scope(|scope| {
            let stdout = scope.spawn(move || read_sync_pipe(stdout));
            let stderr = scope.spawn(move || read_sync_pipe(stderr));
            let status = self.wait();
            if status.is_err() {
                drop(self.kill());
            }
            let stdout = stdout
                .join()
                .map_err(|_| io::Error::other("child stdout reader panicked"))?;
            let stderr = stderr
                .join()
                .map_err(|_| io::Error::other("child stderr reader panicked"))?;
            Ok(Output {
                status: status?,
                stdout: stdout?,
                stderr: stderr?,
            })
        })
    }

    /// Keep the process alive and hand its eventual reap to an OS-thread owner.
    pub fn detach(mut self) {
        self.terminate_on_drop = false;
    }
}

impl Drop for SyncChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if self.reaped {
            return;
        }
        if self.terminate_on_drop {
            #[cfg(unix)]
            drop(kill_process_group(self.group));
            drop(child.kill());
        }
        let pending = std::sync::Arc::new(std::sync::Mutex::new(Some(SyncReap {
            child,
            #[cfg(unix)]
            registration: self.registration.take(),
        })));
        let waiter = std::sync::Arc::clone(&pending);
        // Retain the job outside the spawning closure so allocation failure
        // cannot discard either the native child or its registration.
        let started = std::thread::Builder::new()
            .name("child-reap".into())
            .spawn(move || reap_sync_job(&waiter));
        if started.is_err() {
            // Thread exhaustion leaves no asynchronous owner available.
            // Synchronously reap rather than lose a detached/live child's
            // only wait handle. This exceptional fallback can block until exit.
            reap_sync_job(&pending);
        }
    }
}

fn read_sync_pipe(pipe: Option<impl io::Read>) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(&mut output)?;
    }
    Ok(output)
}

struct SyncReap {
    child: std::process::Child,
    #[cfg(unix)]
    registration: Option<ChildRegistration>,
}

fn reap_sync_job(job: &std::sync::Mutex<Option<SyncReap>>) {
    let pending = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(mut pending) = pending {
        while pending.child.wait().is_err() {
            cleanup_pause();
        }
        #[cfg(unix)]
        drop(pending.registration);
    }
}

/// Async child whose exit status is reserved for its owner.
#[derive(Debug)]
pub struct AsyncChild {
    child: Option<tokio::process::Child>,
    #[cfg(unix)]
    registration: Option<ChildRegistration>,
    /// Writable child standard input.
    pub stdin: Option<tokio::process::ChildStdin>,
    /// Readable child standard output.
    pub stdout: Option<tokio::process::ChildStdout>,
    /// Readable child standard error.
    pub stderr: Option<tokio::process::ChildStderr>,
    terminate_on_drop: bool,
    #[cfg(unix)]
    group: Option<nix::unistd::Pid>,
}

impl AsyncChild {
    pub(crate) fn new(
        mut child: tokio::process::Child,
        #[cfg(unix)] registration: Option<ChildRegistration>,
        owns_group: bool,
    ) -> Self {
        #[cfg(not(unix))]
        let _owns_group = owns_group;
        Self {
            #[cfg(unix)]
            group: owns_group
                .then(|| child.id())
                .flatten()
                .and_then(|pid| i32::try_from(pid).ok())
                .map(nix::unistd::Pid::from_raw),
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            child: Some(child),
            terminate_on_drop: true,
            #[cfg(unix)]
            registration,
        }
    }

    /// Child process identifier, absent after a completed wait.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().and_then(tokio::process::Child::id)
    }

    /// Keep the process alive and hand its eventual reap to an OS-thread owner.
    pub fn detach(mut self) {
        self.terminate_on_drop = false;
    }

    fn native_mut(&mut self) -> io::Result<&mut tokio::process::Child> {
        self.child
            .as_mut()
            .ok_or_else(|| io::Error::other("child ownership transferred"))
    }

    /// Wait and release the exit-status and process-group reservation.
    /// Drain taken capture pipes before waiting so descendants cannot outlive
    /// the reserved group identifier while they still retain those pipes.
    /// # Errors
    /// Returns operating-system wait failures.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.stdin = None;
        let status = self.native_mut()?.wait().await?;
        #[cfg(unix)]
        {
            self.group = None;
            self.registration = None;
        }
        Ok(status)
    }

    /// Poll exit and release the reservation only after reaping.
    /// # Errors
    /// Returns operating-system wait failures.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.native_mut()?.try_wait()?;
        if status.is_some() {
            #[cfg(unix)]
            {
                self.group = None;
                self.registration = None;
            }
        }
        Ok(status)
    }

    /// Send the termination signal while the PID remains reserved.
    /// # Errors
    /// Returns operating-system signal failures.
    pub fn start_kill(&mut self) -> io::Result<()> {
        let group_result = self.kill_group();
        let child_result = self.native_mut()?.start_kill();
        group_result.and(child_result)
    }

    fn kill_group(&self) -> io::Result<()> {
        #[cfg(unix)]
        return kill_process_group(self.group);
        #[cfg(not(unix))]
        Ok(())
    }

    /// Terminate and reap the child.
    /// # Errors
    /// Returns signal or wait failures.
    pub async fn kill(&mut self) -> io::Result<()> {
        let signal_result = self.start_kill();
        // Keep cleanup bounded even if the kernel cannot finish a killed task.
        tokio::time::timeout(std::time::Duration::from_secs(1), self.wait())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "child did not reap within cleanup deadline",
                )
            })??;
        signal_result
    }

    /// Capture remaining output and reap the child.
    /// # Errors
    /// Returns pipe-read or wait failures.
    pub async fn wait_with_output(mut self) -> io::Result<Output> {
        use tokio::io::AsyncReadExt;
        self.stdin = None;
        let mut stdout = self.stdout.take();
        let mut stderr = self.stderr.take();
        let read_stdout = async {
            let mut bytes = Vec::new();
            if let Some(stream) = &mut stdout {
                stream.read_to_end(&mut bytes).await?;
            }
            Ok::<_, io::Error>(bytes)
        };
        let read_stderr = async {
            let mut bytes = Vec::new();
            if let Some(stream) = &mut stderr {
                stream.read_to_end(&mut bytes).await?;
            }
            Ok::<_, io::Error>(bytes)
        };
        // The native child stays in self across all awaits so cancellation
        // still sends its final signal before releasing its reservation.
        let (stdout, stderr) = tokio::try_join!(read_stdout, read_stderr)?;
        let status = self.wait().await?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

impl Drop for AsyncChild {
    fn drop(&mut self) {
        if self.terminate_on_drop && self.id().is_some() {
            drop(self.start_kill());
        }
        let Some(child) = self.child.take() else {
            return;
        };
        if child.id().is_none() {
            return;
        }
        let pending = std::sync::Arc::new(std::sync::Mutex::new(Some(AsyncReap {
            child,
            #[cfg(unix)]
            registration: self.registration.take(),
        })));
        let waiter = std::sync::Arc::clone(&pending);
        let started = std::thread::Builder::new()
            .name("async-child-reap".into())
            .spawn(move || reap_async_job(&waiter));
        if started.is_err() {
            reap_async_job(&pending);
        }
    }
}

struct AsyncReap {
    child: tokio::process::Child,
    #[cfg(unix)]
    registration: Option<ChildRegistration>,
}

fn reap_async_job(job: &std::sync::Mutex<Option<AsyncReap>>) {
    let pending = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(mut pending) = pending {
        // Cache the native status before dropping Tokio's handle. An uncached
        // orphan waiter competing with PID1 could later waitpid a reused PID.
        while !matches!(pending.child.try_wait(), Ok(Some(_))) {
            cleanup_pause();
        }
        #[cfg(unix)]
        drop(pending.registration);
    }
}

#[cfg(unix)]
fn kill_process_group(group: Option<nix::unistd::Pid>) -> io::Result<()> {
    if let Some(group) = group {
        match nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
            Err(error) => return Err(io::Error::from_raw_os_error(error as i32)),
        }
    }
    Ok(())
}

/// Synchronous private-group owner with explicit completion or termination.
#[derive(Debug)]
pub struct GroupSyncChild {
    inner: SyncChild,
    /// Writable child standard input.
    pub stdin: Option<std::process::ChildStdin>,
    /// Readable child standard output.
    pub stdout: Option<std::process::ChildStdout>,
    /// Readable child standard error.
    pub stderr: Option<std::process::ChildStderr>,
}

impl GroupSyncChild {
    pub(crate) fn new(mut inner: SyncChild) -> Self {
        Self {
            stdin: inner.stdin.take(),
            stdout: inner.stdout.take(),
            stderr: inner.stderr.take(),
            inner,
        }
    }

    /// Reserved leader process identifier.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    /// Accept completion after captured pipe EOF and consume group ownership.
    /// # Errors
    /// Returns operating-system wait failures.
    pub fn finish(mut self) -> io::Result<ExitStatus> {
        self.stdin = None;
        self.inner.wait()
    }

    /// Kill the entire group before reaping its reserved leader, within 1s.
    /// # Errors
    /// Returns signal, wait, or cleanup-deadline failures.
    pub fn kill_and_reap(&mut self) -> io::Result<ExitStatus> {
        self.stdin = None;
        let signal_result = self.inner.kill();
        let started = std::time::Instant::now();
        loop {
            if let Some(status) = self.inner.try_wait()? {
                signal_result?;
                return Ok(status);
            }
            if started.elapsed() >= std::time::Duration::from_secs(1) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "child did not reap within cleanup deadline",
                ));
            }
            #[expect(
                clippy::disallowed_methods,
                reason = "synchronous child cleanup owns its caller's blocking execution boundary"
            )]
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

pub(crate) struct ForegroundScope(Option<Box<dyn FnOnce() + Send>>);

impl ForegroundScope {
    pub(crate) fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl std::fmt::Debug for ForegroundScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ForegroundScope")
    }
}

impl Drop for ForegroundScope {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

/// Child owning a private process group until explicit completion.
///
/// Captured readers must reach EOF before finish. Foreground completion first
/// observes the leader exit without reaping, terminates residual group members
/// while its PID remains reserved, and confirms group absence before restoring
/// the terminal. Other normal completion accepts background independent work.
#[derive(Debug)]
pub struct GroupChild {
    inner: Option<AsyncChild>,
    /// Writable child standard input.
    pub stdin: Option<tokio::process::ChildStdin>,
    /// Readable child standard output.
    pub stdout: Option<tokio::process::ChildStdout>,
    /// Readable child standard error.
    pub stderr: Option<tokio::process::ChildStderr>,
    #[cfg(unix)]
    foreground: Option<jackin_process_directory::ForegroundGuard>,
    foreground_scope: Option<ForegroundScope>,
    #[cfg(unix)]
    retiring_group: Option<nix::unistd::Pid>,
}

impl GroupChild {
    pub(crate) fn new(mut inner: AsyncChild) -> Self {
        Self {
            stdin: inner.stdin.take(),
            stdout: inner.stdout.take(),
            stderr: inner.stderr.take(),
            #[cfg(unix)]
            retiring_group: inner.group,
            inner: Some(inner),
            #[cfg(unix)]
            foreground: None,
            foreground_scope: None,
        }
    }

    #[cfg(unix)]
    pub(crate) fn with_foreground(
        mut self,
        foreground: Option<jackin_process_directory::ForegroundGuard>,
    ) -> Self {
        self.foreground = foreground;
        self
    }

    pub(crate) fn with_foreground_scope(mut self, scope: ForegroundScope) -> Self {
        let previous = self.foreground_scope.take();
        self.foreground_scope = Some(ForegroundScope::new(move || {
            drop(previous);
            drop(scope);
        }));
        self
    }

    fn terminal_owned(&self) -> bool {
        #[cfg(unix)]
        if self.foreground.is_some() {
            return true;
        }
        self.foreground_scope.is_some()
    }

    fn inner_mut(&mut self) -> io::Result<&mut AsyncChild> {
        self.inner
            .as_mut()
            .ok_or_else(|| io::Error::other("child ownership transferred"))
    }

    fn restore_foreground(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        if let Some(foreground) = self.foreground.as_mut() {
            foreground.restore()?;
            self.foreground = None;
        }
        Ok(())
    }

    async fn finish_inner(&mut self) -> io::Result<ExitStatus> {
        self.stdin = None;
        #[cfg(unix)]
        if self.terminal_owned() {
            // Never reap before the final real group signal: the zombie leader
            // reserves the PGID against reuse while residual writers die.
            while let Some(pid) = self.id() {
                if jackin_process_directory::ForegroundGuard::child_exited_without_reaping(pid)? {
                    self.inner_mut()?.kill_group()?;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
        let status = self.inner_mut()?.wait().await?;
        if self.terminal_owned() {
            self.wait_for_group_absence().await?;
            self.restore_foreground()?;
        }
        Ok(status)
    }

    async fn wait_for_group_absence(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        while !process_group_absent(self.retiring_group)? {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        #[cfg(unix)]
        {
            self.retiring_group = None;
        }
        Ok(())
    }

    /// Child identifier, absent after the cleanup owner has reaped it.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.inner.as_ref().and_then(AsyncChild::id)
    }

    /// Accept completion after capture EOF and consume group ownership.
    /// # Errors
    /// Returns wait, group-observation, or terminal-restoration failures.
    pub async fn finish(mut self) -> io::Result<ExitStatus> {
        self.finish_inner().await
    }

    /// Finish within the remaining deadline or terminate/reap before timeout.
    /// `None` means deadline expiration followed by confirmed cleanup.
    /// # Errors
    /// Returns signal, wait, group-observation, restoration, or cleanup-deadline failures.
    pub async fn finish_with_timeout(
        mut self,
        timeout: std::time::Duration,
    ) -> io::Result<Option<ExitStatus>> {
        match tokio::time::timeout(timeout, self.finish_inner()).await {
            Ok(result) => result.map(Some),
            Err(_elapsed) => {
                self.kill_and_reap().await?;
                Ok(None)
            }
        }
    }

    /// Terminate the group and confirm termination before terminal restoration.
    /// # Errors
    /// Returns signal, wait, observation, restoration, or cleanup-deadline failures.
    /// Failure retains the terminal token in a dedicated cleanup owner on drop.
    pub async fn kill_and_reap(&mut self) -> io::Result<()> {
        self.stdin = None;
        self.inner_mut()?.kill().await?;
        if self.terminal_owned() {
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                self.wait_for_group_absence(),
            )
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "process group still owns terminal")
            })??;
            self.restore_foreground()?;
            drop(self.foreground_scope.take());
        }
        Ok(())
    }
}

impl Drop for GroupChild {
    fn drop(&mut self) {
        let Some(mut inner) = self.inner.take() else {
            return;
        };
        if !self.terminal_owned() {
            drop(inner);
            return;
        }
        #[cfg(unix)]
        let restored = self.foreground.is_none() && self.retiring_group.is_none();
        #[cfg(not(unix))]
        let restored = true;
        if inner.id().is_none() && restored {
            drop(inner);
            drop(self.foreground_scope.take());
            return;
        }
        // Send real signals only while the original leader is still reserved.
        // After its reap, cleanup probes the PGID with signal 0 only. Numeric
        // reuse may conservatively delay release but cannot kill a replacement.
        if inner.id().is_some() {
            drop(inner.start_kill());
        }
        let pending = std::sync::Arc::new(std::sync::Mutex::new(Some(GroupCleanup {
            inner,
            #[cfg(unix)]
            foreground: self.foreground.take(),
            scope: self.foreground_scope.take(),
            #[cfg(unix)]
            group: self.retiring_group.take(),
        })));
        let waiter = std::sync::Arc::clone(&pending);
        let started = std::thread::Builder::new()
            .name("foreground-reap".into())
            .spawn(move || run_group_cleanup(&waiter));
        if started.is_err() {
            // Thread exhaustion cannot authorize terminal release. Retain all
            // resources and synchronously wait rather than claim restoration.
            run_group_cleanup(&pending);
        }
    }
}

struct GroupCleanup {
    inner: AsyncChild,
    #[cfg(unix)]
    foreground: Option<jackin_process_directory::ForegroundGuard>,
    // Kept until kernel completion and successful physical restoration.
    scope: Option<ForegroundScope>,
    #[cfg(unix)]
    group: Option<nix::unistd::Pid>,
}

fn run_group_cleanup(job: &std::sync::Mutex<Option<GroupCleanup>>) {
    let pending = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(mut pending) = pending {
        // Synchronous Tokio Child::try_wait needs no runtime and updates its
        // native cached status, preventing an orphan queue with a stale PID.
        while !matches!(pending.inner.try_wait(), Ok(Some(_))) {
            cleanup_pause();
        }
        #[cfg(unix)]
        while !matches!(process_group_absent(pending.group), Ok(true)) {
            cleanup_pause();
        }
        #[cfg(unix)]
        if let Some(foreground) = pending.foreground.as_mut() {
            while foreground.restore().is_err() {
                cleanup_pause();
            }
        }
        // Explicit release happens after restored state; permanent kernel or
        // restore failure retains the scope and keeps terminal admission busy.
        drop(pending.scope.take());
    }
}

fn cleanup_pause() {
    #[expect(
        clippy::disallowed_methods,
        reason = "cleanup owns a dedicated OS thread and never relies on an async runtime"
    )]
    std::thread::sleep(std::time::Duration::from_millis(10));
}

#[cfg(unix)]
fn process_group_absent(group: Option<nix::unistd::Pid>) -> io::Result<bool> {
    let Some(group) = group else {
        return Ok(true);
    };
    match nix::sys::signal::killpg(group, None) {
        Err(nix::errno::Errno::ESRCH) => Ok(true),
        Ok(()) | Err(nix::errno::Errno::EPERM) => Ok(false),
        Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
    }
}

#[cfg(unix)]
type ForegroundRestoreJob =
    std::sync::Mutex<Option<(jackin_process_directory::ForegroundGuard, ForegroundScope)>>;

#[cfg(unix)]
pub(crate) fn retain_failed_foreground_restore(
    foreground: jackin_process_directory::ForegroundGuard,
    scope: ForegroundScope,
    gate: std::sync::Arc<std::sync::Mutex<()>>,
) {
    let pending = std::sync::Arc::new(std::sync::Mutex::new(Some((foreground, scope))));
    let worker = std::sync::Arc::clone(&pending);
    let started = std::thread::Builder::new()
        .name("foreground-restore".into())
        .spawn({
            let gate = std::sync::Arc::clone(&gate);
            move || retry_failed_foreground_restore(&worker, gate)
        });
    if started.is_err() {
        // The synchronous setup gate has already been released. Thread
        // exhaustion retains ownership and retries under the same TTY gate.
        retry_failed_foreground_restore(&pending, gate);
    }
}

#[cfg(unix)]
fn retry_failed_foreground_restore(
    job: &ForegroundRestoreJob,
    gate: std::sync::Arc<std::sync::Mutex<()>>,
) {
    let pending = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some((mut foreground, scope)) = pending {
        foreground.set_restore_gate(gate);
        while foreground.restore().is_err() {
            cleanup_pause();
        }
        drop(scope);
    }
}
