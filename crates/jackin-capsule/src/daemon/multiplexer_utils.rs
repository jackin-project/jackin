//! Miscellaneous Multiplexer utility methods.

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
        if matches!(
            reason,
            FullRedrawReason::FirstAttach | FullRedrawReason::Resize
        ) {
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

    pub(super) fn focused_usage_snapshot(
        &mut self,
        force_refresh: bool,
    ) -> jackin_protocol::control::FocusedUsageView {
        self.focused_usage_snapshot_for_provider(None, force_refresh)
    }

    pub(super) fn focused_usage_snapshot_for_provider(
        &mut self,
        provider_label: Option<&str>,
        force_refresh: bool,
    ) -> jackin_protocol::control::FocusedUsageView {
        let focused_id = self.active_focused_id();
        let (agent, provider) =
            focused_id
                .and_then(|id| self.sessions.get(&id))
                .map_or((None, None), |session| {
                    (
                        session.agent.clone(),
                        session.provider.as_ref().map(|p| p.label.clone()),
                    )
                });
        let provider = provider_label
            .map(str::to_owned)
            .or_else(|| provider.as_ref().map(ToOwned::to_owned));
        let mut view = self.usage_cache.focused_snapshot(
            agent.as_deref(),
            provider.as_deref(),
            &self.provider_keys,
            force_refresh,
        );
        if let Some(session_id) = focused_id {
            crate::usage::apply_cached_session_spend(&mut view, session_id);
        }
        self.attach_instance_usage(&mut view);
        view
    }

    pub(super) fn warm_usage_account_snapshots(&mut self, force_refresh: bool) {
        let agent = self
            .active_focused_id()
            .and_then(|id| self.sessions.get(&id))
            .and_then(|session| session.agent.clone());
        self.usage_cache.warm_account_snapshots(
            agent.as_deref(),
            &self.provider_keys,
            force_refresh,
        );
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

    fn attach_instance_usage(&self, view: &mut jackin_protocol::control::FocusedUsageView) {
        use jackin_protocol::control::{InstanceAgentUsageRow, InstanceUsageView};

        let now = chrono::Utc::now();
        let started_at = self
            .agent_history
            .iter()
            .map(|record| record.started_at)
            .min();
        let agent_rows = self
            .agent_history
            .iter()
            .map(|record| {
                let spend = crate::usage::cached_usage_summary_for_instance(
                    Some(&self.instance_id),
                    None,
                    i64::try_from(record.session_id).ok(),
                    None,
                );
                let sampled_identity = i64::try_from(record.session_id).ok().and_then(|session| {
                    crate::telemetry_store::usage_sample_account_identity(
                        std::path::Path::new(crate::usage::TELEMETRY_STORE_PATH),
                        Some(&self.instance_id),
                        Some(session),
                    )
                    .ok()
                    .flatten()
                });
                let tab_lineage = self.instance_tab_lineage(record);
                let last_activity_epoch = spend
                    .last_occurred_at
                    .or_else(|| record.exited_at.map(|time| time.timestamp()))
                    .or_else(|| Some(record.started_at.timestamp()));
                InstanceAgentUsageRow {
                    codename: record.codename.clone(),
                    session_id: record.session_id,
                    agent_label: record.agent.clone().unwrap_or_else(|| "shell".to_owned()),
                    provider_label: record
                        .provider
                        .clone()
                        .unwrap_or_else(|| "account unavailable".to_owned()),
                    account_label: sampled_identity.as_ref().map_or_else(
                        || self.instance_row_account_label(record, view),
                        |(account, _plan)| account.clone(),
                    ),
                    plan_label: sampled_identity
                        .as_ref()
                        .and_then(|(_account, plan)| plan.clone()),
                    lifecycle_label: if record.exited_at.is_some() {
                        "closed".to_owned()
                    } else {
                        "active".to_owned()
                    },
                    tab_label: tab_lineage.tab_label,
                    pane_label: Some(tab_lineage.pane_label),
                    started_at_epoch: Some(record.started_at.timestamp()),
                    exited_at_epoch: record.exited_at.map(|time| time.timestamp()),
                    last_activity_epoch,
                    last_activity_label: last_activity_epoch
                        .map(|epoch| format!("{} ago", compact_age_label(now.timestamp() - epoch))),
                    spend,
                }
            })
            .collect::<Vec<_>>();

        let total = crate::usage::cached_usage_summary_for_instance(
            Some(&self.instance_id),
            None,
            None,
            None,
        );
        let today = crate::usage::cached_usage_summary_for_instance(
            Some(&self.instance_id),
            None,
            None,
            Some(24 * 60 * 60),
        );
        let provider_rows = provider_instance_rows(&agent_rows, &self.usage_cache);
        view.instance = Some(InstanceUsageView {
            instance_label: self.instance_id.clone(),
            started_at_epoch: started_at.map(|time| time.timestamp()),
            age_label: started_at.map_or_else(
                || "not started".to_owned(),
                |start| compact_age_label((now - start).num_seconds()),
            ),
            active_agent_time_label: active_agent_time_label(&agent_rows, now.timestamp()),
            workspace: self.workdir.to_string_lossy().into_owned(),
            today,
            total,
            agent_rows,
            provider_rows,
        });
    }

    fn instance_tab_lineage(&self, record: &super::AgentRecord) -> InstanceTabLineage {
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            if tab.codename == record.codename {
                return InstanceTabLineage {
                    tab_label: Some(tab.label_owned()),
                    pane_label: format!("tab {} · pane session {}", tab_idx + 1, record.session_id),
                };
            }
            if tab
                .tree
                .leaves(crate::tui::layout::Rect::new(
                    0,
                    0,
                    self.term_rows,
                    self.term_cols,
                ))
                .iter()
                .any(|(session_id, _)| *session_id == record.session_id)
            {
                return InstanceTabLineage {
                    tab_label: Some(tab.label_owned()),
                    pane_label: format!("tab {} · pane session {}", tab_idx + 1, record.session_id),
                };
            }
        }
        InstanceTabLineage {
            tab_label: None,
            pane_label: format!("closed tab · session {}", record.session_id),
        }
    }

    fn instance_row_account_label(
        &self,
        record: &super::AgentRecord,
        view: &jackin_protocol::control::FocusedUsageView,
    ) -> String {
        if let Some(account) = instance_row_account_label_from_view(record, view) {
            return account;
        }
        record
            .provider
            .as_deref()
            .and_then(|provider| self.usage_cache.account_identity_for_provider(provider))
            .map_or_else(
                || "account unavailable".to_owned(),
                |(account, _provider, _plan)| account,
            )
    }
}

