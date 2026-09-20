// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Simple Console Usage route.
//!
//! Rust supplies already ordered account/window values. This module owns only
//! the Console split, focus, and Capsule-shaped meter adaptation.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageLifecycleV1, UsagePercent, UsageProjectionV1,
    UsageProviderV1, UsageUnresolvedV1,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::runtime::{BlockingSubscription, SubscriptionPoll};
use crate::tui::state::ManagerState;

/// Heartbeat cadence while the Usage route stays open: a due refresh is
/// requested at most every two minutes (the pre-existing direct-interaction
/// cadence). Broker single-flight generations dedup overlapping requests;
/// completions always reset the timer, so sleep never causes a catch-up
/// burst — at most one refresh is ever in flight.
pub const USAGE_HEARTBEAT_INTERVAL: Duration = Duration::from_mins(2);

/// Completed background refresh payload. Keep the canonical publication
/// intact across the adapter, screen state, and renderer.
pub type UsageRefreshOutcome = Result<UsageProjectionV1, String>;

/// One due Usage refresh, following the instance-refresh effect+subscription
/// pattern: the worker tags its outcome with `generation` and
/// [`UsageScreenState::poll_refresh`] drops completions from stale
/// generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageRefreshRequest {
    pub generation: u64,
    /// True only for an explicit operator refresh (`r`): bypasses the broker
    /// success cadence. Periodic and open-path refreshes pass false so broker
    /// cadence and retry deadlines win. Broker-owned rate-limit/`Retry-After`
    /// deadlines are honored either way, and active generations are joined
    /// rather than duplicated.
    pub force: bool,
}

/// Stable navigation key. Display labels never participate in selection;
/// account rename and provider/account reorder keep the selected destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageEntryId {
    Account {
        provider_id: String,
        canonical_account_id: String,
    },
    Unresolved {
        provider_id: String,
        capability_id: String,
    },
}

/// A borrowed row from the canonical projection. This is navigation-only;
/// usage facts stay on `UsageAccountV1` and are never copied into a UI DTO.
#[derive(Debug, Clone, Copy)]
pub enum UsageEntryRef<'a> {
    Account {
        provider: &'a UsageProviderV1,
        account: &'a UsageAccountV1,
    },
    Unresolved {
        provider: Option<&'a UsageProviderV1>,
        entry: &'a UsageUnresolvedV1,
    },
}

impl<'a> UsageEntryRef<'a> {
    #[must_use]
    pub fn id(self) -> UsageEntryId {
        match self {
            Self::Account { provider, account } => UsageEntryId::Account {
                provider_id: provider.provider_id.clone(),
                canonical_account_id: account.canonical_account_id.clone(),
            },
            Self::Unresolved { entry, .. } => UsageEntryId::Unresolved {
                provider_id: entry.provider_id.clone(),
                capability_id: entry.capability_id.clone(),
            },
        }
    }

