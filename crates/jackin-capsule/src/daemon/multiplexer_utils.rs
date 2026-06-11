//! Miscellaneous Multiplexer utility methods.

use std::collections::BTreeMap;
use std::time::Instant;

use super::{
    Dialog, FullRedrawReason, MAX_SESSIONS, MAX_TABS, Multiplexer, PaletteCloseLabel, Result,
    SESSION_ENV_PASSTHROUGH, SessionInfo,
};

impl Multiplexer {
    pub(super) fn env_for_spawn(&self, overrides: &[(String, String)]) -> Vec<(String, String)> {
        let mut env = self.env_passthrough.clone();
        for (key, value) in overrides {
            if !SESSION_ENV_PASSTHROUGH.iter().any(|allowed| allowed == key) {
                crate::clog!("spawn env: rejected non-allowlisted key {key:?}");
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

    pub(super) fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.launch_config.model_for_agent(agent)
    }

    /// Providers selectable for `agent`. An empty vec means only the
    /// default provider is available and no picker step is needed; a
    /// non-empty vec always has 2+ entries (enforced by the catalog).
    ///
    /// The provider match is intentionally contained in this availability
    /// closure because the Capsule owns the in-memory key slots; the catalog
    /// owns the iteration/filtering, while this closure answers "is this
    /// already captured for this running container?"
    pub(super) fn providers_for_agent(
        &self,
        agent: Option<&str>,
    ) -> Vec<jackin_protocol::Provider> {
        jackin_protocol::Provider::available_for(agent.unwrap_or_default(), |p| {
            self.provider_keys.contains_key(&p)
        })
    }

    /// Resolve the container-side API key for `provider` from the operator
    /// env captured at construction. The host sends only the provider label;
    /// the token never travels the wire. `None` means the key was unset, in
    /// which case the session falls back to the agent's default auth.
    pub(super) fn token_for_provider(&self, provider: jackin_protocol::Provider) -> Option<&str> {
        self.provider_keys.get(&provider).map(String::as_str)
    }

    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). All such paths must
    /// route through here so `MAX_TABS` / `MAX_SESSIONS` are enforced
    /// uniformly — runaway-mis-click defence. `add_tab=true` enforces
    /// both caps; `add_tab=false` enforces only `MAX_SESSIONS` because
    /// the caller is reusing an existing tab.
    pub(super) fn ensure_capacity_for_new_session(&self, add_tab: bool) -> Result<()> {
        if add_tab && self.tabs.len() >= MAX_TABS {
            anyhow::bail!(crate::tui::view::tab_limit_failure_message(MAX_TABS));
        }
        if self.sessions.len() >= MAX_SESSIONS {
            anyhow::bail!(crate::tui::view::pane_limit_failure_message(MAX_SESSIONS));
        }
        Ok(())
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    pub(super) fn no_live_sessions(&self) -> bool {
        self.sessions.is_empty()
    }

    pub(super) fn request_full_redraw(&mut self, reason: FullRedrawReason) {
        self.pending_full_redraw = Some(reason);
        self.pending_diff_redraw = None;
        self.dirty_panes.clear();
    }

    pub(super) fn request_diff_redraw(&mut self, reason: FullRedrawReason) {
        if self.pending_full_redraw.is_none() {
            self.pending_diff_redraw = Some(reason);
        }
    }

    pub(super) fn has_pending_render(&self) -> bool {
        self.pending_full_redraw.is_some()
            || self.pending_diff_redraw.is_some()
            || !self.dirty_panes.is_empty()
    }

    pub(super) fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions
            .iter()
            .map(|(&id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
                state: s.state(),
                active: Some(id) == focused,
                token_usage: self
                    .token_monitor
                    .totals(id)
                    .map(super::super::token_monitor::TokenTotals::to_summary),
                agent_status_report: Some(s.status.report(
                    s.agent.clone(),
                    if s.status.seen { s.status.revision } else { 0 },
                )),
            })
            .collect()
    }

    pub(super) fn visible_text_snapshots(&self) -> BTreeMap<u64, Vec<String>> {
        self.sessions
            .iter()
            .map(|(&id, session)| (id, session.visible_lines()))
            .collect()
    }

    pub(super) fn status_explain_snapshots(&self) -> BTreeMap<u64, serde_json::Value> {
        let now = Instant::now();
        self.sessions
            .iter()
            .map(|(&id, session)| {
                let visible_lines = session.visible_lines();
                let summary = &session.status.last_snapshot_summary;
                let gate = session.hook_authority.as_ref().and_then(|authority| {
                    self.runtime_gate_states
                        .get(&format!("{id}:{}", authority.source_id))
                });
                let report = session.status.report(
                    session.agent.clone(),
                    if session.status.seen {
                        session.status.revision
                    } else {
                        0
                    },
                );
                let watchdog_demoted = summary
                    .has_note(crate::agent_status::evidence::EvidenceNote::WatchdogDemoted);
                let value = serde_json::json!({
                    "session_id": id,
                    "label": session.label,
                    "agent": session.agent,
                    "effective": session.status.effective.label(),
                    "raw": session.status.raw.label(),
                    "seen": session.status.seen,
                    "revision": session.status.revision,
                    "status_report": report,
                    "evidence": {
                        "winner": format!("{:?}", summary.winner).to_ascii_lowercase(),
                        "confidence": format!("{:?}", summary.confidence).to_ascii_lowercase(),
                        "rule_id": summary.rule_id,
                        "authority_source": summary.authority_source,
                        "foreground_pgid": summary.foreground_pgid,
                        "activity": {
                            "last_output_ms_ago": summary.last_output.map(|at| now.saturating_duration_since(at).as_millis()),
                            "last_input_ms_ago": summary.last_input.map(|at| now.saturating_duration_since(at).as_millis()),
                        },
                        "process": {
                            "child_process_count": summary.child_process_count,
                            "cpu_jiffies_delta": summary.cpu_jiffies_delta,
                            "process_exited": summary.process_exited,
                            "foreground_returned_to_shell": summary.foreground_returned_to_shell,
                            "root_is_agent": summary.root_is_agent,
                        },
                        "screen": {
                            "visible_blocker": summary.visible_blocker,
                            "visible_idle": summary.visible_idle,
                            "visible_working": summary.visible_working,
                        },
                        "osc": {
                            "progress_active": summary.osc_progress_active,
                            "title": session.osc_evidence.title,
                            "title_changed_ms_ago": session.osc_evidence.title_changed_at.map(|at| now.saturating_duration_since(at).as_millis()),
                            "notify_edge_ms_ago": session.osc_evidence.notify_edge_at.map(|at| now.saturating_duration_since(at).as_millis()),
                            "progress_cleared_ms_ago": session.osc_evidence.progress_cleared_at.map(|at| now.saturating_duration_since(at).as_millis()),
                            "bel_ms_ago": session.osc_evidence.bel_at.map(|at| now.saturating_duration_since(at).as_millis()),
                            "bel_count": session.osc_evidence.bel_count,
                            "shell_state": session.osc_evidence.shell_state.map(jackin_protocol::agent_status::AgentRawState::label),
                            "shell_mark_ms_ago": session.osc_evidence.shell_mark_at.map(|at| now.saturating_duration_since(at).as_millis()),
                        },
                        "subagents_active": summary.subagents_active,
                        "stale_report": summary.stale_report,
                        "notes": summary.notes.iter().map(|note| format!("{note:?}").to_ascii_lowercase()).collect::<Vec<_>>(),
                    },
                    "stuck": {
                        "active": watchdog_demoted,
                        "reason": if watchdog_demoted { Some("watchdog_demoted") } else { None },
                        "last_output_ms_ago": summary.last_output.map(|at| now.saturating_duration_since(at).as_millis()),
                        "cpu_jiffies_delta": summary.cpu_jiffies_delta,
                        "child_process_count": summary.child_process_count,
                        "authority_source": summary.authority_source,
                        "foreground_pgid": summary.foreground_pgid,
                        "evidence_winner": format!("{:?}", summary.winner).to_ascii_lowercase(),
                        "notes": summary.notes.iter().map(|note| format!("{note:?}").to_ascii_lowercase()).collect::<Vec<_>>(),
                    },
                    "authority": session.hook_authority.as_ref().map(|authority| serde_json::json!({
                        "source_id": authority.source_id,
                        "agent_label": authority.agent_label,
                        "grade": format!("{:?}", super::authority_grade_for_runtime(&authority.agent_label)).to_ascii_lowercase(),
                        "origin": authority.origin.label(),
                        "raw_state": authority.raw_state,
                        "seq": authority.seq,
                        "last_seen_ms_ago": now.saturating_duration_since(authority.last_seen).as_millis(),
                    })),
                    "gate": gate.map(|gate| serde_json::json!({
                        "pending_permission": gate.pending_permission,
                        "subagents_active": gate.subagents_active,
                        "notes": gate.notes.iter().map(|note| format!("{note:?}").to_ascii_lowercase()).collect::<Vec<_>>(),
                    })),
                    "debounce": {
                        "candidate": session.pending_status_transition.candidate.map(jackin_protocol::control::AgentState::label),
                        "confirmations": session.pending_status_transition.confirmations,
                    },
                    "rules": self.rule_packs.explain_with_virtuals(
                        session.agent.as_deref(),
                        &visible_lines,
                        super::status_rule_virtual_regions(session, true),
                    ),
                    "visible": {
                        "lines": visible_lines,
                    },
                });
                (id, value)
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
        let placeholder_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        self.tabs
            .iter()
            .map(|tab| {
                let panes = tab
                    .tree
                    .leaves(placeholder_rect)
                    .into_iter()
                    .map(|(id, _)| match self.sessions.get(&id) {
                        Some(session) => PaneSnapshot {
                            session_id: id,
                            label: session.label.clone(),
                            agent: session.agent.clone(),
                            state: session.state(),
                            agent_status_report: Some(session.status.report(
                                session.agent.clone(),
                                if session.status.seen {
                                    session.status.revision
                                } else {
                                    0
                                },
                            )),
                        },
                        None => PaneSnapshot {
                            session_id: id,
                            label: "(missing)".to_owned(),
                            agent: None,
                            state: crate::protocol::control::AgentState::Idle,
                            agent_status_report: None,
                        },
                    })
                    .collect();
                TabSnapshot {
                    label: tab.label_owned(),
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
        self.agent_history
            .iter()
            .map(|r| jackin_protocol::control::AgentRegistryEntry {
                codename: r.codename.clone(),
                agent: r.agent.clone(),
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
