// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

use jackin_console::backend::{Backend, Completion, Effect, FileSource, Ticket};
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
    refreshes: Refreshes,
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
            refreshes: Refreshes::default(),
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
                account, source, ..
            } => {
                self.bridge.spawn(failure, async move {
                    inputs.refresh_account(&account, source).await
                });
            }
            Effect::ValidateAccount { account, source } => {
                self.bridge.spawn(failure, async move {
                    inputs.validate_account(&account, source).await
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
            Effect::Prewarm { workspace } => self
                .bridge
                .spawn(failure, async move { inputs.prewarm(workspace).await }),
            Effect::Refresh { .. } | Effect::RunDoctor => {}
        }
    }

    pub(crate) fn refresh(&self) {
        self.begin(Cause::Requested);
    }

    fn begin(&self, cause: Cause) {
        if let Some(refresh) = self.refreshes.begin(cause) {
            self.observe(refresh);
        }
    }

    fn observe(&self, refresh: Refresh) {
        let Refresh { ticket, held } = refresh;
        let inputs = Arc::clone(&self.inputs);
        let words = self.words.clone();
        self.bridge.spawn(crashed::refresh(ticket), async move {
            let _held = held;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cause {
    Ticketed(Ticket),
    Requested,
    Periodic,
}

#[derive(Clone, Default)]
struct Refreshes {
    in_flight: Arc<AtomicUsize>,
    ticket: Arc<AtomicU64>,
}

struct Refresh {
    ticket: Ticket,
    held: Refreshing,
}

impl Refreshes {
    fn begin(&self, cause: Cause) -> Option<Refresh> {
        match cause {
            Cause::Ticketed(ticket) => {
                self.ticket.store(ticket, Ordering::SeqCst);
                Some(self.start(ticket))
            }
            Cause::Requested => Some(self.start(self.ticket.load(Ordering::SeqCst))),
            Cause::Periodic => {
                self.in_flight
                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .ok()?;
                Some(Refresh {
                    ticket: self.ticket.load(Ordering::SeqCst),
                    held: Refreshing(Arc::clone(&self.in_flight)),
                })
            }
        }
    }

    fn start(&self, ticket: Ticket) -> Refresh {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        Refresh {
            ticket,
            held: Refreshing(Arc::clone(&self.in_flight)),
        }
    }
}

struct Refreshing(Arc<AtomicUsize>);

impl Drop for Refreshing {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Backend for JackinBackend {
    fn perform(&self, effect: &Effect, world: &World) -> Option<Completion> {
        match effect {
            Effect::Refresh { ticket } => {
                self.refreshed_at.store(world.now_secs(), Ordering::SeqCst);
                self.begin(Cause::Ticketed(*ticket));
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
        let last = self.refreshed_at.load(Ordering::SeqCst);
        if last == NEVER {
            self.refreshed_at.store(now, Ordering::SeqCst);
        } else if now.saturating_sub(last) >= REFRESH_EVERY_SECS && !self.bridge.has_detached_work()
        {
            self.refreshed_at.store(now, Ordering::SeqCst);
            self.begin(Cause::Periodic);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panic_while_refreshing_lets_the_next_refresh_start() {
        let count = Arc::new(AtomicUsize::new(1));
        let held = Refreshing(Arc::clone(&count));
        let task = tokio::spawn(async move {
            let _held = held;
            panic!("observe crashed");
        });
        assert!(task.await.is_err_and(|error| error.is_panic()));
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_ticketed_or_requested_refresh_runs_whatever_is_in_flight_and_answers_the_latest_ticket() {
        let refreshes = Refreshes::default();
        let world = refreshes.begin(Cause::Ticketed(7));
        assert_eq!(world.as_ref().map(|refresh| refresh.ticket), Some(7));
        let requested = refreshes.begin(Cause::Requested);
        assert_eq!(requested.as_ref().map(|refresh| refresh.ticket), Some(7));
        let next = refreshes.begin(Cause::Ticketed(9));
        assert_eq!(next.as_ref().map(|refresh| refresh.ticket), Some(9));
        let again = refreshes.begin(Cause::Requested);
        assert_eq!(again.as_ref().map(|refresh| refresh.ticket), Some(9));
    }

    #[test]
    fn the_periodic_refresh_starts_only_when_no_refresh_is_in_flight() {
        let refreshes = Refreshes::default();
        let idle = refreshes.begin(Cause::Periodic);
        assert!(idle.is_some());
        assert!(refreshes.begin(Cause::Periodic).is_none());
        drop(idle);
        let world = refreshes.begin(Cause::Ticketed(4));
        assert!(refreshes.begin(Cause::Periodic).is_none());
        let requested = refreshes.begin(Cause::Requested);
        drop(world);
        assert!(refreshes.begin(Cause::Periodic).is_none());
        drop(requested);
        let periodic = refreshes.begin(Cause::Periodic);
        assert_eq!(periodic.map(|refresh| refresh.ticket), Some(4));
    }
}