    #[must_use]
    pub fn provider_id(self) -> &'a str {
        match self {
            Self::Account { provider, .. } => &provider.provider_id,
            Self::Unresolved { entry, .. } => &entry.provider_id,
        }
    }

    #[must_use]
    pub fn provider_label(self) -> String {
        match self {
            Self::Account { provider, .. } => provider.display_name.clone(),
            Self::Unresolved { provider, entry } => provider.map_or_else(
                || well_known_provider_name(&entry.provider_id),
                |provider| provider.display_name.clone(),
            ),
        }
    }

    #[must_use]
    pub fn account(self) -> Option<&'a UsageAccountV1> {
        match self {
            Self::Account { account, .. } => Some(account),
            Self::Unresolved { .. } => None,
        }
    }

    #[must_use]
    pub fn unresolved(self) -> Option<&'a UsageUnresolvedV1> {
        match self {
            Self::Account { .. } => None,
            Self::Unresolved { entry, .. } => Some(entry),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UsageFocus {
    #[default]
    List,
    Detail,
}

#[derive(Debug, Default)]
pub struct UsageScreenState {
    /// Complete canonical host inventory. `None` means the first snapshot has
    /// not arrived; an empty projection is distinct from not-yet-loaded.
    pub projection: Option<UsageProjectionV1>,
    pub selected: usize,
    pub selected_id: Option<UsageEntryId>,
    pub focus: UsageFocus,
    pub list_scroll: u16,
    pub detail_scroll: u16,
    pub notice: Option<String>,
    pub generated_at_epoch: Option<i64>,
    pub refresh_due: bool,
    pub force_refresh_pending: bool,
    pub refresh_generation: u64,
    pub last_refresh_at: Option<Instant>,
    pub refresh_rx: Option<BlockingSubscription<(u64, UsageRefreshOutcome)>>,
}

// Manual impls: the in-flight refresh handle carries no value identity.
// Cloning drops it (the worker result is then discarded); equality ignores
// it so tests can compare screen snapshots while a refresh runs. The
// generation counter is value state and survives both.
impl Clone for UsageScreenState {
    fn clone(&self) -> Self {
        Self {
            projection: self.projection.clone(),
            selected: self.selected,
            selected_id: self.selected_id.clone(),
            focus: self.focus,
            list_scroll: self.list_scroll,
            detail_scroll: self.detail_scroll,
            notice: self.notice.clone(),
            generated_at_epoch: self.generated_at_epoch,
            refresh_due: self.refresh_due,
            force_refresh_pending: self.force_refresh_pending,
            refresh_generation: self.refresh_generation,
            last_refresh_at: self.last_refresh_at,
            refresh_rx: None,
        }
    }
}

impl PartialEq for UsageScreenState {
    fn eq(&self, other: &Self) -> bool {
        self.projection == other.projection
            && self.selected == other.selected
            && self.selected_id == other.selected_id
            && self.focus == other.focus
            && self.list_scroll == other.list_scroll
            && self.detail_scroll == other.detail_scroll
            && self.notice == other.notice
            && self.generated_at_epoch == other.generated_at_epoch
            && self.refresh_due == other.refresh_due
            && self.force_refresh_pending == other.force_refresh_pending
            && self.refresh_generation == other.refresh_generation
            && self.last_refresh_at == other.last_refresh_at
    }
}

impl Eq for UsageScreenState {}

impl UsageScreenState {
    /// Open the route over a cached snapshot and mark a refresh due so the
    /// first broker read lands right after open. Selection starts at
    /// Overview; refreshes re-anchor by stable id from there.
    #[must_use]
    pub fn open_with_snapshot(
        projection: Option<UsageProjectionV1>,
        notice: Option<String>,
    ) -> Self {
        Self {
            notice: notice.or_else(|| projection_notice(projection.as_ref())),
            projection,
            refresh_due: true,
            ..Self::default()
        }
    }

    /// Retain the Rust-owned canonical publication without projecting its
    /// account facts into a Console-specific structure.
    pub fn from_projection(projection: &UsageProjectionV1) -> Self {
        Self {
            projection: Some(projection.clone()),
            notice: projection_notice(Some(projection)),
            ..Self::default()
        }
    }

    /// Apply a completed background refresh, re-anchoring selection by
    /// stable id so renames/reorders keep the operator's row. A removed
    /// selection falls back to Overview with an inline notice.
    pub fn apply_refresh(&mut self, projection: UsageProjectionV1, now: Instant) {
        self.notice = projection_notice(Some(&projection));
        self.projection = Some(projection);
        self.last_refresh_at = Some(now);
        self.refresh_due = false;
        match &self.selected_id {
            None => self.selected = 0,
            Some(id) => {
                if let Some(pos) = self.entries().iter().position(|entry| &entry.id() == id) {
                    self.selected = pos.saturating_add(1);
                } else {
                    self.selected = 0;
                    self.selected_id = None;
                    self.notice = Some(match self.notice.take() {
                        Some(notice) => format!(
                            "{notice} · previously selected account unavailable; showing Overview"
                        ),
                        None => {
                            "Previously selected account unavailable; showing Overview".to_owned()
                        }
                    });
                }
            }
        }
        self.selected = self.selected.min(self.entries().len());
    }

    /// Record a failed background refresh. The timer still advances so a
    /// broken broker retries at heartbeat cadence (or on manual `r`),
    /// never once per keypress.
    pub fn apply_refresh_error(&mut self, notice: String, now: Instant) {
        self.notice = Some(notice);
        self.last_refresh_at = Some(now);
        self.refresh_due = false;
    }

    #[must_use]
    pub fn refresh_in_flight(&self) -> bool {
        self.refresh_rx.is_some()
    }

    /// Claim the next due refresh, if any, following the instance-refresh
    /// throttle shape: at most one generation is ever in flight, and a
    /// requester arriving while one runs joins that shared work instead of
    /// queueing a duplicate. Claiming consumes `refresh_due` and the pending
    /// force flag; the worker must tag its outcome with the generation via
    /// [`Self::begin_refresh`].
    pub fn next_refresh_plan_if_due(&mut self, now: Instant) -> Option<UsageRefreshRequest> {
        if self.refresh_in_flight() {
            self.refresh_due = false;
            return None;
        }
        if !self.refresh_due && !self.heartbeat_due(now) {
            return None;
        }
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        self.refresh_due = false;
        let force = std::mem::take(&mut self.force_refresh_pending);
        Some(UsageRefreshRequest {
            generation: self.refresh_generation,
            force,
        })
    }

    pub fn begin_refresh(&mut self, rx: BlockingSubscription<(u64, UsageRefreshOutcome)>) {
        self.refresh_rx = Some(rx);
    }

    /// Poll the in-flight refresh once. `None` means still running — or that
    /// the completed generation was stale and its outcome was dropped.
    pub fn poll_refresh(&mut self) -> Option<UsageRefreshOutcome> {
        let rx = self.refresh_rx.as_mut()?;
        match rx.poll_next() {
            SubscriptionPoll::Ready((generation, outcome)) => {
                self.refresh_rx = None;
                if generation == self.refresh_generation {
                    Some(outcome)
                } else {
                    None
                }
            }
            SubscriptionPoll::Closed => {
                self.refresh_rx = None;
                Some(Err("usage refresh worker disconnected".to_owned()))
            }
            SubscriptionPoll::Pending => None,
        }
    }

    /// True once the last completed refresh is older than the heartbeat
    /// interval. Never true before the first completion: the open path
    /// drives that via `refresh_due`.
    #[must_use]
    pub fn heartbeat_due(&self, now: Instant) -> bool {
        self.last_refresh_at
            .is_some_and(|at| now.duration_since(at) >= USAGE_HEARTBEAT_INTERVAL)
    }

    pub fn move_selection(&mut self, delta: isize) {
        let entry_count = self.entries().len();
        if entry_count == 0 {
            return;
        }
        let len = entry_count.saturating_add(1);
        let current = self.selected.min(len - 1);
        self.selected = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta.cast_unsigned())
                .min(len.saturating_sub(1))
        };
        self.selected_id = self
            .entries()
            .get(self.selected.saturating_sub(1))
            .filter(|_| self.selected > 0)
            .map(|entry| entry.id());
    }

    #[must_use]
    pub fn entries(&self) -> Vec<UsageEntryRef<'_>> {
        let Some(projection) = self.projection.as_ref() else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for provider in &projection.providers {
            entries.extend(
                provider
                    .accounts
                    .iter()
                    .map(|account| UsageEntryRef::Account { provider, account }),
            );
            entries.extend(
                projection
                    .unresolved
                    .iter()
                    .filter(|entry| entry.provider_id == provider.provider_id)
                    .map(|entry| UsageEntryRef::Unresolved {
                        provider: Some(provider),
                        entry,
                    }),
            );
        }
        entries.extend(
            projection
                .unresolved
                .iter()
                .filter(|entry| {
                    !projection
                        .providers
                        .iter()
                        .any(|provider| provider.provider_id == entry.provider_id)
                })
                .map(|entry| UsageEntryRef::Unresolved {
                    provider: None,
                    entry,
                }),
        );
        entries
    }

    #[must_use]
    pub fn selected_entry(&self) -> Option<UsageEntryRef<'_>> {
        (self.selected > 0)
            .then(|| self.entries().get(self.selected - 1).copied())
            .flatten()
    }

    fn overview_selected(&self) -> bool {
        self.selected == 0
    }
}

