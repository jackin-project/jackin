//! Miscellaneous Multiplexer utility methods.

use super::{
    Dialog, FullRedrawReason, Instant, MAX_SESSIONS, MAX_TABS, Multiplexer, PaletteCloseLabel,
    Result, SESSION_ENV_PASSTHROUGH, SessionInfo,
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

    /// Terminal geometry + identity for a new session's grid. The single
    /// construction point for `SessionTerminal` so both spawn paths (new tab,
    /// split) carry the attach client's reported colors.
    pub(super) fn session_terminal(&self, rows: u16, cols: u16) -> crate::session::SessionTerminal {
        crate::session::SessionTerminal {
            rows,
            cols,
            row_arena: self.terminal_row_arena.clone(),
            default_fg: self.attached_terminal.default_fg,
            default_bg: self.attached_terminal.default_bg,
        }
    }

    /// Re-apply the attached client's terminal colors to every live grid.
    /// Called on (re)attach: a container can be reattached from a terminal
    /// with a different palette, and agents that query OSC 10/11 later must
    /// see the current client's colors. A client that could not read its
    /// palette reports `None`, which keeps each grid's previous colors —
    /// the last known answer beats resetting to the baked-in default.
    pub(super) fn apply_client_colors_to_sessions(&mut self) {
        let fg = self.attached_terminal.default_fg;
        let bg = self.attached_terminal.default_bg;
        for session in self.sessions.values_mut() {
            session.shadow_grid.set_reported_colors(fg, bg);
        }
    }

    pub(super) fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.launch_config.model_for_agent(agent)
    }

    /// Model the agent launches with. `OpenCode` has no model of its own, so a
    /// picked alt provider supplies it via its `<provider>/<model>` string
    /// (the `-m` flag); without it `OpenCode` falls back to its default provider
    /// block. Every other agent uses the role-manifest model and the provider
    /// only redirects auth env.
    ///
    /// Resolution for `OpenCode` + a picked provider: the role manifest's
    /// `[opencode.providers.<id>].model` override, then the provider's built-in
    /// default, then the agent's role-manifest model.
    pub(super) fn launch_model(&self, agent: &str, provider_label: Option<&str>) -> Option<&str> {
        if let Some(provider) = provider_label
            .filter(|_| agent == "opencode")
            .and_then(jackin_protocol::Provider::from_label)
        {
            if let Some(model) = self
                .launch_config
                .provider_model(agent, provider.manifest_id())
            {
                return Some(model);
            }
            if let Some(model) = provider.opencode_model() {
                return Some(model);
            }
        }
        self.model_for_agent(agent)
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

    /// Resolve a known provider to the spawn env: its `env_overrides` plus, for
    /// Codex with a resolved key, the `JACKIN_CODEX_PROFILE` activation. Both
    /// the host-initiated `AgentWithProvider` spawn and the in-container
    /// provider picker route through here so the Codex `--profile` wiring
    /// cannot drift between the two paths.
    pub(super) fn provider_spawn_env(
        &self,
        agent_slug: &str,
        provider: jackin_protocol::Provider,
    ) -> Vec<(String, String)> {
        let token = self.token_for_provider(provider);
        if token.is_none() && provider.adapter().needs_key_for_agent(agent_slug) {
            crate::clog!(
                "spawn: provider {:?} selected but its API key is unresolved in container; session falls back to the agent's default auth",
                provider.label()
            );
        }
        let mut env = provider.env_overrides(token);
        // Codex activates an alt provider through a v2 `--profile`. Inject the
        // profile name only when the key resolved: runtime-setup writes the
        // profile file (`minimax.config.toml`) only when the key is present, so
        // pushing the flag without it would make `codex --profile` fail on a
        // missing file instead of falling back to native auth.
        if agent_slug == "codex"
            && token.is_some()
            && let Some(profile) = provider.codex_profile()
        {
            env.push(("JACKIN_CODEX_PROFILE".to_owned(), profile.to_owned()));
        }
        // Claude maps a provider to a model through the `ANTHROPIC_DEFAULT_*_MODEL`
        // env the provider injects. A role's `[claude.providers.<id>].model`
        // override replaces those defaults so the operator can pin a different
        // model for that provider without editing the agent default.
        if agent_slug == "claude"
            && let Some(model) = self
                .launch_config
                .provider_model(agent_slug, provider.manifest_id())
        {
            for (key, value) in &mut env {
                if key.starts_with("ANTHROPIC_DEFAULT_") && key.ends_with("_MODEL") {
                    *value = model.to_owned();
                }
            }
        }
        env
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

    /// Record a state change that can affect the visible frame. Handlers
    /// only mutate state and call this; the render loop composes when the
    /// generation moved. `FirstAttach` and `Resize` additionally arm the
    /// wipe policy — the only two reasons whose next frame starts with a
    /// screen erase.
    pub(super) fn invalidate(&mut self, reason: FullRedrawReason) {
        self.frame_generation = self.frame_generation.wrapping_add(1);
        self.last_invalidate_reason = Some(reason);
        // FirstAttach/Resize always reset the diff baseline. A dialog shown with
        // no live panes (the dirty-exit modal) owns the whole screen, so force a
        // full repaint on its changes too: the incremental cursor-addressed diff
        // is fragile through nested PTY proxies (e.g. an outer wrapper) and can
        // silently drop a selection-move update, leaving the modal looking
        // frozen. A zero-pane modal has no pane content to preserve, so a full
        // repaint is cheap and guarantees the change is visible.
        let exit_modal_open = matches!(
            self.dialog_top(),
            Some(Dialog::ExitDirty { .. } | Dialog::ExitInspect { .. })
        );
        let force_full_repaint = matches!(
            reason,
            FullRedrawReason::FirstAttach | FullRedrawReason::Resize
        ) || (matches!(reason, FullRedrawReason::DialogChange)
            && exit_modal_open);
        if force_full_repaint {
            self.wipe_pending = Some(reason);
        }
        crate::cdebug!(
            "invalidate: reason={} generation={}",
            reason.as_str(),
            self.frame_generation,
        );
    }

    pub(super) fn has_pending_render(&self) -> bool {
        self.frame_generation != self.rendered_generation
    }

    pub(super) fn focused_usage_snapshot(&mut self) -> jackin_protocol::control::FocusedUsageView {
        self.focused_usage_snapshot_for_provider(None)
    }

    /// Agent codename and provider label of the currently focused session.
    fn focused_agent_provider(&self) -> (Option<String>, Option<String>) {
        self.active_focused_id()
            .and_then(|id| self.sessions.get(&id))
            .map_or((None, None), |session| {
                (
                    session.agent.clone(),
                    session.provider.as_ref().map(|p| p.label.clone()),
                )
            })
    }

    pub(super) fn focused_usage_status_label(&self) -> Option<String> {
        let (agent, provider) = self.focused_agent_provider();
        self.usage_cache
            .focused_status_bar_label(agent.as_deref(), provider.as_deref())
    }

    pub(super) fn focused_usage_snapshot_for_provider(
        &mut self,
        provider_label: Option<&str>,
    ) -> jackin_protocol::control::FocusedUsageView {
        let (agent, provider) = self.focused_agent_provider();
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| provider.as_ref().map(ToOwned::to_owned));
        self.usage_cache
            .focused_snapshot(agent.as_deref(), provider.as_deref())
    }

    pub(super) fn request_usage_refresh_for_provider(&mut self, provider_label: Option<&str>) {
        self.pending_usage_refresh = self.usage_refresh_target_for_provider(provider_label);
        if let Some(target) = &self.pending_usage_refresh {
            self.usage_cache
                .request_account_refresh(target, Instant::now());
        }
        self.decorate_open_usage_dialog_refreshing();
    }

    fn usage_refresh_target_for_provider(
        &self,
        provider_label: Option<&str>,
    ) -> Option<crate::usage::UsageRefreshTarget> {
        let (agent, provider) = self.focused_agent_provider();
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| provider.as_ref().map(ToOwned::to_owned));
        agent.map(|agent| crate::usage::UsageRefreshTarget { agent, provider })
    }

    pub(super) fn spawn_active_usage_account_refresh(&mut self, now: Instant) -> bool {
        if self.usage_refresh_task.is_some() {
            return false;
        }
        let active_targets = self
            .sessions
            .values()
            .filter_map(session_refresh_target)
            .collect::<Vec<_>>();
        let focused = self
            .active_focused_id()
            .and_then(|id| self.sessions.get(&id))
            .and_then(session_refresh_target);
        let focused = self.pending_usage_refresh.take().or(focused);
        if active_targets.is_empty() && focused.is_none() {
            return false;
        }
        let provider_keys = self.provider_keys.clone();
        let mut cache = self.usage_cache.clone();
        self.usage_refresh_task = Some(tokio::task::spawn_blocking(move || {
            cache.refresh_active_account_snapshots(&active_targets, focused, &provider_keys, now);
            cache
        }));
        true
    }

    pub(super) async fn finish_usage_account_refresh_if_ready(&mut self, now: Instant) -> bool {
        let Some(task) = self.usage_refresh_task.as_ref() else {
            return false;
        };
        if !task.is_finished() {
            return false;
        }
        let Some(task) = self.usage_refresh_task.take() else {
            return false;
        };
        match task.await {
            Ok(cache) => {
                self.usage_cache = cache;
                if let Some(target) = &self.pending_usage_refresh {
                    self.usage_cache.request_account_refresh(target, now);
                }
                true
            }
            Err(error) => {
                crate::clog!("usage-refresh: background worker failed: {error}");
                false
            }
        }
    }

    pub(super) fn refresh_open_usage_dialog_from_cache(&mut self) -> bool {
        let Some((selected, provider_label)) = self.open_usage_dialog_selection() else {
            return false;
        };
        let mut view = self.focused_usage_snapshot_for_provider(provider_label.as_deref());
        if self.pending_usage_refresh.is_some() {
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
        if self.pending_usage_refresh.is_none() {
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

    pub(super) fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions
            .iter()
            .map(|(&id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
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
                            state: session.state,
                        },
                        None => PaneSnapshot {
                            session_id: id,
                            label: "(missing)".to_owned(),
                            agent: None,
                            state: crate::protocol::control::AgentState::Idle,
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

/// Build a usage refresh target from a session, if it has an agent codename.
fn session_refresh_target(
    session: &crate::session::Session,
) -> Option<crate::usage::UsageRefreshTarget> {
    session
        .agent
        .as_ref()
        .map(|agent| crate::usage::UsageRefreshTarget {
            agent: agent.clone(),
            provider: session.provider.as_ref().map(|p| p.label.clone()),
        })
}

fn decorate_usage_view_refreshing(view: &mut jackin_protocol::control::FocusedUsageView) {
    if !view.updated_label.contains("refreshing") {
        view.updated_label.push_str(" · refreshing...");
    }
}
