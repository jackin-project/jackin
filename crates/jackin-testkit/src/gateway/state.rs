// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use jackin_core::{
    Clock, ImageId, ImageTag, Labels, Phase, ProfileId, ProviderName, SandboxName, SandboxPath,
    TerminalText, Timestamp,
};
use jackin_openshell::{
    GatewayError, GatewayOp, NetworkRule, ProviderProfile, Provisioning, SandboxCreate, SandboxView,
};
use tempfile::TempDir;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::main_process::{Main, MainPlan, PlanEnd};
use super::{
    HostEdit, ProgramScript, ProviderRecord, RunRecord, ShellWitness, Trigger, UploadKind,
};
use crate::engine::FakeEngine;
use crate::host::FakeClock;
use crate::supervisor::{Supervisor, SupervisorModel};
use crate::sync::locked;

pub(super) const SANDBOX_ROOT: &str = "/sandbox";
pub(super) const MAX_ATTACHMENTS: usize = 20;

#[derive(Debug)]
pub(super) struct Armed {
    op: GatewayOp,
    skip: u32,
    error: GatewayError,
}

impl Armed {
    pub(super) fn new(op: GatewayOp, skip: u32, error: GatewayError) -> Armed {
        Armed { op, skip, error }
    }
}

#[derive(Debug)]
pub(super) enum Action {
    Fail(GatewayError),
    Cancel(CancellationToken),
}

#[derive(Debug)]
pub(super) struct ArmedTrigger {
    trigger: Trigger,
    action: Action,
}

impl ArmedTrigger {
    pub(super) fn new(trigger: Trigger, action: Action) -> ArmedTrigger {
        ArmedTrigger { trigger, action }
    }
}

#[derive(Debug, Default)]
pub(super) struct Gate {
    reached: AtomicBool,
    released: AtomicBool,
    wake: Notify,
}

impl Gate {
    pub(super) async fn wait(&self) {
        loop {
            let woken = self.wake.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            if self.released.load(Ordering::SeqCst) {
                return;
            }
            woken.await;
        }
    }
}

#[derive(Debug)]
pub(super) struct Hold {
    trigger: Trigger,
    gate: Arc<Gate>,
}