fn projection_notice(projection: Option<&UsageProjectionV1>) -> Option<String> {
    let unresolved = projection?.unresolved.len();
    (unresolved > 0).then(|| format!("{unresolved} configured capability(s) unresolved"))
}

fn entry_status_label(entry: UsageEntryRef<'_>) -> String {
    match entry {
        UsageEntryRef::Account { account, .. } => {
            let status = account
                .status_label
                .clone()
                .unwrap_or_else(|| lifecycle_label(account.lifecycle).to_owned());
            if account.freshness.is_stale && account.lifecycle == UsageLifecycleV1::Available {
                "stale".to_owned()
            } else {
                status
            }
        }
        UsageEntryRef::Unresolved { entry, .. } => {
            let mut status = lifecycle_label(entry.state).to_owned();
            if let Some(issue) = entry
                .issues
                .first()
                .filter(|issue| !issue.message.trim().is_empty())
            {
                status.push_str(" · ");
                status.push_str(issue.message.trim());
            }
            status
        }
    }
}

fn lifecycle_label(lifecycle: UsageLifecycleV1) -> &'static str {
    match lifecycle {
        UsageLifecycleV1::Available => "available",
        UsageLifecycleV1::AgentUninitialized => "not started",
        UsageLifecycleV1::NeedsLogin => "needs login",
        UsageLifecycleV1::NeedsSecret => "needs secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::Error => "error",
    }
}

