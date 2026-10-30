// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use super::{SpoolFailure, SpoolFailureKind, SpoolRecovery};
use crate::ChildRegistration;

const CLEANUP_WINDOW: Duration = Duration::from_secs(4);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[expect(
    clippy::disallowed_methods,
    reason = "finite synchronous process supervision sleeps only to its existing cleanup deadline"
)]
pub(super) fn process_supervision_poll(deadline: Option<Instant>) {
    let delay = deadline.map_or(POLL_INTERVAL, |deadline| {
        POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()))
    });
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupPhase {
    AnchorHeld,
    Finalizing,
    DestructiveClosed,
    ReapedOrUncertain,
    Finished,
}

#[derive(Debug)]
pub(super) struct SpawnError {
    pub(super) unsupported: bool,
    pub(super) message: String,
    pub(super) group: Option<Box<OwnedGroup>>,
}

#[derive(Debug)]
enum Spawned {
    AnchorFailed(io::Error),
    TargetFailed {
        anchor: Child,
        anchor_registration: ChildRegistration,
        error: io::Error,
    },
    Started {
        anchor: Child,
        anchor_registration: ChildRegistration,
        target: Child,
        target_registration: ChildRegistration,
        target_pid: u32,
        group_id: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildWait {
    NotRequired,
    Pending,
    Exited(ExitStatus),
    Failed,
}

impl ChildWait {
    const fn status(self) -> Option<ExitStatus> {
        match self {
            Self::Exited(status) => Some(status),
            Self::NotRequired | Self::Pending | Self::Failed => None,
        }
    }

    const fn permits_anchor_wait(self) -> bool {
        matches!(self, Self::NotRequired | Self::Exited(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupObservation {
    Pending,
    Present,
    Drained,
    Uncertain,
}

#[derive(Debug)]
pub(super) struct OwnedGroup {
    anchor: Option<Child>,
    anchor_registration: Option<ChildRegistration>,
    target: Option<Child>,
    target_registration: Option<ChildRegistration>,
    target_pid: u32,
    group: Option<Pid>,
    backend: Box<dyn GroupSignalBackend>,
    phase: GroupPhase,
    cleanup_deadline: Option<Instant>,
    target_wait: ChildWait,
    anchor_wait: ChildWait,
    group_observation: GroupObservation,
}

#[derive(Debug)]
pub(super) struct CleanupReport {
    pub(super) target_status: Option<ExitStatus>,
    pub(super) group_drained: bool,
    pub(super) recovery: Option<SpoolRecovery>,
    pub(super) failures: Vec<SpoolFailure>,
}

impl OwnedGroup {
    pub(super) fn spawn(command: &mut Command) -> Result<Self, SpawnError> {
        Self::spawn_with_backend(command, Box::new(SystemGroupSignalBackend))
    }

    pub(super) fn spawn_with_backend(
        command: &mut Command,
        backend: Box<dyn GroupSignalBackend>,
    ) -> Result<Self, SpawnError> {
        let spawned = crate::coordinate_children(|registry| {
            if let Err(error) = registry.reserve_registrations(2) {
                return Spawned::AnchorFailed(io::Error::other(error));
            }
            let anchor_slot = match registry.prepare_registration() {
                Ok(slot) => slot,
                Err(error) => return Spawned::AnchorFailed(io::Error::other(error)),
            };
            let target_slot = match registry.prepare_registration() {
                Ok(slot) => slot,
                Err(error) => return Spawned::AnchorFailed(io::Error::other(error)),
            };
            let anchor = match spawn_anchor() {
                Ok(anchor) => anchor,
                Err(error) => return Spawned::AnchorFailed(error),
            };
            let anchor_pid = anchor.id();
            let anchor_registration = registry.register_prepared(anchor_pid, anchor_slot);
            let group_id = match i32::try_from(anchor_pid) {
                Ok(group_id) if group_id > 0 => group_id,
                _ => {
                    return Spawned::TargetFailed {
                        anchor,
                        anchor_registration,
                        error: io::Error::new(
                            io::ErrorKind::InvalidData,
                            "owned anchor PID is outside the process-group range",
                        ),
                    };
                }
            };
            command.process_group(group_id);
            match command.spawn() {
                Ok(target) => {
                    let target_pid = target.id();
                    let target_registration = registry.register_prepared(target_pid, target_slot);
                    Spawned::Started {
                        anchor,
                        anchor_registration,
                        target,
                        target_registration,
                        target_pid,
                        group_id,
                    }
                }
                Err(error) => Spawned::TargetFailed {
                    anchor,
                    anchor_registration,
                    error,
                },
            }
        });

        match spawned {
            Spawned::AnchorFailed(error) => Err(SpawnError {
                unsupported: error.kind() == io::ErrorKind::NotFound,
                message: format!("starting owned process-group anchor: {error}"),
                group: None,
            }),
            Spawned::TargetFailed {
                anchor,
                anchor_registration,
                error,
            } => {
                let group_id = i32::try_from(anchor.id())
                    .ok()
                    .filter(|group_id| *group_id > 0)
                    .map(Pid::from_raw);
                let group = Self::new(
                    anchor,
                    anchor_registration,
                    None,
                    None,
                    0,
                    group_id,
                    backend,
                );
                Err(SpawnError {
                    unsupported: false,
                    message: format!("starting requested child: {error}"),
                    group: Some(Box::new(group)),
                })
            }
            Spawned::Started {
                anchor,
                anchor_registration,
                target,
                target_registration,
                target_pid,
                group_id,
            } => {
                let mut group = Self::new(
                    anchor,
                    anchor_registration,
                    Some(target),
                    Some(target_registration),
                    target_pid,
                    Some(Pid::from_raw(group_id)),
                    backend,
                );
                if let Some(target) = group.target.as_mut() {
                    drop(target.stdin.take());
                }
                Ok(group)
            }
        }
    }

    fn new(
        anchor: Child,
        anchor_registration: ChildRegistration,
        target: Option<Child>,
        target_registration: Option<ChildRegistration>,
        target_pid: u32,
        group: Option<Pid>,
        backend: Box<dyn GroupSignalBackend>,
    ) -> Self {
        let target_wait = if target.is_some() {
            ChildWait::Pending
        } else {
            ChildWait::NotRequired
        };
        Self {
            anchor: Some(anchor),
            anchor_registration: Some(anchor_registration),
            target,
            target_registration,
            target_pid,
            group,
            backend,
            phase: GroupPhase::AnchorHeld,
            cleanup_deadline: None,
            target_wait,
            anchor_wait: ChildWait::Pending,
            group_observation: GroupObservation::Pending,
        }
    }

    pub(super) const fn target_pid(&self) -> u32 {
        self.target_pid
    }

    pub(super) fn process_group_id(&self) -> i32 {
        self.group.map_or(0, Pid::as_raw)
    }

    pub(super) fn take_output_pipes(
        &mut self,
    ) -> (
        Option<std::process::ChildStdout>,
        Option<std::process::ChildStderr>,
    ) {
        match self.target.as_mut() {
            Some(target) => (target.stdout.take(), target.stderr.take()),
            None => (None, None),
        }
    }

    pub(super) fn observe_target_exit(&mut self) -> Result<bool, io::Error> {
        match self.target_wait {
            ChildWait::Exited(_) => return Ok(true),
            ChildWait::Failed => {
                return Err(io::Error::other("target wait status is uncertain"));
            }
            ChildWait::NotRequired => return Ok(false),
            ChildWait::Pending => {}
        }
        let Some(target) = self.target.as_mut() else {
            return Ok(false);
        };
        match self.backend.wait_child(OwnedChild::Target, target) {
            Ok(Some(status)) => {
                self.target_wait = ChildWait::Exited(status);
                self.target_registration.take();
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
            Err(error) => {
                self.target_wait = ChildWait::Failed;
                Err(error)
            }
        }
    }

    pub(super) fn begin_finalization(&mut self) {
        if self.phase == GroupPhase::AnchorHeld {
            let now = Instant::now();
            self.cleanup_deadline = Some(now.checked_add(CLEANUP_WINDOW).unwrap_or(now));
            self.phase = GroupPhase::Finalizing;
        }
    }

    pub(super) fn cleanup_deadline(&self) -> Option<Instant> {
        self.cleanup_deadline
    }

    #[cfg(test)]
    pub(super) fn expire_cleanup_deadline_for_test(&mut self) {
        self.cleanup_deadline = Some(Instant::now());
    }

    pub(super) fn cleanup_complete(&self) -> bool {
        self.phase == GroupPhase::ReapedOrUncertain || self.phase == GroupPhase::Finished
    }

    pub(super) fn cleanup_step(&mut self, failures: &mut Vec<SpoolFailure>) {
        self.begin_finalization();
        if self.cleanup_complete() || self.enforce_cleanup_deadline(failures) {
            return;
        }
        if !self.signal_held_group(failures) {
            return;
        }
        if self.phase != GroupPhase::DestructiveClosed {
            return;
        }
        self.poll_target_after_closure(failures);
        self.poll_anchor_after_target(failures);
        self.probe_group_after_reap(failures);
    }

    fn enforce_cleanup_deadline(&mut self, failures: &mut Vec<SpoolFailure>) -> bool {
        if !self.deadline_expired() {
            return false;
        }
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Cleanup,
            message: "four-second cleanup deadline expired with uncertain child or group state"
                .to_owned(),
        });
        self.group_observation = GroupObservation::Uncertain;
        self.phase = GroupPhase::ReapedOrUncertain;
        true
    }

    fn signal_held_group(&mut self, failures: &mut Vec<SpoolFailure>) -> bool {
        if self.phase != GroupPhase::Finalizing {
            return true;
        }
        let Some(group) = self.group else {
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Cleanup,
                message: "anchor PID cannot be represented as a process-group ID".to_owned(),
            });
            self.phase = GroupPhase::DestructiveClosed;
            return true;
        };
        match self.backend.signal_group(group, Signal::SIGKILL) {
            Ok(()) | Err(Errno::ESRCH) => {
                self.phase = GroupPhase::DestructiveClosed;
                true
            }
            Err(Errno::EINTR) => false,
            Err(error) => {
                failures.push(SpoolFailure {
                    kind: SpoolFailureKind::Cleanup,
                    message: format!(
                        "closing destructive group authority after SIGKILL error: {error}"
                    ),
                });
                self.phase = GroupPhase::DestructiveClosed;
                true
            }
        }
    }

    fn poll_target_after_closure(&mut self, failures: &mut Vec<SpoolFailure>) {
        if !matches!(self.target_wait, ChildWait::Pending) {
            return;
        }
        let Some(target) = self.target.as_mut() else {
            return;
        };
        match self.backend.wait_child(OwnedChild::Target, target) {
            Ok(Some(status)) => {
                self.target_wait = ChildWait::Exited(status);
                self.target_registration.take();
            }
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                self.target_wait = ChildWait::Failed;
                failures.push(SpoolFailure {
                    kind: SpoolFailureKind::Cleanup,
                    message: format!("observing requested child after group closure: {error}"),
                });
            }
        }
    }

    fn poll_anchor_after_target(&mut self, failures: &mut Vec<SpoolFailure>) {
        if !self.target_wait.permits_anchor_wait()
            || !matches!(self.anchor_wait, ChildWait::Pending)
        {
            return;
        }
        let Some(anchor) = self.anchor.as_mut() else {
            return;
        };
        drop(anchor.stdin.take());
        match self.backend.wait_child(OwnedChild::Anchor, anchor) {
            Ok(Some(status)) => {
                self.anchor_wait = ChildWait::Exited(status);
                self.anchor_registration.take();
            }
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                self.anchor_wait = ChildWait::Failed;
                failures.push(SpoolFailure {
                    kind: SpoolFailureKind::Cleanup,
                    message: format!("observing owned anchor after target reap: {error}"),
                });
            }
        }
    }

    fn probe_group_after_reap(&mut self, failures: &mut Vec<SpoolFailure>) {
        if !self.target_wait.permits_anchor_wait()
            || !matches!(self.anchor_wait, ChildWait::Exited(_))
        {
            return;
        }
        let Some(group) = self.group else {
            self.group_observation = GroupObservation::Uncertain;
            self.phase = GroupPhase::ReapedOrUncertain;
            return;
        };
        match self.backend.group_exists(group) {
            Err(Errno::ESRCH) => {
                self.group_observation = GroupObservation::Drained;
                self.phase = GroupPhase::ReapedOrUncertain;
            }
            Ok(()) => self.group_observation = GroupObservation::Present,
            Err(Errno::EINTR) => {}
            Err(error) => {
                failures.push(SpoolFailure {
                    kind: SpoolFailureKind::Cleanup,
                    message: format!("group absence is uncertain after anchor release: {error}"),
                });
                self.group_observation = GroupObservation::Uncertain;
                self.phase = GroupPhase::ReapedOrUncertain;
            }
        }
    }

    pub(super) fn cleanup_without_pump(mut self) -> CleanupReport {
        let mut failures = Vec::new();
        self.begin_finalization();
        while !self.cleanup_complete() {
            self.cleanup_step(&mut failures);
            if !self.cleanup_complete() {
                process_supervision_poll(self.cleanup_deadline);
            }
        }
        self.into_report(failures)
    }

    pub(super) fn into_report(mut self, failures: Vec<SpoolFailure>) -> CleanupReport {
        if !self.cleanup_complete() {
            self.phase = GroupPhase::DestructiveClosed;
            self.group_observation = GroupObservation::Uncertain;
            self.phase = GroupPhase::ReapedOrUncertain;
        }
        let recovery = self.take_recovery();
        let report = CleanupReport {
            target_status: self.target_wait.status(),
            group_drained: matches!(self.group_observation, GroupObservation::Drained),
            recovery,
            failures,
        };
        self.phase = GroupPhase::Finished;
        report
    }

    fn take_recovery(&mut self) -> Option<SpoolRecovery> {
        let mut children = Vec::new();
        if self.target_wait.status().is_none()
            && let Some(child) = self.target.take()
        {
            children.push(RecoveryChild {
                child,
                registration: self.target_registration.take(),
            });
        }
        if self.anchor_wait.status().is_none()
            && let Some(child) = self.anchor.take()
        {
            children.push(RecoveryChild {
                child,
                registration: self.anchor_registration.take(),
            });
        }
        (!children.is_empty()).then_some(SpoolRecovery { children })
    }

    fn deadline_expired(&self) -> bool {
        self.cleanup_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    fn emergency_finalize(&mut self) {
        if self.cleanup_complete() {
            return;
        }
        self.begin_finalization();
        let mut failures = Vec::new();
        while !self.cleanup_complete() {
            self.cleanup_step(&mut failures);
            if !self.cleanup_complete() {
                process_supervision_poll(self.cleanup_deadline);
            }
        }
    }
}

impl Drop for OwnedGroup {
    fn drop(&mut self) {
        if self.phase == GroupPhase::Finished {
            return;
        }
        self.emergency_finalize();
        self.phase = GroupPhase::Finished;
        if let Some(anchor) = self.anchor.as_mut() {
            drop(anchor.stdin.take());
            drop(anchor.stdout.take());
            drop(anchor.stderr.take());
        }
        if let Some(target) = self.target.as_mut() {
            drop(target.stdin.take());
            drop(target.stdout.take());
            drop(target.stderr.take());
        }

        if let Some(registration) = self.target_registration.take() {
            registration.release_to_reaper();
        }
        if let Some(registration) = self.anchor_registration.take() {
            registration.release_to_reaper();
        }
    }
}

#[derive(Debug)]
pub(super) struct RecoveryChild {
    child: Child,
    registration: Option<ChildRegistration>,
}

impl RecoveryChild {
    pub(super) fn poll_reap_nonblocking(&mut self) -> Result<bool, io::Error> {
        match self.child.try_wait()? {
            Some(_status) => {
                self.registration.take();
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub(super) fn retained(&self) -> bool {
        self.registration.is_some()
    }

    #[cfg(test)]
    pub(super) fn terminate_and_reap_for_test(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_none() {
            match self.child.kill() {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
                Err(error) => return Err(error),
            }
        }
        self.child.wait()?;
        self.registration.take();
        Ok(())
    }
}

impl Drop for RecoveryChild {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        drop(self.child.stdout.take());
        drop(self.child.stderr.take());
        if let Some(registration) = self.registration.take() {
            registration.release_to_reaper();
        }
    }
}

pub(super) trait GroupSignalBackend: fmt::Debug {
    fn signal_group(&mut self, group: Pid, signal: Signal) -> nix::Result<()>;
    fn group_exists(&mut self, group: Pid) -> nix::Result<()>;

    fn wait_child(
        &mut self,
        _role: OwnedChild,
        child: &mut Child,
    ) -> Result<Option<ExitStatus>, io::Error> {
        child.try_wait()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnedChild {
    Target,
    Anchor,
}

#[derive(Debug)]
struct SystemGroupSignalBackend;

impl GroupSignalBackend for SystemGroupSignalBackend {
    fn signal_group(&mut self, group: Pid, signal: Signal) -> nix::Result<()> {
        killpg(group, signal)
    }

    fn group_exists(&mut self, group: Pid) -> nix::Result<()> {
        killpg(group, None)
    }
}

fn spawn_anchor() -> Result<Child, io::Error> {
    let mut missing = None;
    for path in ["/bin/cat", "/usr/bin/cat"] {
        let mut command = Command::new(path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        match command.spawn() {
            Ok(anchor) => return Ok(anchor),
            Err(error) if error.kind() == io::ErrorKind::NotFound => missing = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(missing.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no supported group-anchor executable",
        )
    }))
}
