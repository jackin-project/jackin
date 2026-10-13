// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent session spawning, codenames, and agent history.

use crate::tui::view::{spawn_failure_agent_label, spawn_failure_message};

use super::super::{AgentRecord, Multiplexer, PickerIntent, Result, Session, Tab};

impl Multiplexer {
    /// Pick the next available codename and record it as live.
    /// Increments `wordlist_offset` so consecutive tabs get different words.
    pub(crate) fn pick_next_codename(&mut self) -> String {
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
    pub(crate) fn retire_codename(&mut self, codename: &str) {
        let now = self.wall_now_utc();
        self.session_supervisor.retire_codename(codename, now);
    }

    pub(crate) fn mark_agent_session_exited(&mut self, session_id: u64) {
        let now = self.wall_now_utc();
        if let Some(record) = self
            .session_supervisor
            .agent_history
            .iter_mut()
            .rev()
            .find(|record| record.session_id == session_id)
        {
            record.exited_at.get_or_insert(now);
        }
    }

    /// Single dispatch point for `DialogAction::SpawnAgent`. Spawn
    /// failures (PTY allocation, missing agent binary, cap hit) are
    /// exported once as a typed error without a body; operator detail stays
    /// local to the dialog. The dialog dismisses regardless so the
    /// operator can retry.
    pub(crate) fn dispatch_spawn_intent(&mut self, agent: Option<String>, intent: PickerIntent) {
        let result: Result<()> = match intent {
            PickerIntent::NewTab => self.spawn_session(agent.clone(), &[], None).map(|_| ()),
            PickerIntent::Split(direction) => {
                self.split_focused_into(direction, agent.clone(), &[], None)
            }
        };
        if let Err(err) = result {
            let agent_label = spawn_failure_agent_label(agent.as_deref());
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
            );
            self.open_spawn_failure_dialog(spawn_failure_message(agent_label, &err));
        }
    }

    pub(crate) fn spawn_session(
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
        // Authoritative spawn gate: resolve the slug-or-ID target to its
        // admitted instance. Ambiguity and unknown targets fail here so
        // neither the TUI picker path nor a wire client can silently
        // substitute another account's instance.
        let agent = agent
            .map(|raw| {
                self.launch_env
                    .launch_config
                    .resolve_instance(&raw)
                    .map(str::to_owned)
                    .map_err(|reason| anyhow::anyhow!("rejected spawn target {raw:?}: {reason}"))
            })
            .transpose()?;
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
        )?;
        let account_id = agent.as_deref().and_then(|id| {
            self.launch_env
                .launch_config
                .account_for_instance(id)
                .map(str::to_owned)
        });
        let identity = agent
            .as_deref()
            .and_then(|id| self.launch_env.launch_config.identity_for_instance(id))
            .or(self.launch_env.launch_config.shell_identity)
            .ok_or_else(|| anyhow::anyhow!("spawn target has no isolated Unix identity"))?;
        let (mut session, id) = Session::spawn(
            crate::session::SessionSpawnSpec {
                label: launch.label.clone(),
                agent: agent.clone(),
                account_id: account_id.clone(),
                identity,
                provider: provider_label.map(|label| crate::session::SessionProvider {
                    label: label.to_owned(),
                    env_overrides: env_overrides.to_vec(),
                }),
                cache_dir: launch.cache_dir,
            },
            launch.cmd,
            self.session_terminal(
                self.render.content_rows.saturating_sub(2),
                self.render.term_cols.saturating_sub(2),
            ),
            self.control.event_tx.clone(),
        )?;
        session.usage_capability = agent
            .as_deref()
            .and_then(|id| {
                self.launch_env
                    .launch_config
                    .usage_capability_for_instance(id)
            })
            .cloned();
        let tab_label = launch.label.clone();
        self.session_supervisor.sessions.insert(id, session);
        let mut tab = Tab::new_single(tab_label, id, codename.clone());
        tab.instance = agent.clone();
        tab.account_id = account_id;
        if self.session_supervisor.tabs.is_empty() {
            self.session_supervisor.tabs.push(tab);
            self.session_supervisor.active_tab = 0;
        } else {
            self.session_supervisor.tabs.push(tab);
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
        Ok(id)
    }

    /// Append a session to the agent registry. Uses the explicit provider label
    /// when given; otherwise infers the default provider from the agent slug so
    /// the registry always shows a meaningful value. The owning account is
    /// resolved from the launch config's instance map (not from the
    /// credential envelope, which `sync` instances never populate), so both
    /// fresh spawns and splits stamp the same identity for one instance.
    pub(crate) fn record_agent_history(
        &mut self,
        session_id: u64,
        codename: String,
        agent: Option<String>,
        provider_label: Option<&str>,
    ) {
        // `agent` carries an instance config ID, not a slug: resolve the slug
        // for default-provider inference. Unknown IDs (hand-built test
        // sessions) match verbatim, mirroring `tab_display_label`.
        let slug = agent.as_deref().map(|stored| {
            self.launch_env
                .launch_config
                .agent_for_instance(stored)
                .unwrap_or(stored)
        });
        let provider = provider_label.map(str::to_owned).or_else(|| match slug {
            Some("claude") => Some("anthropic".to_owned()),
            Some("codex") => Some("openai".to_owned()),
            _ => None,
        });
        let account_id = agent.as_deref().and_then(|id| {
            self.launch_env
                .launch_config
                .account_for_instance(id)
                .map(str::to_owned)
        });
        let started_at = self.wall_now_utc();
        self.session_supervisor.agent_history.push(AgentRecord {
            session_id,
            codename,
            agent,
            account_id,
            provider,
            started_at,
            exited_at: None,
        });
    }
}