/// Operator-facing freshness age for one account. Pure over an explicit
/// `now` so tests stay deterministic; render passes wall-clock time.
#[must_use]
pub fn freshness_age_label(
    now_epoch: i64,
    phase: UsageFreshnessPhaseV1,
    last_good_at_epoch: Option<i64>,
    is_stale: bool,
) -> String {
    if phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_good) = last_good_at_epoch else {
        return "never updated".to_owned();
    };
    let age_secs = now_epoch.saturating_sub(last_good).max(0);
    let age = if age_secs < 60 {
        "just now".to_owned()
    } else if age_secs < 3_600 {
        format!("{}m ago", age_secs / 60)
    } else if age_secs < 86_400 {
        format!("{}h ago", age_secs / 3_600)
    } else {
        format!("{}d ago", age_secs / 86_400)
    };
    if is_stale
        || matches!(
            phase,
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
        )
    {
        format!("stale · updated {age}")
    } else {
        format!("updated {age}")
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn well_known_provider_name(provider_id: &str) -> String {
    match provider_id.to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => "Anthropic / Claude".to_owned(),
        "openai" | "codex" => "OpenAI".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        "kimi" | "moonshot" => "Kimi".to_owned(),
        "grok" | "xai" => "Grok".to_owned(),
        "amp" => "Amp".to_owned(),
        "zai" => "Z.AI".to_owned(),
        "minimax" => "MiniMax".to_owned(),
        other => other.to_owned(),
    }
}

