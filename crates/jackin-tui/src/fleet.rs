// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use jackin_core::{Config, GatewayName, Phase, SandboxSummary, SupervisorReading};
use jackin_session::{Session, WorkState};

use crate::sessions::{Reached, SessionSet};

#[derive(Clone, Debug)]
pub struct ObservedSandbox {
    pub gateway: GatewayName,
    pub summary: SandboxSummary,
    pub supervisor: Option<SupervisorReading>,
    pub work: Option<WorkState>,
}

#[derive(Clone, Debug)]
pub struct GatewayReading {
    pub name: GatewayName,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Fleet {
    pub sandboxes: Vec<ObservedSandbox>,
    pub unreadable: usize,
    pub gateways: Vec<GatewayReading>,
}

impl Fleet {
    pub fn failed(&self) -> impl Iterator<Item = &GatewayReading> {
        self.gateways.iter().filter(|g| g.failure.is_some())
    }

    pub fn all_failed(&self) -> bool {
        !self.gateways.is_empty() && self.gateways.iter().all(|g| g.failure.is_some())
    }
}

pub async fn read_fleet(sessions: &SessionSet, config: &Config) -> Fleet {
    let readings = join_all(
        sessions
            .reached()
            .into_iter()
            .map(|(name, reached)| async move {
                match reached {
                    Reached::Down(error) => (
                        GatewayReading {
                            name,
                            failure: Some(error.to_string()),
                        },
                        Vec::new(),
                        0,
                    ),
                    Reached::Up(session) => read_gateway(&name, &session, config).await,
                }
            }),
    )
    .await;
    let mut fleet = Fleet::default();
    for (reading, sandboxes, unreadable) in readings {
        fleet.gateways.push(reading);
        fleet.sandboxes.extend(sandboxes);
        fleet.unreadable += unreadable;
    }
    fleet
}

async fn read_gateway(
    name: &GatewayName,
    session: &Session,
    config: &Config,
) -> (GatewayReading, Vec<ObservedSandbox>, usize) {
    let listing = match session.list().await {
        Ok(listing) => listing,
        Err(error) => {
            return (
                GatewayReading {
                    name: name.clone(),
                    failure: Some(error.to_string()),
                },
                Vec::new(),
                0,
            );
        }
    };
    let unreadable = listing.unreadable.len();
    let sandboxes = join_all(
        listing
            .sandboxes
            .into_iter()
            .map(|summary| observe(name, session, config, summary)),
    )
    .await;
    (
        GatewayReading {
            name: name.clone(),
            failure: None,
        },
        sandboxes,
        unreadable,
    )
}

async fn observe(
    gateway: &GatewayName,
    session: &Session,
    config: &Config,
    summary: SandboxSummary,
) -> ObservedSandbox {
    let ready = summary.phase == Phase::Ready;
    let supervisor = if ready {
        session
            .inspect(&summary.name)
            .await
            .ok()
            .map(|view| view.supervisor)
    } else {
        None
    };
    let running = matches!(
        supervisor,
        Some(SupervisorReading::Reported(
            jackin_core::SupervisorState::Running { .. }
        ))
    );
    let work = if ready && !running {
        Some(session.work_state(config, &summary.name).await)
    } else {
        None
    };
    ObservedSandbox {
        gateway: gateway.clone(),
        summary,
        supervisor,
        work,
    }
}
