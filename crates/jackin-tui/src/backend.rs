// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_console::backend::{
    Backend, ChangeSet, Completion, Effect, FileSource, FolderProbe, OnePassword, ValidationOutcome,
};
use jackin_console::domain::account::CredentialSource as AccountSource;
use jackin_console::domain::agent::Provider;
use jackin_console::domain::instance::Instance;
use jackin_console::domain::launch::LaunchContent;
use jackin_console::launch::{LaunchFeed, LaunchTarget};
use jackin_console::panes::{PaneBody, PaneRequest, PaneSpawn};
use jackin_console::world::{Msg, World};

use crate::bridge::Bridge;
use crate::files::HostFiles;
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
            Effect::Refresh
            | Effect::RunDoctor
            | Effect::Restore { .. }
            | Effect::Prewarm { .. }
            | Effect::RefreshAccount { .. }
            | Effect::ValidateAccount { .. } => {}
        }
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
            Effect::Restore { instance } => unbuilt::restore(instance),
            Effect::Prewarm { workspace } => unbuilt::prewarm(*workspace),
            Effect::RefreshAccount { account, .. } => unbuilt::refresh_account(account),
            Effect::ValidateAccount { account } => unbuilt::validate_account(account),
            _ => {
                self.settle(effect.clone());
                None
            }
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