pub fn handle_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(screen) = state.usage.screen.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Left => match screen.focus {
            UsageFocus::Detail => screen.focus = UsageFocus::List,
            UsageFocus::List => state.usage.visible = false,
        },
        KeyCode::Char('q') => state.usage.visible = false,
        KeyCode::Tab | KeyCode::Right => {
            screen.focus = match screen.focus {
                UsageFocus::List => UsageFocus::Detail,
                UsageFocus::Detail => UsageFocus::List,
            };
        }
        KeyCode::Up | KeyCode::Char('k') if screen.focus == UsageFocus::List => {
            screen.move_selection(-1);
        }
        KeyCode::Down | KeyCode::Char('j') if screen.focus == UsageFocus::List => {
            screen.move_selection(1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            screen.detail_scroll = screen.detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            screen.detail_scroll = screen.detail_scroll.saturating_add(1);
        }
        KeyCode::Enter => {
            screen.focus = match screen.focus {
                UsageFocus::List => UsageFocus::Detail,
                UsageFocus::Detail => UsageFocus::List,
            };
        }
        KeyCode::Char('r') => {
            screen.refresh_due = true;
            screen.force_refresh_pending = true;
        }
        KeyCode::PageUp if screen.focus == UsageFocus::List => {
            screen.list_scroll = screen.list_scroll.saturating_sub(5);
        }
        KeyCode::PageDown if screen.focus == UsageFocus::List => {
            screen.list_scroll = screen.list_scroll.saturating_add(5);
        }
        KeyCode::PageUp => screen.detail_scroll = screen.detail_scroll.saturating_sub(5),
        KeyCode::PageDown => {
            screen.detail_scroll = screen.detail_scroll.saturating_add(5);
        }
        _ => {}
    }
}

const USAGE_COMPACT_BREAKPOINT: u16 = 84;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    if body.width < USAGE_COMPACT_BREAKPOINT {
        match screen.focus {
            UsageFocus::List => render_account_list(frame, body, state, true),
            UsageFocus::Detail => render_detail(frame, body, state, true),
        }
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    render_account_list(frame, columns[0], state, screen.focus == UsageFocus::List);
    render_detail(frame, columns[1], state, screen.focus == UsageFocus::Detail);
}