impl Hold {
    pub(super) fn new(trigger: Trigger) -> Hold {
        Hold {
            trigger,
            gate: Arc::new(Gate::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HoldStatus {
    Absent,
    Armed,
    Reached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Direction {
    Download,
    Upload,
}

#[derive(Debug)]
pub(super) struct Registry {
    pub(super) sandboxes: BTreeMap<SandboxName, Arc<Cell>>,
    pub(super) providers: BTreeMap<ProviderName, ProviderRecord>,
    pub(super) profiles: BTreeMap<ProfileId, ProviderProfile>,
    pub(super) images: BTreeSet<ImageTag>,
    pub(super) installed: BTreeMap<SandboxPath, Vec<u8>>,
    pub(super) engine: Option<Arc<FakeEngine>>,
    pub(super) clock: Arc<FakeClock>,
    pub(super) faults: Vec<Armed>,
    pub(super) holds: Vec<Hold>,
    pub(super) attachment_cap: usize,
    pub(super) triggers: Vec<ArmedTrigger>,
    pub(super) stale_observations: u32,
    pub(super) scripts: BTreeMap<SandboxPath, ProgramScript>,
    pub(super) stalls: Vec<SandboxPath>,
    pub(super) corrupt: BTreeSet<Direction>,
    pub(super) provision_polls: u32,
    pub(super) failing_creates: u32,
    pub(super) supervisor: SupervisorModel,
    pub(super) hidden_until_conflict: bool,
    pub(super) start_ignored: bool,
    pub(super) serving: bool,
    pub(super) version: String,
    pub(super) host_home: Option<PathBuf>,
    pub(super) host_edits: Vec<(UploadKind, HostEdit)>,
}

impl Registry {
    pub(super) fn new() -> Registry {
        Registry {
            sandboxes: BTreeMap::new(),
            providers: BTreeMap::new(),
            profiles: BTreeMap::new(),
            images: BTreeSet::new(),
            installed: BTreeMap::new(),
            engine: None,
            clock: Arc::new(FakeClock::new()),
            faults: Vec::new(),
            holds: Vec::new(),
            attachment_cap: MAX_ATTACHMENTS,
            triggers: Vec::new(),
            stale_observations: 0,
            scripts: BTreeMap::new(),
            stalls: Vec::new(),
            corrupt: BTreeSet::new(),
            provision_polls: 0,
            failing_creates: 0,
            supervisor: SupervisorModel::Cooperative,
            hidden_until_conflict: false,
            start_ignored: false,
            serving: true,
            version: "0.1.6".to_owned(),
            host_home: None,
            host_edits: Vec::new(),
        }
    }

    pub(super) fn now(&self) -> Timestamp {
        self.clock.now()
    }

    pub(super) fn arrive(&mut self, op: GatewayOp, special: Option<&Trigger>) -> Option<Arc<Gate>> {
        let hold = self.holds.iter().find(|hold| {
            !hold.gate.reached.load(Ordering::SeqCst)
                && (hold.trigger == Trigger::Op(op) || special == Some(&hold.trigger))
        })?;
        hold.gate.reached.store(true, Ordering::SeqCst);
        Some(Arc::clone(&hold.gate))
    }

    pub(super) fn release(&mut self, trigger: &Trigger) -> bool {
        let Some(index) = self.holds.iter().position(|hold| hold.trigger == *trigger) else {
            return false;
        };
        let hold = self.holds.remove(index);
        hold.gate.released.store(true, Ordering::SeqCst);
        hold.gate.wake.notify_waiters();
        true
    }

    pub(super) fn held(&self) -> Vec<Trigger> {
        self.holds
            .iter()
            .filter(|hold| hold.gate.reached.load(Ordering::SeqCst))
            .map(|hold| hold.trigger.clone())
            .collect()
    }

    pub(super) fn hold_status(&self, trigger: &Trigger) -> HoldStatus {
        let mut status = HoldStatus::Absent;
        for hold in self.holds.iter().filter(|hold| hold.trigger == *trigger) {
            if hold.gate.reached.load(Ordering::SeqCst) {
                return HoldStatus::Reached;
            }
            status = HoldStatus::Armed;
        }
        status
    }

    pub(super) fn strike(&mut self, op: GatewayOp) -> Result<(), GatewayError> {
        self.strike_at(op, None)
    }

    pub(super) fn strike_at(
        &mut self,
        op: GatewayOp,
        special: Option<&Trigger>,
    ) -> Result<(), GatewayError> {
        self.count_down(op)?;
        let matches = |armed: &ArmedTrigger| {
            armed.trigger == Trigger::Op(op) || special == Some(&armed.trigger)
        };
        while let Some(index) = self.triggers.iter().position(matches) {
            match self.triggers.remove(index).action {
                Action::Fail(error) => return Err(error),
                Action::Cancel(token) => token.cancel(),
            }
        }
        Ok(())
    }

    fn count_down(&mut self, op: GatewayOp) -> Result<(), GatewayError> {
        let Some(index) = self.faults.iter().position(|armed| armed.op == op) else {
            return Ok(());
        };
        if self.faults[index].skip > 0 {
            self.faults[index].skip -= 1;
            return Ok(());
        }
        Err(self.faults.remove(index).error)
    }
}

#[derive(Debug)]
pub(super) struct Cell {
    pub(super) name: SandboxName,
    pub(super) dir: TempDir,
    pub(super) closed: CancellationToken,
    pub(super) inner: Mutex<Sandbox>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Settling {
    Done,
    Provisioning { polls_left: u32 },
    Restarting { stale: u32 },
}

#[derive(Debug)]
pub(super) struct Sandbox {
    pub(super) created_at: Option<Timestamp>,
    pub(super) image_id: Option<ImageId>,
    pub(super) phase: Phase,
    pub(super) provisioning: Provisioning,
    pub(super) spec: SandboxCreate,
    pub(super) settling: Settling,
    pub(super) failure: Option<String>,
    pub(super) plan: MainPlan,
    pub(super) main: Main,
    pub(super) generation: u64,
    pub(super) supervisor: Option<Supervisor>,
    pub(super) runs: Vec<(SandboxPath, RunRecord)>,
    pub(super) shown: Option<Labels>,
    pub(super) live_rules: Vec<NetworkRule>,
    pub(super) shells: Vec<Arc<Mutex<ShellWitness>>>,
    pub(super) stream_status: Option<i32>,
    pub(super) replay_window: Option<usize>,
}

impl Cell {
    pub(super) fn root(&self) -> PathBuf {
        self.dir.path().join("sandbox")
    }

    pub(super) fn state(&self) -> std::sync::MutexGuard<'_, Sandbox> {
        locked(&self.inner)
    }

    pub(super) fn settle(&self) -> Phase {
        let mut state = self.state();
        state.observe(&self.root());
        state.phase
    }
}

impl Sandbox {
    pub(super) fn new(
        spec: SandboxCreate,
        plan: MainPlan,
        polls: u32,
        failure: Option<String>,
        model: SupervisorModel,
        created_at: Timestamp,
    ) -> Sandbox {
        let supervisor = (plan.end == PlanEnd::Supervised).then(|| Supervisor::new(model));
        Sandbox {
            created_at: Some(created_at),
            image_id: None,
            phase: Phase::Provisioning,
            provisioning: Provisioning::Normal,
            spec,
            settling: Settling::Provisioning { polls_left: polls },
            failure,
            main: Main::new(&plan, 0),
            generation: 0,
            plan,
            supervisor,
            runs: Vec::new(),
            shown: None,
            live_rules: Vec::new(),
            shells: Vec::new(),
            stream_status: None,
            replay_window: None,
        }
    }

    pub(super) fn all_labels(&self) -> Labels {
        self.shown
            .clone()
            .unwrap_or_else(|| self.spec.labels().to_labels())
    }

    pub(super) fn view(&self) -> SandboxView {
        SandboxView {
            name: self.spec.name().clone(),
            phase: self.phase,
            labels: self.all_labels(),
            provisioning: self.provisioning.clone(),
            created_at: self.created_at,
        }
    }

    pub(super) fn observe(&mut self, root: &Path) {
        match self.settling {
            Settling::Done => {}
            Settling::Provisioning { polls_left } if polls_left > 0 => {
                self.settling = Settling::Provisioning {
                    polls_left: polls_left - 1,
                };
            }
            Settling::Provisioning { .. } => self.settle_new(root),
            Settling::Restarting { stale } if stale > 0 => {
                self.settling = Settling::Restarting { stale: stale - 1 };
                self.phase = Phase::Ready;
            }
            Settling::Restarting { .. } => self.settle_restart(root),
        }
    }

    fn settle_new(&mut self, root: &Path) {
        self.settling = Settling::Done;
        if let Some(message) = &self.failure {
            self.phase = Phase::Error { exit_code: None };
            self.provisioning = Provisioning::Failed {
                message: TerminalText::sanitize(message.as_bytes()),
            };
            return;
        }
        match self.plan.end {
            PlanEnd::Exits(code) => self.phase = ended_phase(code),
            PlanEnd::Sleeps | PlanEnd::Cat | PlanEnd::Supervised => {
                self.phase = Phase::Ready;
                self.restart_supervisor(root);
            }
        }
    }

    fn settle_restart(&mut self, root: &Path) {
        self.settling = Settling::Done;
        self.phase = Phase::Ready;
        self.generation += 1;
        self.main = Main::new(&self.plan, self.generation);
        self.restart_supervisor(root);
    }

    fn restart_supervisor(&mut self, root: &Path) {
        if let Some(supervisor) = &mut self.supervisor {
            supervisor.restart(root);
        }
    }

    pub(super) fn main_exited(&mut self, code: i32) {
        self.phase = ended_phase(code);
        self.settling = Settling::Done;
        self.main.exit(Some(self.stream_status.unwrap_or(code)));
        if let Some(supervisor) = &mut self.supervisor {
            supervisor.stop();
        }
    }
}

pub(super) fn ended_phase(code: i32) -> Phase {
    if code == 0 {
        Phase::Completed { exit_code: 0 }
    } else {
        Phase::Error {
            exit_code: Some(code),
        }
    }
}
