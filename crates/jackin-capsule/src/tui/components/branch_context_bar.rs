// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Branch context bar component: renders the current git branch and
//! ahead/behind counts in the capsule status bar.
//!
//! Not responsible for: fetching git state (caller provides `branch` and
//! `PullRequestInfo`) or mouse-event dispatch.
//!
//! Key invariant: `BRANCH_CONTEXT_BAR_ROWS` is the exact row budget callers
//! must reserve; rendering writes ANSI escape sequences directly into the
//! caller-supplied `buf` using absolute cursor positions.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::StatefulWidget,
};
use termrock::widgets::{StatusBar, StatusBarState, StatusSlot};

use crate::pull_request::PullRequestInfo;

pub const BRANCH_CONTEXT_BAR_ROWS: u16 = 1;

/// Half-open `[start, end)` column range. Constructor returns `None`
/// when `end <= start` so the renderer / hit-tester can rely on
/// `end > start` for every alive region without re-checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColRange {
    pub(crate) start: u16,
    pub(crate) end: u16,
}

impl ColRange {
    pub(crate) fn new(start: u16, end: u16) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }

    pub(crate) fn contains(self, col: u16) -> bool {
        col >= self.start && col < self.end
    }
}

pub(crate) struct BranchContextBarLayout {
    pub(crate) left_region: Option<ColRange>,
    pub(crate) usage_region: Option<ColRange>,
    pub(crate) debug_chip_region: Option<ColRange>,
    pub(crate) container_region: Option<ColRange>,
}