fn render_account_list(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>, focused: bool) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let block = Block::default()
        .title(" Accounts ")
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let entries = screen.entries();
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_inventory_message(screen))
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    let meter_width = inner.width.saturating_sub(8) as usize;
    let now = now_epoch();
    lines.push(selected_row_line(
        "Overview",
        screen.overview_selected(),
        inner.width,
    ));
    let mut prior_provider: Option<String> = None;
    for (index, entry) in entries.iter().copied().enumerate() {
        let provider = entry.provider_label();
        if prior_provider.as_deref() != Some(provider.as_str()) {
            lines.push(Line::from(Span::styled(
                format!("  {provider}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            prior_provider = Some(provider);
        }
        let selected = index.saturating_add(1) == screen.selected;
        let label = entry_display_label(&entries, index);
        lines.push(selected_row_line(&label, selected, inner.width));
        let status = entry_status_label(entry);
        let summary = entry
            .account()
            .and_then(|account| account.windows.first())
            .map_or_else(|| status.clone(), |window| window.value_label.clone());
        let freshness = entry.account().map_or_else(
            || "never updated".to_owned(),
            |account| {
                freshness_age_label(
                    now,
                    account.freshness.phase,
                    account.freshness.last_good_at_epoch,
                    account.freshness.is_stale,
                )
            },
        );
        lines.push(Line::from(Span::styled(
            format!("    {status} · {summary} · {freshness}"),
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(window) = entry.account().and_then(|account| account.windows.first())
            && let Some(bar) = meter_line(meter_width, window_meter_percent(window))
        {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window_meter_percent(window).unwrap_or(0)),
            )));
        }
    }
    let selected_line = selected_list_line(&lines, screen.selected);
    let visible_height = usize::from(inner.height);
    let mut scroll = usize::from(screen.list_scroll);
    if selected_line < scroll {
        scroll = selected_line;
    } else if selected_line >= scroll.saturating_add(visible_height) && visible_height > 0 {
        scroll = selected_line
            .saturating_add(1)
            .saturating_sub(visible_height);
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>, focused: bool) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let now = now_epoch();
    let Some(entry) = screen.selected_entry() else {
        if screen.entries().is_empty() {
            frame.render_widget(
                Paragraph::new(empty_inventory_message(screen))
                    .block(panel("Overview", focused))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let mut lines = Vec::new();
        if screen.refresh_in_flight() {
            lines.push(refreshing_line());
            lines.push(Line::from(""));
        }
        let width = area.width.saturating_sub(8).max(8) as usize;
        let entries = screen.entries();
        for index in 0..entries.len() {
            append_overview_account(&mut lines, &entries, index, width, now);
        }
        if let Some(notice) = &screen.notice {
            lines.push(Line::from(Span::styled(
                notice.clone(),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel("Overview", focused))
                .scroll((screen.detail_scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };
    let label = entry_display_label(&screen.entries(), screen.selected.saturating_sub(1));
    let provider_label = entry.provider_label();
    let status = entry_status_label(entry);
    let mut lines = vec![Line::from(format!("Provider  {provider_label}"))];
    lines.push(Line::from(format!("Account   {label}")));
    lines.push(Line::from(format!("Status    {status}")));
    if let Some(account) = entry.account() {
        lines.push(Line::from(format!(
            "Freshness {}",
            freshness_age_label(
                now,
                account.freshness.phase,
                account.freshness.last_good_at_epoch,
                account.freshness.is_stale,
            )
        )));
        if let Some(plan) = &account.plan_label {
            lines.push(Line::from(format!("Plan      {plan}")));
        }
        if !account.issues.is_empty() {
            lines.extend(
                account
                    .issues
                    .iter()
                    .map(|issue| Line::from(format!("{}: {}", issue.code, issue.message))),
            );
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Limits"));
    if screen.refresh_in_flight() {
        lines.push(refreshing_line());
        lines.push(Line::from(""));
    }
    for window in entry
        .account()
        .into_iter()
        .flat_map(|account| account.windows.iter())
    {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        let width = area.width.saturating_sub(8).max(8) as usize;
        if let Some(bar) = meter_line(width, window_meter_percent(window)) {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window_meter_percent(window).unwrap_or(0)),
            )));
        }
        let detail = if window.reset_label.is_empty() {
            format!("  {}", window.value_label)
        } else {
            format!("  {} · {}", window.value_label, window.reset_label)
        };
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
    }
    if entry.account().is_none() {
        lines.push(Line::from(
            "No quota data is available for this account yet.",
        ));
    }
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel("Account", focused))
            .scroll((screen.detail_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn refreshing_line() -> Line<'static> {
    Line::from(Span::styled(
        "Refreshing usage…",
        Style::default().fg(Color::DarkGray),
    ))
}

fn empty_inventory_message(screen: &UsageScreenState) -> String {
    if screen.projection.is_none() {
        return screen.notice.as_ref().map_or_else(
            || "Loading account inventory…\n\nPress R to refresh.".to_owned(),
            |error| format!("Usage inventory unavailable: {error}\n\nPress R to retry."),
        );
    }
    if screen.entries().is_empty() {
        return "No accounts configured.\n\nPress R to refresh; use Settings to add an account."
            .to_owned();
    }
    String::new()
}

fn append_overview_window(
    lines: &mut Vec<Line<'static>>,
    window: &jackin_protocol::usage_broker::UsageLimitWindowV1,
    width: usize,
) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    let meter_percent = window_meter_percent(window);
    if let Some(bar) = meter_line(width, meter_percent) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(meter_percent.unwrap_or(0)),
        )));
    }
    let detail = if window.reset_label.is_empty() {
        format!("  {}", window.value_label)
    } else {
        format!("  {} · {}", window.value_label, window.reset_label)
    };
    lines.push(Line::from(detail));
}

fn append_overview_account(
    lines: &mut Vec<Line<'static>>,
    entries: &[UsageEntryRef<'_>],
    index: usize,
    width: usize,
    now_epoch: i64,
) {
    let Some(entry) = entries.get(index).copied() else {
        return;
    };
    let label = entry_display_label(entries, index);
    lines.push(Line::from(Span::styled(
        format!("{} · {label}", entry.provider_label()),
        Style::default().fg(Color::White),
    )));
    if let Some(account) = entry.account() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                freshness_age_label(
                    now_epoch,
                    account.freshness.phase,
                    account.freshness.last_good_at_epoch,
                    account.freshness.is_stale,
                )
            ),
            Style::default().fg(Color::DarkGray),
        )));
        if account.windows.is_empty() {
            lines.push(Line::from(format!("  {}", entry_status_label(entry))));
        } else {
            for window in &account.windows {
                append_overview_window(lines, window, width);
            }
        }
    } else {
        lines.push(Line::from(format!("  {}", entry_status_label(entry))));
    }
    lines.push(Line::from(""));
}

