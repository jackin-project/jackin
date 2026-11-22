// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use jackin_console::backend::{Backend, Completion, Effect, FileSource, Ticket};
use jackin_console::domain::instance::{Instance, InstanceId, InstanceStatus};
use jackin_console::domain::launch::LaunchContent;
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::launch::{LaunchFeed, LaunchTarget};
use jackin_console::panes::{
    PaneBody, PaneKind, PaneRefusal, PaneRequest, PaneSpawn, PaneTiming, TerminalPane,
};
use jackin_console::world::{Msg, World};
use jackin_session::REPAINT_NUDGE;

use crate::bridge::Bridge;
use crate::crashed;
use crate::files::HostFiles;
use crate::launch;
use crate::observe::Inputs;
use crate::panes::PaneHub;

const REFRESH_EVERY_SECS: i64 = 5;
const NEVER: i64 = i64::MIN;

#[derive(Clone)]
pub(crate) struct JackinBackend {
    pub(crate) inputs: Arc<Inputs>,
    pub(crate) bridge: Bridge,
    words: Vocabulary,
    panes: PaneHub,
    pub(crate) usage_schedule: Arc<crate::connections::Schedule>,
    pub(crate) dashboard: Arc<std::sync::Mutex<jackin_usage::dashboard::DashboardMemory>>,
    pub(crate) usage_attempt: Arc<std::sync::Mutex<Option<crate::connections::Attempt>>>,
    refreshed_at: Arc<AtomicI64>,
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
            usage_attempt: Arc::default(),
            dashboard: Arc::default(),
            usage_schedule: Arc::default(),
            refreshed_at: Arc::new(AtomicI64::new(NEVER)),
        }
    }

    fn rerun_doctor(&self) {
        let inputs = Arc::clone(&self.inputs);
        self.bridge.spawn(crashed::doctor(), async move {
            Msg::DoctorFinished(inputs.run_doctor(true).await)
        });
    }

    fn settle(&self, effect: Effect) {
        let failure = crashed::reply(&effect);
        let inputs = Arc::clone(&self.inputs);
        match effect {
            Effect::SaveWorkspace {
                base, workspace, ..
            } => self.bridge.spawn(failure, async move {
                inputs.save_workspace(base.as_deref(), &workspace).await
            }),
            Effect::SaveGlobal { base, global } => {
                self.bridge.spawn(
                    failure,
                    async move { inputs.save_global(&base, &global).await },
                );
            }
            Effect::SaveAccounts {
                ticket,
                base,
                registry,
            } => self.bridge.spawn(failure, async move {
                inputs.save_accounts(ticket, &base, &registry).await
            }),
            Effect::DeleteWorkspace { id } => self
                .bridge
                .spawn(failure, async move { inputs.delete_workspace(id).await }),
            Effect::Stop { instance } => self
                .bridge
                .spawn(failure, async move { inputs.stop(&instance).await }),
            Effect::Purge { instance } => self
                .bridge
                .spawn(failure, async move { inputs.purge(&instance).await }),
            Effect::TrustRole { role } => self
                .bridge
                .spawn(failure, async move { inputs.trust_role(&role).await }),
            Effect::LoadRole { role } => self
                .bridge
                .spawn(failure, async move { inputs.load_role(&role).await }),
            Effect::CheckRole { role, doc, ticket } => self.bridge.spawn(failure, async move {
                inputs.check_role(ticket, &role, &doc).await
            }),
            Effect::SaveRole { role, base, doc } => self.bridge.spawn(failure, async move {
                inputs.save_role(&role, &base, &doc).await
            }),
            Effect::BindRole {
                role,
                agent,
                account,
                ..
            } => self.bridge.spawn(failure, async move {
                inputs.bind_role(&role, agent, account.as_ref()).await
            }),
            Effect::AddRole { location } => self
                .bridge
                .spawn(failure, async move { inputs.add_role(&location).await }),
            Effect::RemoveRole { role } => self
                .bridge
                .spawn(failure, async move { inputs.remove_role(&role).await }),
            Effect::RefreshAccount {
                ticket,
                account,
                source,
                ..
            } => {
                self.bridge.spawn(failure, async move {
                    inputs.refresh_account(ticket, &account, source).await
                });
            }
            Effect::ValidateAccount {
                ticket,
                account,
                source,
            } => {
                self.bridge.spawn(failure, async move {
                    inputs.validate_account(ticket, &account, source).await
                });
            }
            Effect::CheckSource {
                ticket,
                provider,
                source,
                key,
            } => self.bridge.spawn(failure, async move {
                inputs
                    .check_source(ticket, provider, &source, key.as_ref())
                    .await
            }),
            Effect::ProbeFolder { ticket, path } => {
                self.bridge.spawn(
                    failure,
                    async move { inputs.probe_folder(ticket, &path).await },
                );
            }
            Effect::ReadChanges {
                ticket,
                instance,
                unpushed,
                ..
            } => self.bridge.spawn(failure, async move {
                inputs.read_changes(ticket, &instance, unpushed).await
            }),
            Effect::LoadOp { ticket, scope } => self.bridge.spawn(failure, async move {
                Msg::OpLoaded {
                    ticket,
                    result: inputs.list_op(&scope).await,
                }
            }),
            Effect::Restore { instance } => self
                .bridge
                .spawn(failure, async move { inputs.restore(&instance).await }),
            Effect::PowerStart { instance } => self
                .bridge
                .spawn(failure, async move { inputs.power_start(&instance).await }),
            Effect::PowerStop { instance } => self
                .bridge
                .spawn(failure, async move { inputs.power_stop(&instance).await }),
            Effect::Prewarm { workspace } => self
                .bridge
                .spawn(failure, async move { inputs.prewarm(workspace).await }),
            Effect::Refresh { .. } | Effect::RunDoctor => {}
        }
    }

    pub(crate) fn refresh(&self) {
        self.bridge.send(Msg::RequestRefresh { force: true });
    }

    fn observe(&self, ticket: Ticket) {
        let inputs = Arc::clone(&self.inputs);
        let words = self.words.clone();
        self.bridge.spawn(crashed::refresh(ticket), async move {
            let result = match inputs.observe().await {
                Ok(observation) if observation.snapshot.index_error.is_some() => {
                    Err(observation.status(&words).unwrap_or_default())
                }
                Ok(observation) => Ok(Box::new(observation.snapshot)),
                Err(error) => Err(error.to_string()),
            };
            Msg::Refreshed { ticket, result }
        });
    }
}