pub(crate) fn visible_branch(branch: Option<&str>, is_default_branch: bool) -> Option<&str> {
    branch.filter(|_| !is_default_branch)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchBarSlot {
    Context,
    Usage,
    Container,
    RunId,
}

fn col_range(region: &termrock::interaction::HitRegion<BranchBarSlot>) -> Option<ColRange> {
    ColRange::new(
        region.area.x.saturating_add(1),
        region.area.right().saturating_add(1),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn branch_context_bar_layout(
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    container_name: &str,
) -> Option<BranchContextBarLayout> {
    if term_rows == 0 || term_cols == 0 {
        return None;
    }
    // `branch` is the post-filter visible branch. Trust the input here so
    // renderer / layout / hit-test helpers stay default-branch-agnostic.
    let (context_left, left_clickable) = match (pull_request, branch) {
        (Some(pr), _) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        (None, Some(b)) if pull_request_loading => (format!(" Resolving PR · {b} "), true),
        (None, Some(b)) => (format!(" Branch · {b} "), true),
        (None, None) => (String::new(), false),
    };
    let container = format!(" {container_name} ");
    let run_id = debug_run_id
        .map(|value| format!(" {value} "))
        .unwrap_or_default();
    let usage = usage_content(term_cols, usage_status_label, &container, &run_id);
    let left = [status_slot(
        BranchBarSlot::Context,
        &context_left,
        1,
        left_clickable,
    )];
    let right = [
        status_slot(BranchBarSlot::Usage, &usage, 2, !usage.is_empty()),
        status_slot(
            BranchBarSlot::Container,
            &container,
            3,
            !container_name.is_empty(),
        ),
        status_slot(BranchBarSlot::RunId, &run_id, 4, !run_id.is_empty()),
    ];
    let theme = termrock::Theme::default();
    let regions = StatusBar::new(&left, &right, &theme).regions(Rect::new(
        0,
        term_rows.saturating_sub(1),
        term_cols,
        1,
    ));
    let region = |id| {
        regions
            .iter()
            .find(|region| region.id == id)
            .and_then(col_range)
    };
    Some(BranchContextBarLayout {
        left_region: left_clickable
            .then(|| region(BranchBarSlot::Context))
            .flatten(),
        usage_region: region(BranchBarSlot::Usage),
        debug_chip_region: region(BranchBarSlot::RunId),
        container_region: region(BranchBarSlot::Container),
    })
}

fn status_slot(
    id: BranchBarSlot,
    content: &str,
    priority: u8,
    enabled: bool,
) -> StatusSlot<'_, BranchBarSlot> {
    StatusSlot {
        id,
        content,
        priority,
        min_width: u16::from(id == BranchBarSlot::Context),
        enabled,
        style: Style::default(),
        hover_style: None,
    }
}

fn usage_content(width: u16, label: Option<&str>, container: &str, run_id: &str) -> String {
    let Some(label) = label.filter(|label| !label.is_empty()) else {
        return String::new();
    };
    let reserved = termrock::text::display_cols(container)
        .saturating_add(termrock::text::display_cols(run_id))
        .saturating_add(1);
    let available = usize::from(width).saturating_sub(reserved);
    let full = format!(" {label} ");
    if termrock::text::display_cols(&full) <= available {
        return full;
    }
    let compact = format!(" {} ", compact_usage_status_label(label));
    if termrock::text::display_cols(&compact) <= available {
        compact
    } else {
        String::new()
    }
}

fn compact_usage_status_label(label: &str) -> String {
    let parts = label
        .split(" · ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let remaining = parts
        .iter()
        .find(|part| part.starts_with("Session ") || part.starts_with("5-hour "))
        .or_else(|| parts.iter().find(|part| part.contains('%')))
        .map(|part| (*part).to_owned());
    let state = parts.iter().rev().find_map(|part| {
        let lower = part.to_ascii_lowercase();
        [
            "login",
            "secret",
            "stale",
            "unsupported",
            "unavailable",
            "error",
        ]
        .into_iter()
        .find(|word| lower.contains(word))
    });
    match (remaining, state) {
        (Some(remaining), Some(state)) => format!("{remaining} · {state}"),
        (Some(remaining), None) => remaining,
        (None, Some(state)) => state.to_owned(),
        (None, None) => label
            .split_whitespace()
            .next()
            .unwrap_or("usage")
            .to_owned(),
    }
}

pub(crate) struct BranchContextBarView<'a> {
    pub(crate) branch: Option<&'a str>,
    pub(crate) usage_status_label: Option<&'a str>,
    pub(crate) pull_request: Option<&'a PullRequestInfo>,
    pub(crate) pull_request_loading: bool,
    pub(crate) debug_run_id: Option<&'a str>,
    pub(crate) container_name: &'a str,
    pub(crate) hover_target: Option<crate::tui::model::HoverTarget>,
}

pub(crate) fn render_branch_context_bar(
    buffer: &mut Buffer,
    area: Rect,
    view: BranchContextBarView<'_>,
) {
    use crate::tui::model::HoverTarget;
    let BranchContextBarView {
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading,
        debug_run_id,
        container_name,
        hover_target,
    } = view;
    let (left_text, left_clickable) = match (pull_request, branch) {
        (Some(pr), _) => (format!(" PR {} · {} ", pr.number_label(), pr.title), true),
        (None, Some(value)) if pull_request_loading => (format!(" Resolving PR · {value} "), true),
        (None, Some(value)) => (format!(" Branch · {value} "), true),
        (None, None) => (String::new(), false),
    };
    let container = format!(" {container_name} ");
    let run = debug_run_id
        .map(|value| format!(" {value} "))
        .unwrap_or_default();
    let usage = usage_content(area.width, usage_status_label, &container, &run);
    let white_bg = Style::default().bg(jackin_core::tui_theme::WHITE);
    let left = [StatusSlot {
        style: white_bg
            .fg(if left_clickable {
                jackin_core::tui_theme::LINK_BLUE
            } else {
                jackin_core::tui_theme::INK
            })
            .add_modifier(Modifier::BOLD),
        hover_style: Some(
            white_bg
                .fg(jackin_core::tui_theme::DEBUG_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        ..status_slot(BranchBarSlot::Context, &left_text, 1, left_clickable)
    }];
    let right = [
        StatusSlot {
            style: white_bg
                .fg(jackin_core::tui_theme::INK)
                .add_modifier(Modifier::BOLD),
            hover_style: Some(
                white_bg
                    .fg(jackin_core::tui_theme::DEBUG_AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            ..status_slot(BranchBarSlot::Usage, &usage, 2, !usage.is_empty())
        },
        StatusSlot {
            style: white_bg
                .fg(jackin_core::tui_theme::LINK_BLUE)
                .add_modifier(Modifier::BOLD),
            hover_style: Some(
                white_bg
                    .fg(jackin_core::tui_theme::DEBUG_AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            ..status_slot(
                BranchBarSlot::Container,
                &container,
                3,
                !container_name.is_empty(),
            )
        },
        StatusSlot {
            style: Style::default()
                .bg(jackin_core::tui_theme::DANGER_RED)
                .fg(jackin_core::tui_theme::WHITE)
                .add_modifier(Modifier::BOLD),
            hover_style: Some(
                Style::default()
                    .bg(jackin_core::tui_theme::WHITE)
                    .fg(jackin_core::tui_theme::DANGER_RED)
                    .add_modifier(Modifier::BOLD),
            ),
            ..status_slot(BranchBarSlot::RunId, &run, 4, !run.is_empty())
        },
    ];
    let hovered = match hover_target {
        Some(HoverTarget::BranchContext) => Some(BranchBarSlot::Context),
        Some(HoverTarget::UsageStatus) => Some(BranchBarSlot::Usage),
        Some(HoverTarget::Container) => Some(BranchBarSlot::Container),
        Some(HoverTarget::DebugChip) => Some(BranchBarSlot::RunId),
        _ => None,
    };
    let theme = termrock::Theme::default().with_role(
        termrock::style::Role::StatusBar,
        white_bg.fg(jackin_core::tui_theme::INK),
    );
    (&StatusBar::new(&left, &right, &theme)).render(
        area,
        buffer,
        &mut StatusBarState {
            hovered,
            regions: Vec::new(),
        },
    );
}

pub(crate) fn debug_run_id_label() -> Option<String> {
    if !crate::logging::debug_enabled() {
        return None;
    }
    std::env::var("JACKIN_INVOCATION_ID")
        .ok()
        .filter(|id| !id.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchContextBarHit {
    Context,
    UsageStatus,
    Container,
    /// Click on the debug run-id chip (only shown when `--debug` is active).
    DebugChip,
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn branch_context_bar_hit(
    row: u16,
    col: u16,
    term_rows: u16,
    term_cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    pull_request_loading: bool,
    debug_run_id: Option<&str>,
    container_name: &str,
) -> Option<BranchContextBarHit> {
    if row != term_rows {
        return None;
    }
    let layout = branch_context_bar_layout(
        term_rows,
        term_cols,
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading,
        debug_run_id,
        container_name,
    )?;
    if layout.debug_chip_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::DebugChip);
    }
    if layout.container_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Container);
    }
    if layout.usage_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::UsageStatus);
    }
    if layout.left_region.is_some_and(|r| r.contains(col)) {
        return Some(BranchContextBarHit::Context);
    }
    None
}

#[cfg(test)]
mod tests;
