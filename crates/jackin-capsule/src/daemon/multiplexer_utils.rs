// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Miscellaneous Multiplexer utility methods.

use super::{
    Dialog, FullRedrawReason, MAX_SESSIONS, MAX_TABS, Multiplexer, PaletteCloseLabel, Result,
    SESSION_ENV_PASSTHROUGH, SessionInfo,
};

impl Multiplexer {
    pub(super) fn env_for_spawn(&self, overrides: &[(String, String)]) -> Vec<(String, String)> {
        let mut env = self.launch_env.env_passthrough.clone();
        for (key, value) in overrides {
            if !SESSION_ENV_PASSTHROUGH.iter().any(|allowed| allowed == key) {
                continue;
            }
            if let Some((_, existing)) =
                env.iter_mut().find(|(existing_key, _)| existing_key == key)
            {
                *existing = value.clone();
            } else {
                env.push((key.clone(), value.clone()));
            }
        }
        env
    }

    pub(super) fn open_command_palette(&mut self) {
        let close_label = PaletteCloseLabel::for_pane_count(self.active_tab_pane_count());
        self.dialog_push(Dialog::new_command_palette(close_label));
    }

    /// Terminal geometry + identity for a new session's grid. The single
    /// construction point for `SessionTerminal` so both spawn paths (new tab,
    /// split) carry the attach client's reported colors.
    pub(super) fn session_terminal(&self, rows: u16, cols: u16) -> crate::session::SessionTerminal {
        crate::session::SessionTerminal {
            rows,
            cols,
            row_arena: self.render.terminal_row_arena.clone(),
            default_fg: self.client_registry.attached_terminal.default_fg,
            default_bg: self.client_registry.attached_terminal.default_bg,
        }
    }