struct InstanceTabLineage {
    tab_label: Option<String>,
    pane_label: String,
}

fn compact_age_label(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn active_agent_time_label(
    rows: &[jackin_protocol::control::InstanceAgentUsageRow],
    now_epoch: i64,
) -> Option<String> {
    let total_seconds = rows.iter().fold(0i64, |acc, row| {
        let Some(started) = row.started_at_epoch else {
            return acc;
        };
        let ended = row.exited_at_epoch.unwrap_or(now_epoch);
        acc.saturating_add(ended.saturating_sub(started).max(0))
    });
    (total_seconds > 0).then(|| compact_age_label(total_seconds))
}

fn sum_usage_summaries(
    summaries: impl IntoIterator<Item = jackin_protocol::control::UsageSummaryView>,
) -> jackin_protocol::control::UsageSummaryView {
    let mut total = jackin_protocol::control::UsageSummaryView::default();
    for summary in summaries {
        total.sample_count = total.sample_count.saturating_add(summary.sample_count);
        total.token_input = total.token_input.saturating_add(summary.token_input);
        total.token_output = total.token_output.saturating_add(summary.token_output);
        total.token_cache_read = total
            .token_cache_read
            .saturating_add(summary.token_cache_read);
        total.token_cache_write = total
            .token_cache_write
            .saturating_add(summary.token_cache_write);
        total.cost_usd_micros = total
            .cost_usd_micros
            .saturating_add(summary.cost_usd_micros);
        if summary.latest_tokens.is_some() {
            total.latest_tokens = summary.latest_tokens;
        }
        if total.history.is_empty() {
            total.history = summary.history;
        } else {
            for (idx, value) in summary.history.into_iter().enumerate() {
                if idx >= total.history.len() {
                    total.history.push(value);
                } else {
                    total.history[idx] = total.history[idx].saturating_add(value);
                }
            }
        }
        total.exact_cost_sample_count = total
            .exact_cost_sample_count
            .saturating_add(summary.exact_cost_sample_count);
        total.estimated_cost_sample_count = total
            .estimated_cost_sample_count
            .saturating_add(summary.estimated_cost_sample_count);
        total.unpriced_sample_count = total
            .unpriced_sample_count
            .saturating_add(summary.unpriced_sample_count);
        total.first_occurred_at = match (total.first_occurred_at, summary.first_occurred_at) {
            (Some(current), Some(next)) => Some(current.min(next)),
            (None, next) => next,
            (current, None) => current,
        };
        total.last_occurred_at = match (total.last_occurred_at, summary.last_occurred_at) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (None, next) => next,
            (current, None) => current,
        };
        if total.top_model.is_none() {
            total.top_model = summary.top_model;
        }
    }
    total
}

