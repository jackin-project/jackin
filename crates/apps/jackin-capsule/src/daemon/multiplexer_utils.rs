// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Miscellaneous Multiplexer utility methods.

use super::{FullRedrawReason, Multiplexer, SessionInfo};

mod spawn_support;
mod usage_views;

#[cfg(test)]
pub(crate) use usage_views::refresh_usage_targets_with_client;

impl Multiplexer {
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
