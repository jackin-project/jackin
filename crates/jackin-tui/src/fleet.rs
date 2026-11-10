// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use futures::future::join_all;
use jackin_core::{
    Config, GatewayName, Phase, SandboxName, SandboxSummary, SupervisorReading, SupervisorState,
    Timestamp,
};
use jackin_session::{Session, UnreadableSandbox, WorkState};
use thiserror::Error;
use tokio::sync::Semaphore;

use jackin_console::domain::instance::InstanceId;

use crate::console::ConsoleLimits;
use crate::sessions::{Reached, SessionSet};

#[derive(Clone, Debug)]
pub struct ObservedTask {
    pub text: String,
    pub statuses: Vec<(jackin_core::Agent, String)>,
}

#[derive(Clone, Debug)]
pub struct ObservedSandbox {
    pub gateway: GatewayName,
    pub summary: SandboxSummary,
    pub supervisor: Option<SupervisorReading>,
    pub work: Option<WorkState>,
    pub task: Option<ObservedTask>,
}

#[derive(Clone, Debug)]
pub struct ObservedUnreadable {
    pub gateway: GatewayName,
    pub sandbox: UnreadableSandbox,
}

#[derive(Clone, Debug)]
pub struct GatewayReading {
    pub name: GatewayName,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub sandboxes: Vec<ObservedSandbox>,
    pub unreadable: Vec<ObservedUnreadable>,
    pub gateways: Vec<GatewayReading>,
}

impl Fleet {
    pub fn failed(&self) -> impl Iterator<Item = &GatewayReading> {
        self.gateways.iter().filter(|g| g.failure.is_some())
    }

    pub fn all_failed(&self) -> bool {
        !self.gateways.is_empty() && self.gateways.iter().all(|g| g.failure.is_some())
    }

    pub(crate) fn admit(&mut self, observed: ObservedSandbox) {
        self.sandboxes.retain(|seen| {
            seen.gateway != observed.gateway || seen.summary.name != observed.summary.name
        });
        self.unreadable.retain(|seen| {
            seen.gateway != observed.gateway || seen.sandbox.name != observed.summary.name
        });
        self.sandboxes.push(observed);
    }

    pub(crate) fn locate(&self, instance: &InstanceId) -> Option<&ObservedSandbox> {
        self.sandboxes.iter().find(|observed| {
            observed.gateway.as_str() == instance.gateway.as_str()
                && observed.summary.name.as_str() == instance.sandbox_name.as_str()
        })
    }

    pub(crate) fn locate_unreadable(&self, instance: &InstanceId) -> Option<&ObservedUnreadable> {
        self.unreadable.iter().find(|observed| {
            observed.gateway.as_str() == instance.gateway.as_str()
                && observed.sandbox.name.as_str() == instance.sandbox_name.as_str()
        })
    }
}

#[derive(Debug, Default)]
pub struct WorkCache {
    entries: Mutex<BTreeMap<(GatewayName, SandboxName), (WorkKey, WorkState)>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkKey {
    phase: Phase,
    created_at: Option<Timestamp>,
    supervisor: Option<SupervisorReading>,
}

impl WorkCache {
    fn get(&self, at: &(GatewayName, SandboxName), key: &WorkKey) -> Option<WorkState> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(at)
            .filter(|(cached, _)| cached == key)
            .map(|(_, state)| state.clone())
    }

    fn put(&self, at: (GatewayName, SandboxName), key: WorkKey, state: WorkState) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.insert(at, (key, state));
    }

    fn keep_seen(&self, fleet: &Fleet) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(gateway, sandbox), _| {
            fleet.failed().any(|g| g.name == *gateway)
                || fleet
                    .sandboxes
                    .iter()
                    .any(|seen| seen.gateway == *gateway && seen.summary.name == *sandbox)
        });
    }
}

#[derive(Debug)]
pub(crate) struct Waited(pub(crate) Duration);

impl fmt::Display for Waited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[derive(Debug, Error)]
#[error("no answer within {0}")]
pub(crate) struct TimedOut(pub(crate) Waited);

pub(crate) async fn within<T>(deadline: Duration, work: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(deadline, work).await.ok()
}

struct Inspected {
    summary: SandboxSummary,
    supervisor: Option<SupervisorReading>,
}

impl Inspected {
    fn needs_work(&self) -> bool {
        self.summary.phase == Phase::Ready
            && !matches!(
                self.supervisor,
                Some(SupervisorReading::Reported(SupervisorState::Running { .. }))
            )
    }

    fn work_key(&self) -> WorkKey {
        WorkKey {
            phase: self.summary.phase,
            created_at: self.summary.created_at,
            supervisor: self.supervisor.clone(),
        }
    }
}

