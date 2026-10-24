// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]

use super::*;
use std::collections::{HashMap, VecDeque};

struct ScriptedWaitBackend {
    peeks: VecDeque<Result<Option<Pid>, nix::errno::Errno>>,
    waits: HashMap<i32, VecDeque<Result<WaitStatus, nix::errno::Errno>>>,
    peek_calls: usize,
}

impl ScriptedWaitBackend {
    fn new(
        peeks: impl IntoIterator<Item = Result<Option<Pid>, nix::errno::Errno>>,
        waits: impl IntoIterator<Item = (Pid, Result<WaitStatus, nix::errno::Errno>)>,
    ) -> Self {
        let mut queued_waits: HashMap<_, VecDeque<_>> = HashMap::new();
        for (pid, wait) in waits {
            queued_waits
                .entry(pid.as_raw())
                .or_default()
                .push_back(wait);
        }
        Self {
            peeks: peeks.into_iter().collect(),
            waits: queued_waits,
            peek_calls: 0,
        }
    }
}

impl ReaperWaitBackend for ScriptedWaitBackend {
    fn peek(&mut self) -> nix::Result<Option<Pid>> {
        self.peek_calls += 1;
        self.peeks.pop_front().unwrap_or(Ok(None))
    }

    fn reap(&mut self, pid: Pid) -> nix::Result<WaitStatus> {
        self.waits
            .get_mut(&pid.as_raw())
            .and_then(VecDeque::pop_front)
            .unwrap_or(Err(nix::errno::Errno::ECHILD))
    }
}

#[derive(Default)]
struct RecordingObserver {
    selected: Vec<(Pid, bool)>,
    reaped: Vec<Pid>,
}

impl ReaperObserver for RecordingObserver {
    fn selected(&mut self, pid: Pid, managed: bool) {
        self.selected.push((pid, managed));
    }

    fn reaped(&mut self, pid: Pid) {
        self.reaped.push(pid);
    }
}

#[test]
fn same_pending_event_retries_peek_and_targeted_wait_eintr() {
    let pid = Pid::from_raw(41_001);
    let mut backend = ScriptedWaitBackend::new(
        [Err(nix::errno::Errno::EINTR), Ok(Some(pid)), Ok(None)],
        [
            (pid, Err(nix::errno::Errno::EINTR)),
            (pid, Ok(WaitStatus::Exited(pid, 0))),
        ],
    );
    let mut observer = RecordingObserver::default();
    let pass = run_selective_pass(
        &jackin_process::ChildRegistry::default(),
        &mut observer,
        &mut backend,
    );
    assert_eq!(pass, SelectivePass::complete());
    assert_eq!(backend.peek_calls, 3);
    assert_eq!(observer.selected, [(pid, false)]);
    assert_eq!(observer.reaped, [pid]);
}

#[test]
fn targeted_echild_continues_to_the_next_selected_child() {
    let raced = Pid::from_raw(41_002);
    let eligible = Pid::from_raw(41_003);
    let mut backend = ScriptedWaitBackend::new(
        [Ok(Some(raced)), Ok(Some(eligible)), Ok(None)],
        [
            (raced, Err(nix::errno::Errno::ECHILD)),
            (eligible, Ok(WaitStatus::Exited(eligible, 0))),
        ],
    );
    let mut observer = RecordingObserver::default();
    let pass = run_selective_pass(
        &jackin_process::ChildRegistry::default(),
        &mut observer,
        &mut backend,
    );
    assert_eq!(pass, SelectivePass::complete());
    assert_eq!(observer.reaped, [eligible]);
}

#[test]
fn permanent_eintr_is_finite_and_returns_a_requeue_disposition() {
    let interruption_count = SELECTIVE_EINTR_BUDGET + 1;
    let mut backend = ScriptedWaitBackend::new(
        std::iter::repeat_n(Err(nix::errno::Errno::EINTR), interruption_count),
        [],
    );
    let mut observer = RecordingObserver::default();
    let pass = run_selective_pass(
        &jackin_process::ChildRegistry::default(),
        &mut observer,
        &mut backend,
    );
    assert_eq!(pass.failure, Some(nix::errno::Errno::EINTR));
    assert!(pass.requeue);
    assert_eq!(backend.peek_calls, interruption_count);
}
