// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_console::backend::{
    Backend, ChangeSet, Completion, Effect, FolderProbe, OnePassword, ValidationOutcome,
};
use jackin_console::domain::account::CredentialSource as AccountSource;
use jackin_console::domain::agent::Provider;
use jackin_console::domain::instance::Instance;
use jackin_console::domain::launch::LaunchContent;
use jackin_console::launch::{LaunchFeed, LaunchTarget};
use jackin_console::panes::{PaneBody, PaneRequest, PaneSpawn};
use jackin_console::world::{Msg, World};

use crate::bridge::Bridge;
use crate::observe::Inputs;
use crate::sessions::Access;
use crate::unbuilt::{self, UnbuiltLaunch, UnbuiltPane};

#[derive(Clone)]
pub(crate) struct JackinBackend {
    inputs: Arc<Inputs>,
    bridge: Bridge,
    refreshing: Arc<AtomicBool>,
}

impl fmt::Debug for JackinBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JackinBackend").finish_non_exhaustive()
    }
}

impl JackinBackend {
    pub(crate) fn new(inputs: Arc<Inputs>, bridge: Bridge) -> JackinBackend {
        JackinBackend {
            inputs,
            bridge,
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn rerun_doctor(&self) {
        let Some(run) = self.inputs.doctor.begin() else {
            return;
        };
        let inputs = Arc::clone(&self.inputs);
        self.bridge.spawn(async move {
            let report = inputs.run_doctor(true).await;
            run.finish(report.clone());
            Msg::DoctorFinished(report)
        });
    }

    fn refresh(&self) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let inputs = Arc::clone(&self.inputs);
        let refreshing = Arc::clone(&self.refreshing);
        self.bridge.spawn(async move {
            let result = match inputs.observe().await {
                Ok(observation) if observation.snapshot.index_error.is_some() => {
                    Err(observation.status().unwrap_or_default())
                }
                Ok(observation) => Ok(Box::new(observation.snapshot)),
                Err(error) => Err(error.to_string()),
            };
            refreshing.store(false, Ordering::SeqCst);
            Msg::Refreshed { result }
        });
    }
}

impl Backend for JackinBackend {
    fn validate(
        &self,
        _provider: Provider,
        _source: &AccountSource,
        _plain_value: Option<&str>,
        _op: &dyn OnePassword,
        _now: i64,
    ) -> ValidationOutcome {
        unbuilt::validation()
    }

    fn probe_folder(&self, _path: &str) -> FolderProbe {
        unbuilt::folder_probe()
    }

    fn perform(&self, effect: &Effect, _world: &World) -> Option<Completion> {
        match effect {
            Effect::Refresh => {
                self.refresh();
                None
            }
            Effect::RunDoctor => {
                self.rerun_doctor();
                None
            }
            _ => unbuilt::fails(effect),
        }
    }

    fn due(&self, world: &World) -> Vec<Msg> {
        if self.inputs.doctor.due(world.now_secs()) {
            self.rerun_doctor();
        }
        Vec::new()
    }

    fn changes(
        &self,
        _instance_id: &str,
        _touched: &[String],
        _uncommitted: usize,
        unpushed: usize,
    ) -> ChangeSet {
        unbuilt::changes(unpushed)
    }

    fn launch_content(&self) -> Arc<LaunchContent> {
        Arc::new(LaunchContent::openshell())
    }

    fn launch(&self, world: &World, target: &LaunchTarget) -> Box<dyn LaunchFeed> {
        let sessions = &self.inputs.sessions;
        let failure = match sessions.access(sessions.default_name()) {
            Access::Down { error, .. } => {
                unbuilt::gateway_failure(&LaunchContent::openshell().network_failure, &error)
            }
            Access::Up(_) | Access::NotChosen => unbuilt::launch_failure(),
        };
        Box::new(UnbuiltLaunch::new(world, target, failure))
    }

    fn spawn_pane(&self, _request: &PaneRequest, spawn: PaneSpawn) -> Box<dyn PaneBody> {
        Box::new(UnbuiltPane::new(&spawn))
    }

    fn initial_panes(&self, _instance: &Instance) -> Vec<PaneRequest> {
        Vec::new()
    }
}
