// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Focused usage snapshots, broker refresh scheduling, and usage views.

use super::super::{Dialog, Multiplexer};

impl Multiplexer {
    pub(crate) fn focused_usage_snapshot(&mut self) -> jackin_protocol::control::FocusedUsageView {
        self.focused_usage_snapshot_for_provider(None)
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

    pub(crate) fn focused_usage_status_label(&self) -> Option<String> {
        let (agent, provider, capability) = self.focused_agent_provider();
        self.usage
            .usage_cache
            .focused_status_bar_label_for_capability(
                agent.as_deref(),
                provider.as_deref(),
                capability.as_ref(),
            )
    }

    pub(crate) fn focused_usage_snapshot_for_provider(
        &mut self,
        provider_label: Option<&str>,
    ) -> jackin_protocol::control::FocusedUsageView {
        let (agent, provider, capability) = self.focused_agent_provider();
        if agent.is_none() && self.launch_env.available_instances.is_empty() {
            return jackin_protocol::control::FocusedUsageView::unavailable(
                "No agent instances configured for this Capsule.",
                chrono::Utc::now().timestamp(),
            );
        }
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| provider.as_ref().map(ToOwned::to_owned));
        self.usage.usage_cache.focused_snapshot_for_capability(
            agent.as_deref(),
            provider.as_deref(),
            capability.as_ref(),
        )
    }

    pub(crate) fn request_usage_refresh_for_provider(&mut self, provider_label: Option<&str>) {
        self.usage.pending_usage_refresh = self.usage_refresh_target_for_provider(provider_label);
        self.decorate_open_usage_dialog_refreshing();
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
            agent,
            provider,
            capability,
        })
    }

    pub(crate) fn spawn_active_usage_account_refresh(&mut self) -> bool {
        if self.usage.usage_refresh_task.is_some() {
            return false;
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
            return false;
        }
        self.usage.usage_refresh_task = Some(jackin_telemetry::spawn::joined_blocking(move || {
            let client = jackin_usage::host::UsageBrokerClient::scoped_relay();
            refresh_usage_targets_with_client(&client, active_targets, focused, manual.as_ref())
        }));
        true
    }

    pub(crate) async fn finish_usage_account_refresh_if_ready(&mut self) -> bool {
        let Some(task) = self.usage.usage_refresh_task.as_ref() else {
            return false;
        };
        if !task.is_finished() {
            return false;
        }
        let Some(task) = self.usage.usage_refresh_task.take() else {
            return false;
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
                false
            }
        }
    }

    pub(crate) fn refresh_open_usage_dialog_from_cache(&mut self) -> bool {
        let Some((selected, provider_label)) = self.open_usage_dialog_selection() else {
            return false;
        };
        let mut view = self.focused_usage_snapshot_for_provider(provider_label.as_deref());
        // "Refreshing" is derived from observable truth — a refresh task is
        // actually in flight — not from the `pending_usage_refresh` scheduling
        // flag (which lingered Some and stuck the marker onto Fresh data). The
        // decorate fn additionally no-ops on a Fresh snapshot, so a loaded view
        // is never annotated as refreshing (Bug 1).
        if self.usage.usage_refresh_task.is_some() {
            decorate_usage_view_refreshing(&mut view);
        }
        if let Some(Dialog::Usage {
            view: current,
            selected: current_selected,
            ..
        }) = self.dialog_top_mut()
        {
            if **current == view && *current_selected == selected {
                return false;
            }
            **current = view;
            *current_selected = selected;
            return true;
        }
        false
    }

    fn decorate_open_usage_dialog_refreshing(&mut self) {
        // Same truth source as `refresh_open_usage_dialog_from_cache`: only an
        // in-flight task drives the marker, never the scheduling flag (Bug 1).
        if self.usage.usage_refresh_task.is_none() {
            return;
        }
        if let Some(Dialog::Usage { view, .. }) = self.dialog_top_mut() {
            decorate_usage_view_refreshing(view);
        }
    }

    fn open_usage_dialog_selection(
        &self,
    ) -> Option<(
        crate::tui::components::dialog::UsageDialogTab,
        Option<String>,
    )> {
        let Dialog::Usage { view, selected, .. } = self.dialog_top()? else {
            return None;
        };
        let provider = (*selected == crate::tui::components::dialog::UsageDialogTab::Provider)
            .then(|| view.focused_provider.clone())
            .flatten();
        Some((*selected, provider))
    }
}

pub(crate) fn refresh_usage_targets_with_client(
    client: &jackin_usage::host::UsageBrokerClient,
    active_targets: Vec<crate::usage::UsageRefreshTarget>,
    focused: Option<crate::usage::UsageRefreshTarget>,
    manual: Option<&crate::usage::UsageRefreshTarget>,
) -> Vec<super::super::BrokerUsageRefresh> {
    let mut requests = std::collections::BTreeMap::new();
    for target in active_targets.into_iter().chain(focused) {
        let force = manual == Some(&target);
        requests
            .entry(target.capability.clone())
            .and_modify(|(_, existing_force)| *existing_force |= force)
            .or_insert((target, force));
    }
    requests
        .into_iter()
        .map(|(capability, (target, force))| {
            let result = client
                .current_for_capability(capability.clone())
                .and_then(|current| {
                    client.refresh_for_capability(capability.clone(), current.generation, force)
                })
                .and_then(|state| {
                    if state.phase.is_active() {
                        client.join_for_capability(
                            capability,
                            state.generation,
                            std::time::Duration::from_secs(30),
                        )
                    } else {
                        Ok(state)
                    }
                });
            super::super::BrokerUsageRefresh { target, result }
        })
        .collect()
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
                agent: agent.clone(),
                provider: session.provider.as_ref().map(|p| p.label.clone()),
                capability,
            })
    })
}

fn decorate_usage_view_refreshing(view: &mut jackin_protocol::control::FocusedUsageView) {
    // Never annotate a Fresh snapshot as "refreshing" — the marker is only for a
    // view that is still loading/stale while a refresh runs. A Fresh view that is
    // being re-fetched in the background updates its timestamp on completion
    // instead, so the status bar never reads the contradictory
    // `Updated just now · refreshing...` (Bug 1).
    if view.status == jackin_protocol::control::UsageSnapshotStatus::Fresh {
        return;
    }
    if !view.updated_label.contains("refreshing") {
        view.updated_label.push_str(" · refreshing...");
    }
}
