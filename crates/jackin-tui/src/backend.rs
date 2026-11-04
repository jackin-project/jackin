// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_console::backend::{Backend, Completion, Effect};
use jackin_console::domain::instance::Instance;
use jackin_console::domain::launch::LaunchContent;
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::launch::{LaunchFeed, LaunchTarget};
use jackin_console::panes::{PaneBody, PaneRequest, PaneSpawn};
use jackin_console::world::{Msg, World};

use crate::bridge::Bridge;
use crate::launch;
use crate::observe::Inputs;
use crate::unbuilt::{self, UnbuiltPane};

#[derive(Clone)]
pub(crate) struct JackinBackend {
    pub(crate) inputs: Arc<Inputs>,
    pub(crate) bridge: Bridge,
    words: Vocabulary,
    refreshing: Arc<AtomicBool>,
}

impl fmt::Debug for JackinBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JackinBackend").finish_non_exhaustive()
    }
}

impl JackinBackend {
    pub(crate) fn new(inputs: Arc<Inputs>, bridge: Bridge, words: Vocabulary) -> JackinBackend {
        JackinBackend {
            inputs,
            bridge,
            words,
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn rerun_doctor(&self) {
        let inputs = Arc::clone(&self.inputs);
        self.bridge
            .spawn(async move { Msg::DoctorFinished(inputs.run_doctor(true).await) });
    }

    pub(crate) fn refresh(&self) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let inputs = Arc::clone(&self.inputs);
        let refreshing = Arc::clone(&self.refreshing);
        let words = self.words.clone();
        self.bridge.spawn(async move {
            let result = match inputs.observe().await {
                Ok(observation) if observation.snapshot.index_error.is_some() => {
                    Err(observation.status(&words).unwrap_or_default())
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
            _ => unbuilt::fails(effect, &self.words),
        }
    }

    fn launch_content(&self) -> Arc<LaunchContent> {
        Arc::new(LaunchContent::openshell())
    }

    fn launch(&self, world: &World, target: &LaunchTarget) -> Box<dyn LaunchFeed> {
        launch::start(self, world, target)
    }

    fn spawn_pane(&self, _request: &PaneRequest, spawn: PaneSpawn) -> Box<dyn PaneBody> {
        Box::new(UnbuiltPane::new(&spawn, &self.words))
    }

    fn initial_panes(&self, _instance: &Instance) -> Vec<PaneRequest> {
        Vec::new()
    }
}