impl Backend for JackinBackend {
    fn usage_connections(
        &self,
    ) -> Option<&dyn jackin_console::usage_connections::UsageConnections> {
        Some(self)
    }

    fn perform(&self, effect: &Effect, world: &World) -> Option<Completion> {
        match effect {
            Effect::Refresh { ticket } => {
                self.refreshed_at.store(world.now_secs(), Ordering::SeqCst);
                self.observe(*ticket);
                None
            }
            Effect::RunDoctor => {
                self.rerun_doctor();
                None
            }
            _ => {
                self.settle(effect.clone());
                None
            }
        }
    }

    fn due(&self, world: &World) -> Vec<Msg> {
        let now = world.now_secs();
        self.schedule_usage(now);
        let last = self.refreshed_at.load(Ordering::SeqCst);
        if last == NEVER {
            self.refreshed_at.store(now, Ordering::SeqCst);
        } else if now.saturating_sub(last) >= REFRESH_EVERY_SECS && !self.bridge.has_detached_work()
        {
            self.refreshed_at.store(now, Ordering::SeqCst);
            return vec![Msg::RequestRefresh { force: false }];
        }
        Vec::new()
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

    fn initial_panes(&self, instance: &Instance) -> Vec<PaneRequest> {
        if instance.status != InstanceStatus::Running {
            return Vec::new();
        }
        if instance.agents.is_empty() {
            return vec![PaneRequest::new(
                &instance.id,
                Some(instance.agent),
                instance.account.clone(),
            )];
        }
        instance
            .agents
            .iter()
            .map(|entry| {
                let account = entry.account.clone().or_else(|| {
                    (entry.agent == instance.agent).then(|| instance.account.clone())?
                });
                PaneRequest::new(&instance.id, Some(entry.agent), account)
            })
            .collect()
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

    fn spawnable(
        &self,
        world: &World,
        instance: &Instance,
        kind: &PaneKind,
    ) -> Result<(), PaneRefusal> {
        match kind {
            PaneKind::Agent { agent, .. } => {
                let open = world.daemons.get(&instance.id).is_some_and(|daemon| {
                    daemon
                        .panes
                        .iter()
                        .any(|pane| pane.agent.as_ref() == Some(agent))
                });
                if open {
                    Err(PaneRefusal::MainProcessOpen { agent: *agent })
                } else {
                    Ok(())
                }
            }
            PaneKind::Shell => Ok(()),
        }
    }

    fn capsule_left(&self, _world: &World, instance: &InstanceId) {
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