fn instance_row_account_label_from_view(
    record: &super::AgentRecord,
    view: &jackin_protocol::control::FocusedUsageView,
) -> Option<String> {
    let provider = record.provider.as_deref().unwrap_or_default();
    let account = view.account.account_label.trim();
    if account.is_empty()
        || !provider_matches_usage_account(provider, &view.account.provider_label)
        || account.starts_with("needs ")
        || account.ends_with(" unavailable")
    {
        None
    } else {
        Some(account.to_owned())
    }
}

fn provider_matches_usage_account(provider: &str, account_provider: &str) -> bool {
    let provider = provider.to_ascii_lowercase();
    let account_provider = account_provider.to_ascii_lowercase();
    provider == account_provider
        || provider.contains(&account_provider)
        || account_provider.contains(&provider)
        || (provider.contains("openai") && account_provider.contains("codex"))
        || (provider.contains("codex") && account_provider.contains("codex"))
        || (provider.contains("anthropic") && account_provider.contains("claude"))
        || (provider.contains("claude") && account_provider.contains("claude"))
}

fn provider_instance_rows(
    agent_rows: &[jackin_protocol::control::InstanceAgentUsageRow],
    usage_cache: &crate::usage::UsageCache,
) -> Vec<jackin_protocol::control::InstanceProviderUsageRow> {
    use std::collections::BTreeMap;

    let mut grouped: BTreeMap<
        (String, String, Option<String>),
        Vec<jackin_protocol::control::UsageSummaryView>,
    > = BTreeMap::new();
    for row in agent_rows {
        let plan_label = row.plan_label.clone().or_else(|| {
            usage_cache
                .account_identity_for_provider(&row.provider_label)
                .and_then(|(account, _provider, plan)| {
                    (account == row.account_label && row.account_label != "account unavailable")
                        .then_some(plan)
                })
                .flatten()
        });
        grouped
            .entry((
                row.provider_label.clone(),
                row.account_label.clone(),
                plan_label,
            ))
            .or_default()
            .push(row.spend.clone());
    }
    grouped
        .into_iter()
        .map(|((provider_label, account_label, plan_label), summaries)| {
            jackin_protocol::control::InstanceProviderUsageRow {
                provider_label,
                account_label,
                plan_label,
                spend: sum_usage_summaries(summaries),
            }
        })
        .collect()
}
