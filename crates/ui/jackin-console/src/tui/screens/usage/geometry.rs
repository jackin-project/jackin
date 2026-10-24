// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::publication::empty_publication_body_lines;
use super::{
    UsageAccount, UsageScreenState, append_publication_age, empty_publication_lines,
    freshness_age_label, meter_line, meter_style, non_empty_label, panel, quota_state_label,
    row_style,
};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Stable semantic owner for a rendered usage-list row.
///
/// A target returned by [`UsageListGeometry::target_at`] reflects the supplied
/// prepared geometry; it does not authorize live input against a newer frame
/// or a different owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageRowTarget {
    Overview,
    Account(String),
}

/// Direction of a usage-pane wheel query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelDirection {
    Up,
    Down,
}

/// Pure semantic action produced by a usage-pane wheel query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageWheelAction {
    MoveSelection(isize),
    ScrollDetail(i8),
}

/// Shared rectangles for the rendered usage frame.
///
/// These rectangles describe the requested layout. They do not certify that a
/// particular frame was painted or that input still belongs to this layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageFrameLayout {
    pub list_outer: Rect,
    pub list_inner: Rect,
    pub detail_outer: Rect,
    pub detail_inner: Rect,
    pub detail_status: Rect,
    pub detail_body: Rect,
}

/// Prepared usage-list lines and their semantic row ownership.
///
/// Use the same inner rectangle and scroll offset as the frame being queried;
/// this geometry is not a live-input or owner-authority token.
#[derive(Debug, Clone)]
pub struct UsageListGeometry {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) row_targets: Vec<Option<UsageRowTarget>>,
    pub(crate) style: Option<Style>,
    pub(crate) scroll: bool,
    inner: Rect,
}

