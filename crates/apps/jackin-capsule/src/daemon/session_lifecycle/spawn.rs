// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Spawn requests, session launch construction, and pane home assignment.

use super::super::{
    Dialog, FullRedrawReason, Multiplexer, Result, SessionLaunch, SpawnRequest,
    build_agent_command, build_shell_command,
};

impl Multiplexer {
    pub(crate) fn open_spawn_failure_dialog(&mut self, message: String) {
        self.dialog_push(Dialog::SpawnFailure(
            crate::tui::components::dialog::SpawnFailureState::new("Spawn failed", message),
        ));
        self.invalidate(FullRedrawReason::DialogChange);
    }

    pub(crate) fn spawn_request(
        &mut self,
        request: SpawnRequest,
        env_overrides: &[(String, String)],
    ) -> Result<u64> {
        match request {
            SpawnRequest::Instance(target) => {
                let id = self.spawn_session(Some(target), env_overrides, None)?;
                self.note_agent_started();
                Ok(id)
            }
            SpawnRequest::Shell => self.spawn_session(None, env_overrides, None),
        }
    }

    /// P3: an agent session just spawned — the start moment the usage lifecycle
    /// hangs off. Kick a usage refresh now so the focused segment moves from
    /// `refreshing` to a real headline promptly rather than waiting for the next
    /// poll cycle. (The daemon already owns this moment via `SpawnRequest`, so no
    /// separate launch proxy is needed.)
    fn note_agent_started(&mut self) {
        self.spawn_active_usage_account_refresh();
    }

    /// Derived-root suffix for one more concurrent session of `instance`.
    /// The first live session keeps the launch-config home; every further
    /// concurrent session gets `{home}/panes/{seq}` with a daemon-monotonic
    /// `seq`, so concurrent panes never share one account's state root.
    /// The entrypoint seeds credentials into the derived root from the
    /// instance's forwarded dir, exactly like a fresh base home.
    fn assign_pane_home_seq(&mut self, instance: &str) -> Option<u64> {
        let live = self
            .session_supervisor
            .sessions
            .values()
            .any(|session| session.agent.as_deref() == Some(instance));
        if !live {
            return None;
        }
        let seq = self.pane_home_seq;
        self.pane_home_seq = self.pane_home_seq.wrapping_add(1);
        Some(seq)
    }

    pub(crate) fn session_launch(
        &mut self,
        instance: Option<&str>,
        provider_label: Option<&str>,
        env_passthrough: &[(String, String)],
        codename: &str,
    ) -> Result<SessionLaunch> {
        let derived_home_seq = instance.and_then(|id| self.assign_pane_home_seq(id));
        let cwd = self.launch_env.workdir.as_path();
        match instance {
            Some(instance) => {
                let config = &self.launch_env.launch_config;
                let slug = config.agent_for_instance(instance).ok_or_else(|| {
                    anyhow::anyhow!("instance {instance:?} has no agent runtime in launch config")
                })?;
                let base_home = config.home_for_instance(instance).ok_or_else(|| {
                    anyhow::anyhow!("instance {instance:?} has no home dir in launch config")
                })?;
                let derived_home;
                let home_dir = match derived_home_seq {
                    Some(seq) => {
                        derived_home = format!(
                            "{base_home}/{}/{seq}",
                            jackin_core::container_paths::PANE_HOMES_DIR_NAME
                        );
                        derived_home.as_str()
                    }
                    None => base_home,
                };
                let forwarded_dir = config.forwarded_for_instance(instance).ok_or_else(|| {
                    anyhow::anyhow!("instance {instance:?} has no forwarded dir in launch config")
                })?;
                let identity = config.identity_for_instance(instance).ok_or_else(|| {
                    anyhow::anyhow!("instance {instance:?} has no isolated Unix identity")
                })?;
                let label = crate::tui::model::visible_agent_label(
                    config.label_for_instance(instance),
                    Some(slug),
                    provider_label,
                );
                let mut cmd = build_agent_command(&crate::session::AgentSpawnSpec {
                    agent: slug,
                    instance,
                    home_dir,
                    forwarded_dir,
                    model: config.model_for_instance(instance),
                    effort: config.effort_for_instance(instance),
                    auth_mode: config.auth_mode_for_instance(instance),
                    env_passthrough,
                    cwd,
                    codename,
                    identity,
                });
                crate::session::apply_account_env(
                    &mut cmd,
                    instance,
                    config.auth_mode_for_instance(instance),
                    config.credential_provider_surface_for_instance(instance),
                    &self.launch_env.agent_credentials,
                );
                Ok(SessionLaunch {
                    label,
                    cmd,
                    cache_dir: config.cache_for_instance(instance).map(str::to_owned),
                })
            }
            None => Ok(SessionLaunch {
                label: crate::tui::model::visible_agent_label(None, None, None),
                cmd: build_shell_command(
                    env_passthrough,
                    cwd,
                    codename,
                    self.launch_env
                        .launch_config
                        .shell_identity
                        .ok_or_else(|| {
                            anyhow::anyhow!("launch config has no isolated shell identity")
                        })?,
                ),
                cache_dir: None,
            }),
        }
    }
}
