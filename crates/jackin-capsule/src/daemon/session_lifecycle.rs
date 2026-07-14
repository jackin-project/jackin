// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session, tab, and pane lifecycle methods for the Multiplexer.

use crate::tui::view::{spawn_failure_agent_label, spawn_failure_message};

use super::{
    AgentRecord, Dialog, FullRedrawReason, Multiplexer, PickerIntent, Result, Session,
    SessionLaunch, SpawnRequest, Tab, Utc, build_agent_command, build_shell_command,
};

impl Multiplexer {
    pub(super) fn open_spawn_failure_dialog(&mut self, message: String) {
        self.dialog_push(Dialog::SpawnFailure(
            jackin_tui::components::ErrorPopupState::new("Spawn failed", message),
        ));
        self.invalidate(FullRedrawReason::DialogChange);
    }

    pub(super) fn active_tab_pane_count(&self) -> usize {
        self.session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)
            .map(|tab| tab.tree.all_ids().len())
            .unwrap_or_default()
    }

    /// Count of currently visible panes, mirroring [`Self::visible_panes`]
    /// without computing pane geometry or allocating the `VisiblePane` vec.
    /// A zoom collapses the layout to the single zoomed pane.
    pub(super) fn visible_pane_count(&self) -> usize {
        if self.active_zoomed_id().is_some() {
            1
        } else {
            self.session_supervisor
                .tabs
                .get(self.session_supervisor.active_tab)
                .map_or(0, |tab| tab.tree.leaf_count())
        }
    }

    pub(super) fn next_tab(&mut self) {
        if self.session_supervisor.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.session_supervisor.active_tab =
            (self.session_supervisor.active_tab + 1) % self.session_supervisor.tabs.len();
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    pub(super) fn prev_tab(&mut self) {
        if self.session_supervisor.tabs.is_empty() {
            return;
        }
        self.cancel_drag();
        let prev = self.active_focused_id();
        self.session_supervisor.active_tab = if self.session_supervisor.active_tab == 0 {
            self.session_supervisor.tabs.len() - 1
        } else {
            self.session_supervisor.active_tab - 1
        };
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    pub(super) fn jump_tab(&mut self, idx: usize) {
        if idx < self.session_supervisor.tabs.len() && idx != self.session_supervisor.active_tab {
            self.cancel_drag();
            let prev = self.active_focused_id();
            self.session_supervisor.active_tab = idx;
            self.synthesise_focus_swap(prev, self.active_focused_id());
        }
    }

    pub(super) fn close_focused_tab(&mut self) {
        if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
            return;
        }
        // Drop any in-flight selection / drag-resize anchored to a
        // pane in this tab — resize_panes below invalidates every
        // remaining pane's rect and removing the active tab swaps the
        // visible content entirely. Mirrors close_focused_pane and
        // remove_exited_session, which both call cancel_drag for the
        // same reason.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let tab_ids = self.session_supervisor.tabs[self.session_supervisor.active_tab]
            .tree
            .all_ids();
        let closed_codename = self.session_supervisor.tabs[self.session_supervisor.active_tab]
            .codename
            .clone();
        crate::clog!(
            "action: close_focused_tab tab_idx={} pane_count={}",
            self.session_supervisor.active_tab,
            tab_ids.len()
        );
        for id in tab_ids {
            if let Some(session) = self.session_supervisor.sessions.remove(&id) {
                self.mark_agent_session_exited(id);
                session.terminate();
            }
        }
        self.session_supervisor
            .tabs
            .remove(self.session_supervisor.active_tab);
        self.retire_codename(&closed_codename);
        if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
            self.session_supervisor.active_tab =
                self.session_supervisor.tabs.len().saturating_sub(1);
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    pub(super) fn exit_all_sessions(&mut self) {
        self.cancel_drag();
        crate::clog!(
            "action: exit_all_sessions session_count={} tab_count={}",
            self.session_supervisor.sessions.len(),
            self.session_supervisor.tabs.len()
        );
        for (_, session) in self.session_supervisor.sessions.drain() {
            session.terminate();
        }
        self.session_supervisor.tabs.clear();
        self.session_supervisor.active_tab = 0;
        self.clipboard.dialog_copy_feedback_deadline = None;
        self.render.hover_target = None;
    }

    /// Drop the session whose PTY just exited. Removes the pane from
    /// the owning tab's tree, focuses a sibling if any remain, and
    /// removes the tab itself when its last pane is gone. Same
    /// semantic as `close_focused_pane` but driven by the agent
    /// process exiting instead of an explicit operator action — keeps
    /// `○ Done` tabs from piling up after every agent quits.
    ///
    /// When the closed tab was the active one, focus moves to the
    /// tab on the **left**. Operator's mental model: exiting an
    /// agent should return them to whatever they were looking at
    /// before they opened that tab, not to the next-tab-to-the-right
    /// (which feels like a stack push).
    #[expect(
        clippy::excessive_nesting,
        reason = "Session-removal fn: per-tab reflow with nested drag/selection \
              cancellation + tab-index clamping. The nesting is the per-tab \
              reflow protocol."
    )]
    pub(super) fn remove_exited_session(&mut self, session_id: u64) {
        crate::clog!("action: remove_exited_session id={session_id}");
        // Any in-flight selection / drag-resize was anchored to a
        // pane that may be about to disappear (or whose siblings
        // are about to reflow). Drop both gestures so the next motion
        // event does not paint stale geometry. `cancel_drag` clears
        // selection + drag together; calling it unconditionally is
        // cheaper than per-field re-validation.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let owning_tab = self
            .session_supervisor
            .tabs
            .iter()
            .position(|t| t.tree.all_ids().contains(&session_id));
        if let Some(tab_idx) = owning_tab {
            let leaves = self.session_supervisor.tabs[tab_idx].tree.all_ids();
            let tab_is_empty = leaves.len() == 1 && leaves[0] == session_id;
            if tab_is_empty {
                // `PaneTree::remove` is a no-op on a top-level
                // `Leaf` (no parent split to collapse), so we drop
                // the tab here instead of calling it. Without this
                // branch the tab persists with a dangling session
                // id and the operator sees a `Done` tab they
                // cannot interact with.
                let was_active = tab_idx == self.session_supervisor.active_tab;
                let closed_codename = self.session_supervisor.tabs[tab_idx].codename.clone();
                self.session_supervisor.tabs.remove(tab_idx);
                // INV-D8: retire codename so tab labels drop the exited name.
                let remaining_live = self.session_supervisor.sessions.len().saturating_sub(1);
                use super::ports::{PORTS, StatusPort};
                if PORTS.should_retire_codename_on_exit(session_id, remaining_live) {
                    self.retire_codename(&closed_codename);
                }
                if was_active {
                    // Move to the tab on the left when it exists;
                    // otherwise stay at index 0 (the leftmost tab
                    // remaining, which was the next-right neighbour
                    // before the removal). `saturating_sub(1)`
                    // collapses both "go left" and "no-left, stay
                    // at 0" into the same expression. Clamp again
                    // so `active_tab` stays in bounds if the last
                    // tab in the strip just vanished.
                    self.session_supervisor.active_tab = tab_idx.saturating_sub(1);
                    if self.session_supervisor.active_tab >= self.session_supervisor.tabs.len() {
                        self.session_supervisor.active_tab =
                            self.session_supervisor.tabs.len().saturating_sub(1);
                    }
                } else if tab_idx < self.session_supervisor.active_tab {
                    // A non-active tab to the left of the active one
                    // vanished; shift `active_tab` down so it keeps
                    // pointing at the same tab.
                    self.session_supervisor.active_tab -= 1;
                }
            } else {
                self.session_supervisor.tabs[tab_idx]
                    .tree
                    .remove(session_id);
                if self.session_supervisor.tabs[tab_idx].focused_id == session_id {
                    let remaining = self.session_supervisor.tabs[tab_idx].tree.all_ids();
                    if let Some(&next_focus) = remaining.first() {
                        self.session_supervisor.tabs[tab_idx].focused_id = next_focus;
                    }
                }
            }
        }
        self.session_supervisor.sessions.remove(&session_id);
        self.mark_agent_session_exited(session_id);
        if let Some(tab_idx) = owning_tab
            && let Some(tab) = self.session_supervisor.tabs.get_mut(tab_idx)
        {
            tab.zoomed = tab.zoomed.filter(|&id| id != session_id);
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    pub(super) fn spawn_request(
        &mut self,
        request: SpawnRequest,
        env_overrides: &[(String, String)],
    ) -> Result<u64> {
        match request {
            SpawnRequest::Agent(agent_slug) => {
                if let Err(reason) = crate::session::validate_agent_slug(
                    &agent_slug,
                    &self.launch_env.available_agents,
                ) {
                    anyhow::bail!("rejected agent {agent_slug:?}: {reason}");
                }
                let id = self.spawn_session(Some(agent_slug), env_overrides, None)?;
                self.note_agent_started();
                Ok(id)
            }
            SpawnRequest::AgentWithProvider {
                slug,
                provider_label,
            } => {
                if let Err(reason) =
                    crate::session::validate_agent_slug(&slug, &self.launch_env.available_agents)
                {
                    anyhow::bail!("rejected agent {slug:?}: {reason}");
                }
                // Token is resolved container-side (not on the wire) from the
                // per-provider API key env; the host only sends the label.
                let resolved_env = if let Some(provider) =
                    jackin_protocol::Provider::from_label(&provider_label)
                {
                    self.provider_spawn_env(&slug, provider)
                } else {
                    crate::clog!(
                        "spawn: unknown provider label {provider_label:?}; no env redirect applied"
                    );
                    env_overrides.to_vec()
                };
                let id = self.spawn_session(Some(slug), &resolved_env, Some(&provider_label))?;
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
        self.spawn_active_usage_account_refresh(std::time::Instant::now());
    }

    pub(super) fn session_launch(
        &self,
        agent: Option<&str>,
        provider_label: Option<&str>,
        env_passthrough: &[(String, String)],
        codename: &str,
    ) -> SessionLaunch {
        let cwd = self.launch_env.workdir.as_path();
        match agent {
            Some(slug) => {
                let label = crate::tui::model::visible_agent_label(Some(slug), provider_label);
                SessionLaunch {
                    label,
                    cmd: build_agent_command(
                        slug,
                        self.launch_model(slug, provider_label),
                        env_passthrough,
                        cwd,
                        codename,
                    ),
                }
            }
            None => SessionLaunch {
                label: crate::tui::model::visible_agent_label(None, None),
                cmd: build_shell_command(env_passthrough, cwd, codename),
            },
        }
    }

    /// Pick the next available codename and record it as live.
    /// Increments `wordlist_offset` so consecutive tabs get different words.
    pub(super) fn pick_next_codename(&mut self) -> String {
        let codename = crate::wordlist::pick_codename(
            &self.session_supervisor.codename_live,
            &self.session_supervisor.codename_retired,
            self.session_supervisor.wordlist_offset,
        );
        self.session_supervisor.wordlist_offset =
            self.session_supervisor.wordlist_offset.wrapping_add(1);
        codename
    }

    /// Move a closed tab's codename from `live` to `retired` (so it is never
    /// reused this container lifetime) and stamp the matching history record.
    pub(super) fn retire_codename(&mut self, codename: &str) {
        self.session_supervisor.codename_live.remove(codename);
        self.session_supervisor
            .codename_retired
            .insert(codename.to_owned());
        if let Some(record) = self
            .session_supervisor
            .agent_history
            .iter_mut()
            .rev()
            .find(|r| r.codename == codename)
        {
            record.exited_at = Some(Utc::now());
        }
    }

    pub(super) fn mark_agent_session_exited(&mut self, session_id: u64) {
        if let Some(record) = self
            .session_supervisor
            .agent_history
            .iter_mut()
            .rev()
            .find(|record| record.session_id == session_id)
        {
            record.exited_at.get_or_insert_with(Utc::now);
        }
    }

    /// Single dispatch point for `DialogAction::SpawnAgent`. Spawn
    /// failures (PTY allocation, missing agent binary, cap hit) are
    /// clog'd with their intent and agent label so a `jackin load
    /// --debug` shows the cause; the dialog dismisses regardless so
    /// the operator can retry.
    pub(super) fn dispatch_spawn_intent(&mut self, agent: Option<String>, intent: PickerIntent) {
        let result: Result<()> = match intent {
            PickerIntent::NewTab => self.spawn_session(agent.clone(), &[], None).map(|_| ()),
            PickerIntent::Split(direction) => {
                self.split_focused_into(direction, agent.clone(), &[], None)
            }
        };
        if let Err(err) = result {
            let agent_label = spawn_failure_agent_label(agent.as_deref());
            crate::clog!("spawn ({intent:?}, agent={agent_label}) failed: {err:?}");
            self.open_spawn_failure_dialog(spawn_failure_message(agent_label, &err));
        }
    }

    pub(super) fn dispatch_spawn_intent_with_provider(
        &mut self,
        agent: Option<String>,
        intent: PickerIntent,
        env_overrides: &[(String, String)],
        provider_label: Option<&str>,
    ) {
        let result: Result<()> = match intent {
            PickerIntent::NewTab => self
                .spawn_session(agent.clone(), env_overrides, provider_label)
                .map(|_| ()),
            PickerIntent::Split(direction) => {
                self.split_focused_into(direction, agent.clone(), env_overrides, provider_label)
            }
        };
        if let Err(err) = result {
            let agent_label = spawn_failure_agent_label(agent.as_deref());
            crate::clog!("spawn ({intent:?}, agent={agent_label}) failed: {err:?}");
            self.open_spawn_failure_dialog(spawn_failure_message(agent_label, &err));
        }
    }

    pub(super) fn spawn_session(
        &mut self,
        agent: Option<String>,
        env_overrides: &[(String, String)],
        provider_label: Option<&str>,
    ) -> Result<u64> {
        // Bound the per-container surface so a runaway client (or an
        // operator mis-click loop) cannot allocate unbounded PTYs.
        // Each session retains ~SCROLLBACK_LEN lines of scrollback,
        // a master+slave PTY pair, and a child process — at MAX_TABS
        // sessions the container memory footprint is still well
        // under typical limits, but well past the size any operator
        // can usefully navigate.
        self.ensure_capacity_for_new_session(true)?;
        let codename = self.pick_next_codename();
        // Mirror split_focused_into: resize_panes below reflows every
        // pane's interior rect, and the new tab swaps the visible
        // content. Drop any in-flight gesture anchored to a now-stale
        // pane rect so the next mouse-motion does not paint selection
        // or splitter feedback against geometry that has moved.
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(
            agent.as_deref(),
            provider_label,
            &env_passthrough,
            &codename,
        );
        let (session, id) = Session::spawn(
            &launch.label,
            agent.clone(),
            provider_label.map(|label| crate::session::SessionProvider {
                label: label.to_owned(),
                env_overrides: env_overrides.to_vec(),
            }),
            launch.cmd,
            self.session_terminal(
                self.render.content_rows.saturating_sub(2),
                self.render.term_cols.saturating_sub(2),
            ),
            self.control.event_tx.clone(),
        )?;
        let tab_label = launch.label.clone();
        self.session_supervisor.sessions.insert(id, session);
        if self.session_supervisor.tabs.is_empty() {
            self.session_supervisor
                .tabs
                .push(Tab::new_single(tab_label, id, codename.clone()));
            self.session_supervisor.active_tab = 0;
        } else {
            self.session_supervisor
                .tabs
                .push(Tab::new_single(tab_label, id, codename.clone()));
            self.session_supervisor.active_tab = self.session_supervisor.tabs.len() - 1;
        }
        self.session_supervisor
            .codename_live
            .insert(codename.clone());
        self.record_agent_history(id, codename, agent.clone(), provider_label);
        // Reflow so the new pane's PTY gets the correct interior
        // dimensions (outer rect minus border rows/cols). Without
        // this, the session keeps its initial `content_rows ×
        // term_cols` guess and the agent draws its bottom rows
        // past the pane's bottom border.
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, Some(id));
        crate::clog!(
            "action: spawn_session id={id} agent={:?} label={label} tab_idx={tab_idx}",
            agent,
            label = launch.label,
            tab_idx = self.session_supervisor.active_tab
        );
        Ok(id)
    }

    /// Append a session to the agent registry. Uses the explicit provider label
    /// when given; otherwise infers the default provider from the agent slug so
    /// the registry always shows a meaningful value.
    pub(super) fn record_agent_history(
        &mut self,
        session_id: u64,
        codename: String,
        agent: Option<String>,
        provider_label: Option<&str>,
    ) {
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| match agent.as_deref() {
                Some("claude") => Some("anthropic".to_owned()),
                Some("codex") => Some("openai".to_owned()),
                _ => None,
            });
        self.session_supervisor.agent_history.push(AgentRecord {
            session_id,
            codename,
            agent,
            provider,
            started_at: Utc::now(),
            exited_at: None,
        });
    }

    pub(super) fn toggle_zoom(&mut self) {
        let Some(tab) = self
            .session_supervisor
            .tabs
            .get_mut(self.session_supervisor.active_tab)
        else {
            return;
        };
        let focused = tab.focused_id;
        let was_zoomed = tab
            .zoomed
            .is_some_and(|zoom_id| tab.tree.all_ids().contains(&zoom_id));
        tab.zoomed = if was_zoomed { None } else { Some(focused) };
        self.resize_panes();
        crate::clog!(
            "action: toggle_zoom from={was_zoomed} to={} focused={focused:?}",
            self.active_zoomed_id().is_some()
        );
    }
}
