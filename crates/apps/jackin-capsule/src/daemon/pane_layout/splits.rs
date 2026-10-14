// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    Multiplexer, Result, Session, SplitDirection, SplitDirectionGeometry, SplitPosition,
    content_rect, split_spawn_inner_size,
};

impl Multiplexer {
    pub(crate) fn split_focused_into(
        &mut self,
        direction: SplitDirection,
        instance: Option<String>,
        env_overrides: &[(String, String)],
        provider_label: Option<&str>,
    ) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        let instance = instance
            .map(|raw| {
                self.launch_env
                    .launch_config
                    .resolve_instance(&raw)
                    .map(str::to_owned)
                    .map_err(|reason| anyhow::anyhow!("rejected spawn target {raw:?}: {reason}"))
            })
            .transpose()?;

        self.cancel_drag();
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
        else {
            return Ok(());
        };
        let tab_codename = tab.codename.clone();
        let from_id = tab.focused_id;
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let from_rect = tab
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == from_id)
            .map_or(content_rect, |(_, r)| r);
        let split_geometry = match direction {
            SplitDirection::Left | SplitDirection::Right => SplitDirectionGeometry::LeftRight,
            SplitDirection::Above | SplitDirection::Below => SplitDirectionGeometry::TopBottom,
        };
        let (spawn_rows, spawn_cols) = split_spawn_inner_size(split_geometry, from_rect);
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(
            instance.as_deref(),
            provider_label,
            &env_passthrough,
            &tab_codename,
        )?;
        let agent_for_history = instance.clone();
        let account_for_session = instance
            .as_deref()
            .and_then(|id| self.launch_env.launch_config.account_for_instance(id))
            .map(str::to_owned);
        let usage_capability = instance.as_deref().and_then(|id| {
            self.launch_env
                .launch_config
                .usage_capability_for_instance(id)
        });
        let identity = instance
            .as_deref()
            .and_then(|id| self.launch_env.launch_config.identity_for_instance(id))
            .or(self.launch_env.launch_config.shell_identity)
            .ok_or_else(|| anyhow::anyhow!("split target has no isolated Unix identity"))?;
        let (mut session, new_id) = Session::spawn(
            crate::session::SessionSpawnSpec {
                label: launch.label.clone(),
                agent: instance,
                account_id: account_for_session,
                identity,
                provider: provider_label.map(|label| crate::session::SessionProvider {
                    label: label.to_owned(),
                    env_overrides: env_overrides.to_vec(),
                }),
                cache_dir: launch.cache_dir,
            },
            launch.cmd,
            self.session_terminal(spawn_rows, spawn_cols),
            self.control.event_tx.clone(),
        )?;
        session.usage_capability = usage_capability.cloned();
        self.session_supervisor.sessions.insert(new_id, session);
        self.record_agent_history(
            new_id,
            tab_codename.clone(),
            agent_for_history,
            provider_label,
        );
        let tab = &mut self.session_supervisor.tabs[self.session_supervisor.active_tab];
        let placed = match direction {
            SplitDirection::Left => tab.tree.split_h(from_id, new_id, SplitPosition::Before),
            SplitDirection::Right => tab.tree.split_h(from_id, new_id, SplitPosition::After),
            SplitDirection::Above => tab.tree.split_v(from_id, new_id, SplitPosition::Before),
            SplitDirection::Below => tab.tree.split_v(from_id, new_id, SplitPosition::After),
        };
        if !placed {
            if let Some(orphan) = self.session_supervisor.sessions.remove(new_id) {
                orphan.terminate();
            }

            self.mark_agent_session_exited(new_id);
            let _warning = jackin_telemetry::record_recovered_degradation();
            return Ok(());
        }
        tab.focused_id = new_id;
        self.resize_panes();
        self.synthesise_focus_swap(Some(from_id), Some(new_id));
        Ok(())
    }

    pub(crate) fn split_focused(&mut self, direction: SplitDirection) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        let (instance, provider_env_overrides, provider_label) = self.focused_spawn_metadata();
        self.split_focused_into(
            direction,
            instance,
            &provider_env_overrides,
            provider_label.as_deref(),
        )
    }

    pub(crate) fn focused_spawn_metadata(
        &self,
    ) -> (Option<String>, Vec<(String, String)>, Option<String>) {
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
        else {
            return (None, Vec::new(), None);
        };
        let from_id = tab.focused_id;
        self.session_supervisor
            .sessions
            .get(from_id)
            .map_or((None, Vec::new(), None), |session| {
                let (env, label) = session.provider.as_ref().map_or_else(
                    || (Vec::new(), None),
                    |provider| (provider.env_overrides.clone(), Some(provider.label.clone())),
                );
                (session.agent.clone(), env, label)
            })
    }

    pub(crate) fn close_focused_pane(&mut self) {
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get_mut(self.session_supervisor.active_tab)
        else {
            return;
        };
        let id = tab.focused_id;
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();
        tab.tree.remove(id);
        if let Some(session) = self.session_supervisor.sessions.remove(id) {
            session.terminate();
        }

        tab.zoomed = tab.zoomed.filter(|&zid| zid != id);
        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            self.session_supervisor
                .tabs
                .remove(self.session_supervisor.active_tab);
            if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
                self.session_supervisor.active_tab =
                    self.session_supervisor.tabs.len().saturating_sub(1);
            }
        }
        self.mark_agent_session_exited(id);
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }
}
