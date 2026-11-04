// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_console::backend::{Backend, Completion, Effect, FileSource};
use jackin_console::domain::instance::{Instance, InstanceStatus};
use jackin_console::domain::launch::LaunchContent;
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::launch::{LaunchFeed, LaunchTarget};
use jackin_console::panes::{
    PaneBody, PaneKind, PaneRefusal, PaneRequest, PaneSpawn, PaneTiming, TerminalPane,
};
use jackin_console::world::{Msg, World};
use jackin_session::REPAINT_NUDGE;

use crate::bridge::Bridge;
use crate::files::HostFiles;
use crate::launch;
use crate::observe::Inputs;
use crate::panes::PaneHub;
use crate::unbuilt;

#[derive(Clone)]
pub(crate) struct JackinBackend {
    pub(crate) inputs: Arc<Inputs>,
    pub(crate) bridge: Bridge,
    words: Vocabulary,
    panes: PaneHub,
    refreshing: Arc<AtomicBool>,
}

impl fmt::Debug for JackinBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JackinBackend").finish_non_exhaustive()
    }
}

impl JackinBackend {
    pub(crate) fn new(
        inputs: Arc<Inputs>,
        bridge: Bridge,
        words: Vocabulary,
        panes: PaneHub,
    ) -> JackinBackend {
        JackinBackend {
            inputs,
            bridge,
            words,
            panes,
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn rerun_doctor(&self) {
        let inputs = Arc::clone(&self.inputs);
        self.bridge
            .spawn(async move { Msg::DoctorFinished(inputs.run_doctor(true).await) });
    }

    fn settle(&self, effect: Effect) {
        let inputs = Arc::clone(&self.inputs);
        match effect {
            Effect::SaveWorkspace {
                base, workspace, ..
            } => self
                .bridge
                .spawn(async move { inputs.save_workspace(base.as_deref(), &workspace).await }),
            Effect::SaveGlobal { base, global } => self
                .bridge
                .spawn(async move { inputs.save_global(&base, &global).await }),
            Effect::SaveAccounts { base, registry } => self
                .bridge
                .spawn(async move { inputs.save_accounts(&base, &registry).await }),
            Effect::DeleteWorkspace { id } => self
                .bridge
                .spawn(async move { inputs.delete_workspace(id).await }),
            Effect::Stop { instance } => self
                .bridge
                .spawn(async move { inputs.stop(&instance).await }),
            Effect::Purge { instance } => self
                .bridge
                .spawn(async move { inputs.purge(&instance).await }),
            Effect::TrustRole { role } => self
                .bridge
                .spawn(async move { inputs.trust_role(&role).await }),
            Effect::LoadRole { role } => self
                .bridge
                .spawn(async move { inputs.load_role(&role).await }),
            Effect::CheckRole { role, doc } => self
                .bridge
                .spawn(async move { inputs.check_role(&role, &doc).await }),
            Effect::SaveRole { role, base, doc } => self
                .bridge
                .spawn(async move { inputs.save_role(&role, &base, &doc).await }),
            Effect::BindRole {
                role,
                agent,
                account,
                ..
            } => self
                .bridge
                .spawn(async move { inputs.bind_role(&role, agent, account.as_ref()).await }),
            Effect::AddRole { location } => self
                .bridge
                .spawn(async move { inputs.add_role(&location).await }),
            Effect::RemoveRole { role } => self
                .bridge
                .spawn(async move { inputs.remove_role(&role).await }),
            Effect::RefreshAccount { account, .. } => self
                .bridge
                .spawn(async move { inputs.refresh_account(&account).await }),
            Effect::ValidateAccount { account } => self
                .bridge
                .spawn(async move { inputs.validate_account(&account).await }),
            Effect::CheckSource {
                ticket,
                provider,
                source,
                key,
            } => self.bridge.spawn(async move {
                inputs
                    .check_source(ticket, provider, &source, key.as_ref())
                    .await
            }),
            Effect::ProbeFolder { ticket, path } => self
                .bridge
                .spawn(async move { inputs.probe_folder(ticket, &path).await }),
            Effect::ReadChanges {
                ticket,
                instance,
                unpushed,
                ..
            } => self
                .bridge
                .spawn(async move { inputs.read_changes(ticket, &instance, unpushed).await }),
            Effect::LoadOp { ticket, scope } => self.bridge.spawn(async move {
                Msg::OpLoaded {
                    ticket,
                    result: inputs.list_op(&scope).await,
                }
            }),
            Effect::Refresh
            | Effect::RunDoctor
            | Effect::Restore { .. }
            | Effect::Prewarm { .. } => {}
        }
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
            Effect::Restore { .. } | Effect::Prewarm { .. } => unbuilt::fails(effect, &self.words),
            _ => {
                self.settle(effect.clone());
                None
            }
        }
    }

    fn file_source(&self, _world: &World) -> Box<dyn FileSource> {
        Box::new(HostFiles::new(
            Arc::clone(&self.inputs.host.fs),
            self.bridge.handle().clone(),
            self.inputs.home.clone(),
        ))
    }

    fn launch_content(&self) -> Arc<LaunchContent> {
        Arc::new(LaunchContent::openshell())
    }

    fn launch(&self, world: &World, target: &LaunchTarget) -> Box<dyn LaunchFeed> {
        launch::start(self, world, target)
    }

    fn spawn_pane(&self, request: &PaneRequest, spawn: PaneSpawn) -> Box<dyn PaneBody> {
        Box::new(TerminalPane::new(
            spawn.id,
            request.clone(),
            self.panes.links(spawn.id),
            self.pane_timing(),
            spawn.now_ms,
        ))
    }

    fn initial_panes(&self, instance: &Instance) -> Vec<PaneRequest> {
        if instance.status != InstanceStatus::Running {
            return Vec::new();
        }
        vec![PaneRequest::new(
            &instance.id,
            Some(instance.agent),
            instance.account.clone(),
        )]
    }

    fn spawnable(
        &self,
        world: &World,
        instance: &Instance,
        kind: &PaneKind,
    ) -> Result<(), PaneRefusal> {
        let has_agent = world
            .daemons
            .get(&instance.id)
            .is_some_and(|daemon| daemon.panes.iter().any(|pane| pane.agent.is_some()));
        match kind {
            PaneKind::Agent { agent, .. } if has_agent => {
                Err(PaneRefusal::MainProcessOpen { agent: *agent })
            }
            PaneKind::Agent { .. } | PaneKind::Shell => Ok(()),
        }
    }

    fn capsule_left(&self, _world: &World, instance: &str) {
        self.panes.leave(instance);
    }

    fn pane_timing(&self) -> PaneTiming {
        let defaults = PaneTiming::default();
        PaneTiming {
            nudge_ms: i64::try_from(REPAINT_NUDGE.as_millis()).unwrap_or(defaults.nudge_ms),
            ..defaults
        }
    }
}