pub async fn read_fleet(
    sessions: &SessionSet,
    config: &Config,
    limits: ConsoleLimits,
    cache: &WorkCache,
) -> Fleet {
    let readings = join_all(
        sessions
            .reached()
            .into_iter()
            .map(|(name, reached)| async move {
                match reached {
                    Reached::Down(error) => unreadable(&name, error.to_string()),
                    Reached::Up(session) => {
                        read_gateway(&name, &session, config, limits, cache).await
                    }
                }
            }),
    )
    .await;
    let mut fleet = Fleet::default();
    for (reading, sandboxes, unreadable) in readings {
        fleet.gateways.push(reading);
        fleet.sandboxes.extend(sandboxes);
        fleet.unreadable.extend(unreadable);
    }
    cache.keep_seen(&fleet);
    fleet
}

fn unreadable(
    name: &GatewayName,
    failure: String,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadable>,
) {
    (
        GatewayReading {
            name: name.clone(),
            failure: Some(failure),
        },
        Vec::new(),
        Vec::new(),
    )
}

async fn read_gateway(
    name: &GatewayName,
    session: &Session,
    config: &Config,
    limits: ConsoleLimits,
    cache: &WorkCache,
) -> (
    GatewayReading,
    Vec<ObservedSandbox>,
    Vec<ObservedUnreadable>,
) {
    let deadline = limits.connect_deadline;
    let listing = match within(deadline, session.list()).await {
        Some(Ok(listing)) => listing,
        Some(Err(error)) => return unreadable(name, error.to_string()),
        None => return unreadable(name, TimedOut(Waited(deadline)).to_string()),
    };
    let unreadable = listing
        .unreadable
        .into_iter()
        .map(|sandbox| ObservedUnreadable {
            gateway: name.clone(),
            sandbox,
        })
        .collect();
    let inspected = join_all(
        listing
            .sandboxes
            .into_iter()
            .map(|summary| inspect(session, summary, deadline)),
    )
    .await;
    let slots = Semaphore::new(limits.work_reads.get());
    let works = join_all(
        inspected
            .iter()
            .map(|sandbox| read_work(name, session, config, sandbox, deadline, (cache, &slots))),
    )
    .await;
    let tasks = join_all(
        inspected
            .iter()
            .map(|sandbox| read_task(session, &sandbox.summary, deadline)),
    )
    .await;
    let sandboxes = inspected
        .into_iter()
        .zip(works)
        .zip(tasks)
        .map(
            |(
                (
                    Inspected {
                        summary,
                        supervisor,
                    },
                    work,
                ),
                task,
            )| ObservedSandbox {
                gateway: name.clone(),
                summary,
                supervisor,
                work,
                task,
            },
        )
        .collect();
    (
        GatewayReading {
            name: name.clone(),
            failure: None,
        },
        sandboxes,
        unreadable,
    )
}

async fn inspect(session: &Session, summary: SandboxSummary, deadline: Duration) -> Inspected {
    let supervisor = if summary.phase == Phase::Ready {
        within(deadline, session.inspect(&summary.name))
            .await
            .and_then(Result::ok)
            .map(|view| view.supervisor)
    } else {
        None
    };
    Inspected {
        summary,
        supervisor,
    }
}

async fn read_work(
    gateway: &GatewayName,
    session: &Session,
    config: &Config,
    sandbox: &Inspected,
    deadline: Duration,
    (cache, slots): (&WorkCache, &Semaphore),
) -> Option<WorkState> {
    if !sandbox.needs_work() {
        return None;
    }
    let at = (gateway.clone(), sandbox.summary.name.clone());
    let key = sandbox.work_key();
    if let Some(state) = cache.get(&at, &key) {
        return Some(state);
    }
    let _slot = slots.acquire().await.ok()?;
    let state = within(deadline, session.work_state(config, &sandbox.summary.name)).await?;
    if !matches!(state, WorkState::Unknown(_)) {
        cache.put(at, key, state.clone());
    }
    Some(state)
}

async fn read_task(
    session: &Session,
    summary: &SandboxSummary,
    deadline: Duration,
) -> Option<ObservedTask> {
    let agents: Vec<jackin_core::Agent> = summary
        .labels
        .planned
        .agents
        .iter()
        .map(|item| item.agent)
        .collect();
    let mut paths = vec![jackin_core::sandbox_layout::TASK_FILE.clone()];
    for core_agent in &agents {
        paths.push(jackin_core::sandbox_layout::task_status(*core_agent));
    }
    let found = within(
        deadline,
        session.gateway().read_files(&summary.name, &paths),
    )
    .await
    .and_then(Result::ok)?;
    let mut found = found.into_iter();
    let text = String::from_utf8(found.next().flatten()?).ok()?;
    let mut statuses = Vec::new();
    for (core_agent, bytes) in agents.into_iter().zip(found) {
        if let Some(bytes) = bytes
            && let Ok(text) = String::from_utf8(bytes)
        {
            statuses.push((core_agent, text));
        }
    }
    Some(ObservedTask { text, statuses })
}