    /// Re-apply the attached client's terminal colors to every live grid.
    /// Called on (re)attach: a container can be reattached from a terminal
    /// with a different palette, and agents that query OSC 10/11 later must
    /// see the current client's colors. A client that could not read its
    /// palette reports `None`, which keeps each grid's previous colors —
    /// the last known answer beats resetting to the baked-in default.
    pub(super) fn apply_client_colors_to_sessions(&mut self) {
        let fg = self.client_registry.attached_terminal.default_fg;
        let bg = self.client_registry.attached_terminal.default_bg;
        for session in self.session_supervisor.sessions.values_mut() {
            session.shadow_grid.set_reported_colors(fg, bg);
        }
    }

    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). All such paths must
    /// route through here so `MAX_TABS` / `MAX_SESSIONS` are enforced
    /// uniformly — runaway-mis-click defence. `add_tab=true` enforces
    /// both caps; `add_tab=false` enforces only `MAX_SESSIONS` because
    /// the caller is reusing an existing tab.
    pub(super) fn ensure_capacity_for_new_session(&self, add_tab: bool) -> Result<()> {
        if add_tab && self.session_supervisor.tabs.len() >= MAX_TABS {
            anyhow::bail!(crate::tui::view::tab_limit_failure_message(MAX_TABS));
        }
        if self.session_supervisor.sessions.len() >= MAX_SESSIONS {
            anyhow::bail!(crate::tui::view::pane_limit_failure_message(MAX_SESSIONS));
        }
        Ok(())
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    pub(super) fn no_live_sessions(&self) -> bool {
        self.session_supervisor.sessions.is_empty()
    }

    /// Record a state change that can affect the visible frame. Handlers
    /// only mutate state and call this; the render loop composes when the
    /// generation moved. `FirstAttach` and `Resize` additionally arm the
    /// wipe policy — the only two reasons whose next frame starts with a
    /// screen erase.
    pub(super) fn invalidate(&mut self, reason: FullRedrawReason) {
        self.render.frame_generation = self.render.frame_generation.wrapping_add(1);
        self.render.last_invalidate_reason = Some(reason);
        if matches!(
            reason,
            FullRedrawReason::FirstAttach | FullRedrawReason::Resize
        ) {
            self.render.wipe_pending = Some(reason);
        }
    }

    pub(super) fn has_pending_render(&self) -> bool {
        self.render.frame_generation != self.render.rendered_generation
    }

    pub(super) fn focused_usage_snapshot(&mut self) -> jackin_protocol::control::FocusedUsageView {
        let (agent, provider, capability) = self.focused_agent_provider();
        if agent.is_none() && self.launch_env.available_instances.is_empty() {
            return jackin_protocol::control::FocusedUsageView::unavailable(
                "No agent instances configured for this Capsule.",
                chrono::Utc::now().timestamp(),
            );
        }
        self.usage.usage_cache.focused_snapshot_for_capability(
            agent.as_deref(),
            provider.as_deref(),
            capability.as_ref(),
        )
    }

    /// Agent codename and provider label of the currently focused session.
    fn focused_agent_provider(
        &self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<jackin_protocol::usage_broker::UsageAccountCapability>,
    ) {
        self.active_focused_id()
            .and_then(|id| self.session_supervisor.sessions.get(id))
            .map_or((None, None, None), |session| {
                (
                    session.agent.clone(),
                    session.provider.as_ref().map(|p| p.label.clone()),
                    session.usage_capability.clone(),
                )
            })
    }

    pub(super) fn focused_usage_status_label(&self) -> Option<String> {
        let (agent, provider, capability) = self.focused_agent_provider();
        self.usage
            .usage_cache
            .focused_status_bar_label_for_capability(
                agent.as_deref(),
                provider.as_deref(),
                capability.as_ref(),
            )
    }

    pub(super) fn usage_projection_snapshot(
        &self,
    ) -> Option<&jackin_protocol::usage_broker::UsageProjectionV2> {
        self.usage.canonical_projection.as_ref()
    }

    pub(super) fn usage_projection_error(&self) -> Option<&str> {
        self.usage.canonical_projection_error.as_deref()
    }

    pub(super) fn adopt_usage_projection(
        &mut self,
        projection: jackin_protocol::usage_broker::UsageProjectionV2,
    ) -> bool {
        if projection.validate().is_err() {
            self.usage.canonical_projection_error =
                Some("Usage broker returned an invalid canonical projection.".to_owned());
            return true;
        }
        if self
            .usage
            .canonical_projection
            .as_ref()
            .is_some_and(|previous| {
                previous.broker_instance_id == projection.broker_instance_id
                    && previous.broker_generation > projection.broker_generation
            })
        {
            self.usage.canonical_projection_error =
                Some("Usage broker returned an older canonical projection.".to_owned());
            return true;
        }
        let changed = self.usage.canonical_projection.as_ref() != Some(&projection)
            || self.usage.canonical_projection_error.is_some();
        self.usage.canonical_projection = Some(projection);
        self.usage.canonical_projection_error = None;
        self.usage.canonical_projection_revoked = false;
        changed
    }

    pub(super) fn adopt_usage_projection_error(
        &mut self,
        error: jackin_protocol::usage_broker::UsageCoordinationError,
    ) -> bool {
        use jackin_protocol::usage_broker::UsageCoordinationErrorKind;

        let revoked = matches!(
            error.kind,
            UsageCoordinationErrorKind::Unauthorized | UsageCoordinationErrorKind::CatalogRevoked
        );
        let changed = self.usage.canonical_projection_error.as_ref() != Some(&error.message)
            || (revoked && !self.usage.canonical_projection_revoked);
        if revoked {
            self.usage.canonical_projection = None;
            // Only a later authorized publication can restore display authority.
            self.usage.canonical_projection_revoked = true;
        }
        self.usage.canonical_projection_error = Some(error.message);
        changed
    }

    pub(super) fn request_usage_refresh_for_provider(&mut self, provider_label: Option<&str>) {
        self.usage.pending_usage_refresh = self.usage_refresh_target_for_provider(provider_label);
    }

    fn usage_refresh_target_for_provider(
        &self,
        provider_label: Option<&str>,
    ) -> Option<crate::usage::UsageRefreshTarget> {
        let session = self
            .active_focused_id()
            .and_then(|id| self.session_supervisor.sessions.get(id))?;
        let agent = session.agent.clone()?;
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| session.provider.as_ref().map(|p| p.label.clone()));
        let capability = session.usage_capability.clone()?;
        Some(crate::usage::UsageRefreshTarget {
            instance_id: agent.clone(),
            agent,
            provider,
            capability,
        })
    }

    pub(super) fn spawn_active_usage_account_refresh(&mut self) -> bool {
        // Surface inventory is independent of live sessions and their refresh joins.
        let projection_spawned = if self.usage.projection_refresh_task.is_none() {
            self.usage.projection_refresh_task =
                Some(jackin_telemetry::spawn::joined_blocking(|| {
                    jackin_usage::host::UsageBrokerClient::scoped_relay()
                        .current_projection_for_surface()
                }));
            true
        } else {
            false
        };
        if self.usage.usage_refresh_task.is_some() {
            return projection_spawned;
        }
        let active_targets = self
            .session_supervisor
            .sessions
            .values()
            .filter_map(session_refresh_target)
            .collect::<Vec<_>>();
        let focused = self
            .active_focused_id()
            .and_then(|id| self.session_supervisor.sessions.get(id))
            .and_then(session_refresh_target);
        let manual = self.usage.pending_usage_refresh.take();
        let focused = manual.clone().or(focused);
        if active_targets.is_empty() && focused.is_none() {
            return projection_spawned;
        }
        self.usage.usage_refresh_task = Some(jackin_telemetry::spawn::joined_blocking(move || {
            let client = jackin_usage::host::UsageBrokerClient::scoped_relay();
            refresh_usage_targets_with_client(&client, active_targets, focused, manual.as_ref())
        }));
        true
    }

    pub(super) async fn finish_usage_account_refresh_if_ready(&mut self) -> bool {
        let mut projection_changed = false;
        if self
            .usage
            .projection_refresh_task
            .as_ref()
            .is_some_and(|task| task.is_finished())
            && let Some(task) = self.usage.projection_refresh_task.take()
        {
            projection_changed = match task.await {
                Ok(Ok(projection)) => self.adopt_usage_projection(projection),
                Ok(Err(error)) => self.adopt_usage_projection_error(error),
                Err(error) => {
                    let error_type = if error.is_panic() {
                        jackin_telemetry::schema::enums::ErrorType::Panic
                    } else {
                        jackin_telemetry::schema::enums::ErrorType::DependencyCancelled
                    };
                    let _error = jackin_telemetry::record_error(error_type);
                    self.usage.canonical_projection_error =
                        Some("Usage broker projection read failed.".to_owned());
                    true
                }
            };
        }
        let Some(task) = self.usage.usage_refresh_task.as_ref() else {
            return projection_changed;
        };
        if !task.is_finished() {
            return projection_changed;
        }
        let Some(task) = self.usage.usage_refresh_task.take() else {
            return projection_changed;
        };
        match task.await {
            Ok(refreshes) => {
                for refresh in refreshes {
                    match refresh.result {
                        Ok(state) => self
                            .usage
                            .usage_cache
                            .adopt_broker_generation(&refresh.target, &state),
                        Err(error) => self
                            .usage
                            .usage_cache
                            .adopt_broker_error(&refresh.target, &error),
                    }
                }
                true
            }
            Err(error) => {
                let error_type = if error.is_panic() {
                    jackin_telemetry::schema::enums::ErrorType::Panic
                } else {
                    jackin_telemetry::schema::enums::ErrorType::DependencyCancelled
                };
                let _error = jackin_telemetry::record_error(error_type);
                projection_changed
            }
        }
    }

    pub(super) fn refresh_open_usage_dialog_from_projection(&mut self) -> bool {
        let projection = self.usage.canonical_projection.clone();
        let error = self.usage.canonical_projection_error.clone();
        let revoked = self.usage.canonical_projection_revoked;
        let Some(dialog @ Dialog::Usage { .. }) = self.dialog_top_mut() else {
            return false;
        };
        let mut changed = if revoked {
            dialog.revoke_usage_projection(
                error.unwrap_or_else(|| "Usage inventory authorization was revoked.".to_owned()),
            )
        } else {
            dialog.apply_usage_snapshot(projection, error)
        };
        changed |= self.update_usage_refresh_availability();
        changed
    }

    pub(super) fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.session_supervisor
            .sessions
            .iter()
            .map(|(id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
                account_id: s.account_id.clone(),
                state: s.state,
                active: Some(id) == focused,
            })
            .collect()
    }

    /// Build a tab/pane tree snapshot for the host console's preview
    /// pane. The leaf order matches `PaneTree::leaves` so the operator
    /// sees panes in the same left-to-right / top-to-bottom order the
    /// multiplexer renders. Missing sessions (race against a kill)
    /// fall back to a placeholder so the snapshot still covers every
    /// leaf the tree references — the host UI can dim those rows.
    pub(super) fn tab_snapshots(&self) -> Vec<crate::protocol::control::TabSnapshot> {
        use crate::protocol::control::{PaneSnapshot, TabSnapshot};
        use crate::tui::layout::Rect;
        let placeholder_rect = Rect::new(0, 0, self.render.term_rows, self.render.term_cols);
        self.session_supervisor
            .tabs
            .iter()
            .map(|tab| {
                let panes = tab
                    .tree
                    .leaves(placeholder_rect)
                    .into_iter()
                    .map(|(id, _)| match self.session_supervisor.sessions.get(id) {
                        Some(session) => PaneSnapshot {
                            session_id: id,
                            label: session.label.clone(),
                            agent: session.agent.clone(),
                            account_id: session.account_id.clone(),
                            state: session.state,
                            agent_status_report: Some(session.status.report(session.agent.clone())),
                        },
                        None => PaneSnapshot {
                            session_id: id,
                            label: "(missing)".to_owned(),
                            agent: None,
                            account_id: None,
                            state: crate::protocol::control::AgentState::Idle,
                            agent_status_report: None,
                        },
                    })
                    .collect();
                TabSnapshot {
                    label: tab.label_owned(),
                    instance: tab.instance.clone(),
                    account_id: tab.account_id.clone(),
                    focused_pane: tab.focused_id,
                    panes,
                }
            })
            .collect()
    }

    /// Snapshot the agent history for the control-channel `Agents` query.
    /// Active agents have `exited_at == None`; exited agents have a timestamp.
    pub(super) fn agent_registry_snapshot(
        &self,
    ) -> Vec<jackin_protocol::control::AgentRegistryEntry> {
        self.session_supervisor
            .agent_history
            .iter()
            .map(|r| jackin_protocol::control::AgentRegistryEntry {
                codename: r.codename.clone(),
                agent: r.agent.clone(),
                account_id: r.account_id.clone(),
                provider: r.provider.clone(),
                started_at: r.started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                exited_at: r
                    .exited_at
                    .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                status: if r.exited_at.is_some() {
                    "exited".to_owned()
                } else {
                    "active".to_owned()
                },
                // is_self is determined client-side from JACKIN_AGENT_CODENAME.
                is_self: false,
            })
            .collect()
    }
}