impl UsageListGeometry {
    /// Resolve an inner-list coordinate against this prepared geometry.
    ///
    /// `scroll` must match the offset used when preparing the corresponding
    /// frame. The result alone does not validate a later frame or input owner.
    #[must_use]
    pub fn target_at(&self, x: u16, y: u16, scroll: u16) -> Option<UsageRowTarget> {
        if !contains(self.inner, x, y) {
            return None;
        }
        let row = usize::from(scroll).saturating_add(usize::from(y.saturating_sub(self.inner.y)));
        self.row_targets.get(row).cloned().flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UsageDetailAreas {
    pub(super) status: Rect,
    pub(super) body: Rect,
}

struct TaggedLine {
    line: Line<'static>,
    target: Option<UsageRowTarget>,
}

/// Compute the shared usage-frame rectangles for `area`.
///
/// This computes layout only; callers that process live input must separately
/// validate that the corresponding frame and owner are still current.
#[must_use]
pub fn usage_frame_layout(area: Rect) -> UsageFrameLayout {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    let detail_inner = panel("Overview").inner(columns[1]);
    let detail = detail_areas(detail_inner);
    UsageFrameLayout {
        list_outer: columns[0],
        list_inner: Block::default().borders(Borders::ALL).inner(columns[0]),
        detail_outer: columns[1],
        detail_inner,
        detail_status: detail.status,
        detail_body: detail.body,
    }
}

pub(super) fn detail_areas(inner: Rect) -> UsageDetailAreas {
    if inner.width == 0 || inner.height == 0 {
        return UsageDetailAreas {
            status: Rect::new(inner.x, inner.y, 0, 0),
            body: Rect::new(inner.x, inner.y, 0, 0),
        };
    }
    let status_height = inner.height.min(1);
    UsageDetailAreas {
        status: Rect::new(inner.x, inner.y, inner.width, status_height),
        body: Rect::new(
            inner.x,
            inner.y.saturating_add(status_height),
            inner.width,
            inner.height.saturating_sub(status_height),
        ),
    }
}

/// Prepare the usage list's exact lines and semantic ownership for one frame.
///
/// `inner` and the screen's scroll offset must be the ones used by the renderer.
/// The resulting query describes prepared paint geometry but does not itself
/// authorize live input or establish that a later frame is unchanged.
#[must_use]
pub fn list_geometry(
    screen: &UsageScreenState,
    inner: Rect,
    now_epoch: i64,
    full_route: bool,
) -> UsageListGeometry {
    let mut tagged = Vec::new();
    let mut style = None;
    let mut scroll = false;

    if screen.accounts.is_empty() {
        let lines = if full_route {
            empty_publication_body_lines(screen, now_epoch)
        } else {
            empty_publication_lines(screen, now_epoch)
        };
        tagged.extend(
            lines
                .into_iter()
                .map(|line| TaggedLine { line, target: None }),
        );
        style = Some(Style::default().fg(Color::DarkGray));
    } else {
        let mut publication_age = Vec::new();
        append_publication_age(&mut publication_age, screen, now_epoch);
        tagged.extend(
            publication_age
                .into_iter()
                .map(|line| TaggedLine { line, target: None }),
        );
        let meter_width = inner.width.saturating_sub(8) as usize;
        push_line(
            &mut tagged,
            Line::from(Span::styled(
                format!(
                    "  s:sort({}) f:filter({}) c:constrained",
                    screen.sort.label(),
                    screen.filter.label()
                ),
                Style::default().fg(Color::DarkGray),
            )),
            None,
        );
        push_line(&mut tagged, Line::from(""), None);
        let order = screen.visible_order();
        if order.is_empty() {
            push_line(
                &mut tagged,
                Line::from(Span::styled(
                    format!("  No accounts match filter '{}'.", screen.filter.label()),
                    Style::default().fg(Color::DarkGray),
                )),
                None,
            );
            push_line(
                &mut tagged,
                Line::from(Span::styled(
                    "  Press f to cycle the filter.",
                    Style::default().fg(Color::DarkGray),
                )),
                None,
            );
            scroll = true;
        } else {
            let overview_selected = screen.overview_selected();
            push_line(
                &mut tagged,
                Line::from(Span::styled(
                    format!("{}Overview", if overview_selected { "▸  " } else { "   " }),
                    row_style(overview_selected),
                )),
                Some(UsageRowTarget::Overview),
            );
            push_line(&mut tagged, Line::from(""), None);
            for (position, &index) in order.iter().enumerate() {
                let account = &screen.accounts[index];
                if position == 0
                    || screen.accounts[order[position - 1]].provider != account.provider
                {
                    push_line(
                        &mut tagged,
                        Line::from(Span::styled(
                            format!("  {}", account.provider),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        )),
                        None,
                    );
                }
                append_account_list_rows(
                    &mut tagged,
                    account,
                    position.saturating_add(1) == screen.selected,
                    meter_width,
                    now_epoch,
                );
            }
            scroll = true;
        }
    }

    let mut row_targets = Vec::new();
    if inner.width > 0 {
        for tagged_line in &tagged {
            let rows = Paragraph::new(vec![tagged_line.line.clone()])
                .wrap(Wrap { trim: false })
                .line_count(inner.width);
            row_targets.extend(std::iter::repeat_n(tagged_line.target.clone(), rows));
        }
    }
    UsageListGeometry {
        lines: tagged.into_iter().map(|entry| entry.line).collect(),
        row_targets,
        style,
        scroll,
        inner,
    }
}

/// Map a wheel coordinate to a usage-local action for the prepared frame.
///
/// The supplied layout and screen must describe the frame being processed.
/// The action is pure and carries no live owner, revision, or frame authority.
#[must_use]
pub fn usage_wheel_action(
    layout: UsageFrameLayout,
    screen: &UsageScreenState,
    x: u16,
    y: u16,
    direction: WheelDirection,
) -> Option<UsageWheelAction> {
    if contains(layout.list_inner, x, y) && !screen.visible_order().is_empty() {
        return Some(UsageWheelAction::MoveSelection(match direction {
            WheelDirection::Up => -1,
            WheelDirection::Down => 1,
        }));
    }
    if contains(layout.detail_body, x, y) {
        return Some(UsageWheelAction::ScrollDetail(match direction {
            WheelDirection::Up => -3,
            WheelDirection::Down => 3,
        }));
    }
    None
}

fn append_account_list_rows(
    lines: &mut Vec<TaggedLine>,
    account: &UsageAccount,
    selected: bool,
    meter_width: usize,
    now_epoch: i64,
) {
    let target = UsageRowTarget::Account(account.stable_id());
    let cursor = if selected { "▸ " } else { "  " };
    let summary = account.summary_window().map_or_else(
        || account.status.clone(),
        |window| {
            if window.value.trim().is_empty() {
                quota_state_label(window.quota_state).to_owned()
            } else {
                window.value.clone()
            }
        },
    );
    push_line(
        lines,
        Line::from(Span::styled(
            format!("  {cursor}{}", account.account),
            row_style(selected),
        )),
        Some(target.clone()),
    );
    let mut sub = format!(
        "      {} · {} · {}",
        account.status,
        summary,
        freshness_age_label(now_epoch, account)
    );
    if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
        sub.push_str(&format!(" · {plan}"));
    }
    let issue_count = account.issue_count();
    if issue_count > 0 {
        sub.push_str(&format!(
            " · {} issue{}",
            issue_count,
            if issue_count == 1 { "" } else { "s" }
        ));
    }
    push_line(
        lines,
        Line::from(Span::styled(sub, Style::default().fg(Color::DarkGray))),
        Some(target.clone()),
    );
    if let Some(window) = account.summary_window()
        && let Some(bar) = meter_line(meter_width, window.meter_geometry())
    {
        push_line(
            lines,
            Line::from(Span::styled(bar, meter_style(window.quota_state))),
            Some(target),
        );
    }
}

fn push_line(lines: &mut Vec<TaggedLine>, line: Line<'static>, target: Option<UsageRowTarget>) {
    lines.push(TaggedLine { line, target });
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    area.width > 0
        && area.height > 0
        && x >= area.x
        && y >= area.y
        && x < area.x.saturating_add(area.width)
        && y < area.y.saturating_add(area.height)
}
