// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Simple Console Usage route.
//!
//! Rust supplies already ordered account/window values. This module owns only
//! the Console split, focus, and Capsule-shaped meter adaptation.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent};
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageLifecycleV1, UsagePercent};
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

/// Completed background refresh payload: canonical rows plus the
/// projection-level notice. Produced off the UI thread by
/// `load_console_usage_state` in the console adapter.
pub type UsageRefreshOutcome = Result<(Vec<UsageAccount>, Option<String>), String>;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageWindow {
    pub window_id: String,
    pub label: String,
    pub value: String,
    pub reset: String,
    pub remaining_percent: Option<u8>,
    pub used_percent: Option<u8>,
    pub reset_at_epoch: Option<i64>,
}

impl UsageWindow {
    /// Meter geometry input. Providers report exactly one representation;
    /// `used` is mirrored to `remaining` without inventing precision.
    /// `None` means unknown — callers must render no bar at all.
    #[must_use]
    pub fn meter_percent(&self) -> Option<u8> {
        self.remaining_percent.or_else(|| {
            self.used_percent
                .map(|used| 100_u8.saturating_sub(used.min(100)))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAccount {
    pub provider_id: String,
    /// Canonical account id; the capability id while `unresolved`.
    pub canonical_account_id: String,
    pub unresolved: bool,
    pub provider: String,
    pub account: String,
    pub status: String,
    pub lifecycle: UsageLifecycleV1,
    pub freshness_phase: UsageFreshnessPhaseV1,
    pub last_good_at_epoch: Option<i64>,
    pub is_stale: bool,
    pub windows: Vec<UsageWindow>,
}

impl UsageAccount {
    /// Stable selection identity across rename/reorder. Display labels
    /// never participate: renames keep the id, and unresolved rows anchor
    /// on their capability id.
    #[must_use]
    pub fn stable_id(&self) -> String {
        format!("{}:{}", self.provider_id, self.canonical_account_id)
    }
}

#[derive(Debug, Default)]
pub struct UsageScreenState {
    pub accounts: Vec<UsageAccount>,
    pub selected: usize,
    pub selected_id: Option<String>,
    pub detail: bool,
    pub scroll: u16,
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
            accounts: self.accounts.clone(),
            selected: self.selected,
            selected_id: self.selected_id.clone(),
            detail: self.detail,
            scroll: self.scroll,
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
        self.accounts == other.accounts
            && self.selected == other.selected
            && self.selected_id == other.selected_id
            && self.detail == other.detail
            && self.scroll == other.scroll
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
    pub fn open_with_snapshot(accounts: Vec<UsageAccount>, notice: Option<String>) -> Self {
        Self {
            accounts,
            notice,
            refresh_due: true,
            ..Self::default()
        }
    }

    /// Project the Rust-owned canonical publication into Console rows.
    ///
    /// The Console owns only layout. Provider/account identity, lifecycle,
    /// ordering, and quota labels remain in the protocol projection.
    pub fn from_projection(projection: &jackin_protocol::usage_broker::UsageProjectionV1) -> Self {
        let mut accounts = Vec::new();
        for provider in &projection.providers {
            for account in &provider.accounts {
                let mut status = account
                    .status_label
                    .clone()
                    .unwrap_or_else(|| lifecycle_label(account.lifecycle).to_owned());
                if account.freshness.is_stale && account.lifecycle == UsageLifecycleV1::Available {
                    status = "stale".to_owned();
                }
                let windows = account
                    .windows
                    .iter()
                    .map(|window| UsageWindow {
                        window_id: window.window_id.clone(),
                        label: window.label.clone(),
                        value: window.value_label.clone(),
                        reset: window.reset_label.clone(),
                        remaining_percent: window.remaining_percent.map(UsagePercent::get),
                        used_percent: window.used_percent.map(UsagePercent::get),
                        reset_at_epoch: window.reset_at_epoch,
                    })
                    .collect();
                accounts.push(UsageAccount {
                    provider_id: provider.provider_id.clone(),
                    canonical_account_id: account.canonical_account_id.clone(),
                    unresolved: false,
                    provider: provider.display_name.clone(),
                    account: account.display_label.clone(),
                    status,
                    lifecycle: account.lifecycle,
                    freshness_phase: account.freshness.phase,
                    last_good_at_epoch: account.freshness.last_good_at_epoch,
                    is_stale: account.freshness.is_stale,
                    windows,
                });
            }
        }

        for unresolved in &projection.unresolved {
            let provider_name = projection
                .providers
                .iter()
                .find(|p| p.provider_id == unresolved.provider_id)
                .map_or_else(
                    || well_known_provider_name(&unresolved.provider_id),
                    |p| p.display_name.clone(),
                );
            let account_label = format!("Unresolved ({})", unresolved.capability_id);
            let mut status = lifecycle_label(unresolved.state).to_owned();
            if let Some(issue) = unresolved
                .issues
                .first()
                .filter(|issue| !issue.message.trim().is_empty())
            {
                status.push_str(" · ");
                status.push_str(issue.message.trim());
            }
            accounts.push(UsageAccount {
                provider_id: unresolved.provider_id.clone(),
                canonical_account_id: unresolved.capability_id.clone(),
                unresolved: true,
                provider: provider_name,
                account: account_label,
                status,
                lifecycle: unresolved.state,
                freshness_phase: UsageFreshnessPhaseV1::Failed,
                last_good_at_epoch: None,
                is_stale: false,
                windows: Vec::new(),
            });
        }

        let mut provider_order = Vec::new();
        for account in &accounts {
            if !provider_order.contains(&account.provider) {
                provider_order.push(account.provider.clone());
            }
        }
        accounts.sort_by_key(|account| {
            provider_order
                .iter()
                .position(|p| p == &account.provider)
                .unwrap_or(usize::MAX)
        });

        let notice = if projection.unresolved.is_empty() {
            None
        } else {
            Some(format!(
                "{} configured capability(s) unresolved",
                projection.unresolved.len()
            ))
        };
        Self {
            accounts,
            notice,
            generated_at_epoch: Some(projection.generated_at_epoch),
            ..Self::default()
        }
    }

    /// Apply a completed background refresh, re-anchoring selection by
    /// stable id so renames/reorders keep the operator's row. A removed
    /// selection falls back to Overview with an inline notice.
    pub fn apply_refresh(
        &mut self,
        accounts: Vec<UsageAccount>,
        notice: Option<String>,
        now: Instant,
    ) {
        self.accounts = accounts;
        self.notice = notice;
        self.last_refresh_at = Some(now);
        self.refresh_due = false;
        match &self.selected_id {
            None => self.selected = 0,
            Some(id) => {
                if let Some(pos) = self.accounts.iter().position(|a| &a.stable_id() == id) {
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
        self.selected = self.selected.min(self.accounts.len());
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
        if self.accounts.is_empty() {
            return;
        }
        let len = self.accounts.len().saturating_add(1);
        let current = self.selected.min(len - 1);
        self.selected = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta.cast_unsigned())
                .min(len.saturating_sub(1))
        };
        self.selected_id = self
            .accounts
            .get(self.selected.saturating_sub(1))
            .filter(|_| self.selected > 0)
            .map(UsageAccount::stable_id);
    }

    pub fn selected_account(&self) -> Option<&UsageAccount> {
        (self.selected > 0).then(|| self.accounts.get(self.selected - 1))?
    }

    fn overview_selected(&self) -> bool {
        self.selected == 0
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
pub fn freshness_age_label(now_epoch: i64, account: &UsageAccount) -> String {
    if account.freshness_phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_good) = account.last_good_at_epoch else {
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
    if account.is_stale
        || matches!(
            account.freshness_phase,
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
        KeyCode::Esc | KeyCode::Char('q') => state.usage.visible = false,
        KeyCode::Up | KeyCode::Char('k') => screen.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => screen.move_selection(1),
        KeyCode::Enter => screen.detail = !screen.detail,
        KeyCode::Char('r') => {
            screen.refresh_due = true;
            screen.force_refresh_pending = true;
        }
        KeyCode::PageUp => screen.scroll = screen.scroll.saturating_sub(5),
        KeyCode::PageDown => {
            screen.scroll = screen.scroll.saturating_add(5);
        }
        _ => {}
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    render_account_list(frame, columns[0], state);
    render_detail(frame, columns[1], state);
}

fn render_account_list(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let focused = true;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if screen.accounts.is_empty() {
        frame.render_widget(
            Paragraph::new("No providers configured.\n\nPress R to refresh.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    let meter_width = inner.width.saturating_sub(8) as usize;
    let now = now_epoch();
    lines.push(Line::from(Span::styled(
        format!(
            "{}Overview",
            if screen.overview_selected() {
                "▸  "
            } else {
                "   "
            }
        ),
        row_style(screen.overview_selected()),
    )));
    lines.push(Line::from(""));
    for (index, account) in screen.accounts.iter().enumerate() {
        if index == 0 || screen.accounts[index - 1].provider != account.provider {
            lines.push(Line::from(Span::styled(
                format!("  {}", account.provider),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let selected = index.saturating_add(1) == screen.selected;
        let cursor = if selected { "▸ " } else { "  " };
        let summary = account
            .windows
            .first()
            .and_then(UsageWindow::meter_percent)
            .map_or_else(
                || account.status.clone(),
                |percent| format!("{percent}% left"),
            );
        lines.push(Line::from(Span::styled(
            format!("  {cursor}{}", account.account),
            row_style(selected),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "      {} · {} · {}",
                account.status,
                summary,
                freshness_age_label(now, account)
            ),
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(window) = account.windows.first()
            && let Some(bar) = meter_line(meter_width, window.meter_percent())
        {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.meter_percent().unwrap_or(0)),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let now = now_epoch();
    let Some(account) = screen.selected_account() else {
        if screen.accounts.is_empty() {
            frame.render_widget(
                Paragraph::new("No providers configured.\n\nPress R to refresh.")
                    .block(panel("Overview"))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let mut lines = vec![Line::from("Status    available"), Line::from("")];
        if screen.refresh_in_flight() {
            lines.push(refreshing_line());
            lines.push(Line::from(""));
        }
        let width = area.width.saturating_sub(8).max(8) as usize;
        for account in &screen.accounts {
            append_overview_account(&mut lines, account, width, now);
        }
        if let Some(notice) = &screen.notice {
            lines.push(Line::from(Span::styled(
                notice.clone(),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel("Overview"))
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };
    let title = if screen.detail { "Account" } else { "Overview" };
    let mut lines = vec![
        Line::from(format!("Provider  {}", account.provider)),
        Line::from(format!("Account   {}", account.account)),
        Line::from(format!("Status    {}", account.status)),
        Line::from(format!("Freshness {}", freshness_age_label(now, account))),
        Line::from(""),
        Line::from("Limits"),
    ];
    if screen.refresh_in_flight() {
        lines.push(refreshing_line());
        lines.push(Line::from(""));
    }
    for window in &account.windows {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        let width = area.width.saturating_sub(8).max(8) as usize;
        if let Some(bar) = meter_line(width, window.meter_percent()) {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.meter_percent().unwrap_or(0)),
            )));
        }
        let detail = if window.reset.is_empty() {
            format!("  {}", window.value)
        } else {
            format!("  {} · {}", window.value, window.reset)
        };
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
    }
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(title))
            .scroll((screen.scroll, 0))
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

fn append_overview_window(lines: &mut Vec<Line<'static>>, window: &UsageWindow, width: usize) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, window.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(window.meter_percent().unwrap_or(0)),
        )));
    }
    let detail = if window.reset.is_empty() {
        format!("  {}", window.value)
    } else {
        format!("  {} · {}", window.value, window.reset)
    };
    lines.push(Line::from(detail));
}

fn append_overview_account(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    lines.push(Line::from(Span::styled(
        format!("{} · {}", account.provider, account.account),
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", freshness_age_label(now_epoch, account)),
        Style::default().fg(Color::DarkGray),
    )));
    if account.windows.is_empty() {
        lines.push(Line::from(format!("  {}", account.status)));
    } else {
        for window in &account.windows {
            append_overview_window(lines, window, width);
        }
    }
    lines.push(Line::from(""));
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
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