fn entry_display_label(entries: &[UsageEntryRef<'_>], index: usize) -> String {
    let Some(entry) = entries.get(index).copied() else {
        return "Account".to_owned();
    };
    let Some(account) = entry.account() else {
        let ordinal = entries[..index]
            .iter()
            .filter(|candidate| {
                candidate.provider_id() == entry.provider_id() && candidate.account().is_none()
            })
            .count()
            .saturating_add(1);
        return format!("Unresolved account {ordinal}");
    };

    let duplicate_indices = entries
        .iter()
        .enumerate()
        .filter_map(|(candidate_index, candidate)| {
            let candidate_account = candidate.account()?;
            (candidate.provider_id() == entry.provider_id()
                && candidate_account.display_label == account.display_label)
                .then_some(candidate_index)
        })
        .collect::<Vec<_>>();
    if duplicate_indices.len() <= 1 {
        return account.display_label.clone();
    }
    let ordinal = duplicate_indices
        .iter()
        .position(|candidate_index| *candidate_index == index)
        .unwrap_or(0)
        .saturating_add(1);
    format!("{} ({ordinal})", account.display_label)
}

fn selected_row_line(label: &str, selected: bool, width: u16) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let text = truncate_to_cols(&format!("{marker}{label}"), usize::from(width));
    let padding = usize::from(width).saturating_sub(termrock::text::display_cols(&text));
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(Span::styled(
        format!("{text}{}", " ".repeat(padding)),
        style,
    ))
}

fn truncate_to_cols(text: &str, max_cols: usize) -> String {
    if termrock::text::display_cols(text) <= max_cols {
        return text.to_owned();
    }
    if max_cols == 0 {
        return String::new();
    }
    let content_cols = max_cols.saturating_sub(1);
    let mut truncated = String::new();
    for character in text.chars() {
        let mut next = truncated.clone();
        next.push(character);
        if termrock::text::display_cols(&next) > content_cols {
            break;
        }
        truncated.push(character);
    }
    truncated.push('…');
    truncated
}

fn selected_list_line(lines: &[Line<'_>], _selected: usize) -> usize {
    lines
        .iter()
        .position(|line| line.spans.iter().any(|span| span.content.starts_with("▸ ")))
        .unwrap_or(0)
}

fn window_meter_percent(window: &jackin_protocol::usage_broker::UsageLimitWindowV1) -> Option<u8> {
    window.remaining_percent.map(UsagePercent::get).or_else(|| {
        window
            .used_percent
            .map(|used| 100_u8.saturating_sub(used.get().min(100)))
    })
}

fn panel(title: &'static str, focused: bool) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        })
}

/// Meter bar for a known percent. `None` renders no bar: unknown quota
/// must never show an empty (fabricated) meter.
fn meter_line(width: usize, percent: Option<u8>) -> Option<String> {
    let percent = usize::from(percent?.min(100));
    let filled = width.saturating_mul(percent) / 100;
    Some(format!(
        "  {}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    ))
}

fn meter_style(remaining: u8) -> Style {
    match remaining {
        0..=15 => Style::default().fg(Color::Red),
        16..=35 => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}

#[cfg(test)]
mod tests;