pub(super) fn refresh_usage_targets_with_client(
    client: &jackin_usage::host::UsageBrokerClient,
    active_targets: Vec<crate::usage::UsageRefreshTarget>,
    focused: Option<crate::usage::UsageRefreshTarget>,
    manual: Option<&crate::usage::UsageRefreshTarget>,
) -> Vec<super::BrokerUsageRefresh> {
    let mut requests = std::collections::BTreeMap::new();
    for target in active_targets.into_iter().chain(focused) {
        let force = manual.is_some_and(|manual| {
            manual.instance_id == target.instance_id && manual.capability == target.capability
        });
        requests
            .entry((target.instance_id.clone(), target.capability.clone()))
            .and_modify(|(_, existing_force)| *existing_force |= force)
            .or_insert((target, force));
    }
    // Admit every distinct session scope before any generation wait can block.
    let mut refreshes = requests
        .into_iter()
        .map(|((instance_id, capability), (target, force))| {
            let result = client
                .current_for_capability(capability.clone(), instance_id.clone())
                .and_then(|current| {
                    client.refresh_for_capability(
                        capability,
                        instance_id,
                        current.generation,
                        force,
                    )
                });
            super::BrokerUsageRefresh { target, result }
        })
        .collect::<Vec<_>>();
    // Independent bounded waits preserve request order without serializing accounts.
    std::thread::scope(|scope| {
        for refresh in &mut refreshes {
            if let Ok(state) = &refresh.result
                && state.phase.is_active()
            {
                let generation = state.generation;
                scope.spawn(move || {
                    refresh.result = client.join_for_capability(
                        refresh.target.capability.clone(),
                        refresh.target.instance_id.clone(),
                        generation,
                        std::time::Duration::from_secs(30),
                    );
                });
            }
        }
    });
    refreshes
}

/// Build a usage refresh target from a session, if it has an agent codename.
fn session_refresh_target(
    session: &crate::session::Session,
) -> Option<crate::usage::UsageRefreshTarget> {
    session.agent.as_ref().and_then(|agent| {
        session
            .usage_capability
            .clone()
            .map(|capability| crate::usage::UsageRefreshTarget {
                instance_id: agent.clone(),
                agent: agent.clone(),
                provider: session.provider.as_ref().map(|p| p.label.clone()),
                capability,
            })
    })
}
