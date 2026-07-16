// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch stage progress rail and label animation.

use jackin_core::tui_theme::{accent_fg, danger_fg, scroll_track_fg, text_fg};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::components::cells::coalesce_cells;
use crate::{LaunchStage, LaunchView, StageStatus, active_stage_index};

const STAGE_PULSE_PERIOD: usize = 12;
const BLOCK_WIDTH: usize = 3;
const BLOCK_GAP: usize = 1;
const LABEL_GAP: usize = 4;
const LABEL_SIDE_OVERHANG: usize = 12;
const LABEL_EDGE_FADE_WIDTH: usize = 24;
pub const LABEL_SLIDE_FRAMES: usize = 12;
pub const PROGRESS_RAIL_WIDTH: usize =
    LaunchStage::ALL.len() * BLOCK_WIDTH + (LaunchStage::ALL.len() - 1) * BLOCK_GAP;
pub const LABEL_VIEW_WIDTH: usize = PROGRESS_RAIL_WIDTH + LABEL_SIDE_OVERHANG * 2;

pub fn render_progress(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let label_width = usize::from(area.width).min(LABEL_VIEW_WIDTH);
    let rail_height = area.height.min(2);
    let rail = Rect {
        height: rail_height,
        ..area
    };
    let lines = vec![
        blocks_line(view, frozen),
        labels_line(view, frozen, label_width),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rail);

    // Overall completion uses TermRock's determinate Progress so the fraction
    // and non-color percentage cue stay on the shared widget contract.
    if area.height >= 3 {
        let fraction = overall_stage_fraction(view);
        let theme = termrock::Theme::default();
        let progress = termrock::widgets::Progress::new(
            termrock::widgets::ProgressKind::Determinate { fraction },
            &theme,
        )
        .label(if frozen { "done" } else { "load" });
        let bar = Rect {
            y: area.y.saturating_add(2),
            height: 1,
            ..area
        };
        frame.render_widget(&progress, bar);
    }
}

#[must_use]
pub fn overall_stage_fraction(view: &LaunchView) -> f64 {
    let statuses = display_stage_statuses(view);
    if statuses.is_empty() {
        return 0.0;
    }
    let done = statuses
        .iter()
        .filter(|status| matches!(status, StageStatus::Done | StageStatus::Skipped))
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "stage counts are tiny; f64 is exact for these values"
    )]
    {
        done as f64 / statuses.len() as f64
    }
}

#[must_use]
pub fn display_stage_statuses(view: &LaunchView) -> Vec<StageStatus> {
    if view.stages.is_empty() {
        return Vec::new();
    }

    let active = active_stage_index(view);
    view.stages
        .iter()
        .enumerate()
        .map(|(index, row)| match index.cmp(&active) {
            std::cmp::Ordering::Less => {
                if row.status == StageStatus::Failed {
                    StageStatus::Failed
                } else {
                    StageStatus::Done
                }
            }
            std::cmp::Ordering::Equal => row.status,
            std::cmp::Ordering::Greater => StageStatus::Queued,
        })
        .collect()
}

/// One block per stage, filling gray (queued) -> green (done) so a glance
/// reads as a percent-complete bar; all green means loaded.
fn blocks_line(view: &LaunchView, frozen: bool) -> Line<'static> {
    let pulse = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
    let display_statuses = display_stage_statuses(view);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, status) in display_statuses.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(BLOCK_GAP)));
        }
        // Thin horizontal segments (a slim progress bar), not tall full
        // blocks: heavy `━` for reached/active stages, light `─` for queued.
        let (glyph, color) = match status {
            StageStatus::Done | StageStatus::Skipped => ('━', accent_fg()),
            StageStatus::Running => ('━', if pulse { text_fg() } else { accent_fg() }),
            StageStatus::Failed => ('━', danger_fg()),
            StageStatus::Blocked => ('━', text_fg()),
            StageStatus::Queued => ('─', scroll_track_fg()),
        };
        spans.push(Span::styled(
            glyph.to_string().repeat(BLOCK_WIDTH),
            Style::default().fg(color),
        ));
    }
    Line::from(spans)
}

#[derive(Debug, Clone, Copy)]
pub struct LabelCell {
    ch: char,
    style: Style,
}

#[must_use]
pub fn labels_line(view: &LaunchView, frozen: bool, width: usize) -> Line<'static> {
    if width == 0 || view.stages.is_empty() {
        return Line::from(String::new());
    }

    let active = active_stage_index(view);
    let bright = !frozen && (view.frame / STAGE_PULSE_PERIOD).is_multiple_of(2);
    let display_statuses = display_stage_statuses(view);
    let (strip, centers) = label_strip(view, active, bright, &display_statuses);
    let active_center = centers.get(active).copied().unwrap_or(0);
    let center = if frozen {
        active_center
    } else {
        animated_label_center(view, &centers).unwrap_or(active_center)
    };
    let start = center as isize - (width / 2) as isize;
    let cells = (0..width).map(|x| {
        let index = start + x as isize;
        let cell = if index >= 0 {
            #[expect(clippy::cast_sign_loss, reason = "index checked non-negative above")]
            let idx = index as usize;
            strip.get(idx).copied().unwrap_or_else(blank_label_cell)
        } else {
            blank_label_cell()
        };
        faded_label_cell(cell, label_edge_fade_factor(x, width))
    });
    Line::from(coalesce_cells(cells.map(|cell| (cell.ch, cell.style))))
}

#[must_use]
pub fn label_strip(
    view: &LaunchView,
    active: usize,
    bright: bool,
    display_statuses: &[StageStatus],
) -> (Vec<LabelCell>, Vec<usize>) {
    let mut cells = Vec::new();
    let mut centers = Vec::with_capacity(view.stages.len());
    for (index, row) in view.stages.iter().enumerate() {
        if index > 0 {
            cells.extend((0..LABEL_GAP).map(|_| blank_label_cell()));
        }
        let start = cells.len();
        let style = label_style_for_stage(
            display_statuses
                .get(index)
                .copied()
                .unwrap_or(StageStatus::Queued),
            index == active,
            bright,
        );
        let label = row.stage.label();
        cells.extend(label.chars().map(|ch| LabelCell { ch, style }));
        centers.push(start + label.chars().count() / 2);
    }
    (cells, centers)
}

fn label_style_for_stage(status: StageStatus, active: bool, bright: bool) -> Style {
    if active {
        return match status {
            StageStatus::Failed => jackin_core::tui_theme::danger(),
            _ if bright => jackin_core::tui_theme::text_strong(),
            _ => Style::default()
                .fg(accent_fg())
                .add_modifier(Modifier::BOLD),
        };
    }

    match status {
        StageStatus::Done | StageStatus::Skipped => jackin_core::tui_theme::text_muted(),
        StageStatus::Failed => Style::default().fg(danger_fg()),
        StageStatus::Running | StageStatus::Blocked => jackin_core::tui_theme::accent(),
        StageStatus::Queued => Style::default().fg(scroll_track_fg()),
    }
}

fn blank_label_cell() -> LabelCell {
    LabelCell {
        ch: ' ',
        style: Style::default(),
    }
}

#[must_use]
pub fn label_edge_fade_factor(index: usize, width: usize) -> f32 {
    let fade_width = LABEL_EDGE_FADE_WIDTH.min(width / 2).max(1);
    let edge_distance = index.min(width.saturating_sub(1).saturating_sub(index));
    if edge_distance >= fade_width {
        return 1.0;
    }

    let ratio = ((edge_distance + 1) as f32 / fade_width as f32).clamp(0.0, 1.0);
    ratio * ratio * 2.0f32.mul_add(-ratio, 3.0)
}

#[must_use]
pub fn faded_color(color: Color, factor: f32) -> Color {
    // TermRock owns the phosphor fade math used across the ecosystem.
    termrock::style::faded(color, factor)
}

fn faded_label_cell(cell: LabelCell, factor: f32) -> LabelCell {
    let mut style = cell.style;
    if let Some(fg) = style.fg {
        style.fg = Some(faded_color(fg, factor));
    }
    LabelCell { style, ..cell }
}

#[must_use]
pub fn animated_label_center(view: &LaunchView, centers: &[usize]) -> Option<usize> {
    let transition = view.label_transition?;
    if transition.from == transition.to {
        return None;
    }
    let from = *centers.get(transition.from)?;
    let to = *centers.get(transition.to)?;
    let elapsed = view.frame.saturating_sub(transition.start_frame);
    if elapsed >= LABEL_SLIDE_FRAMES {
        return None;
    }
    let progress = elapsed as f32 / LABEL_SLIDE_FRAMES as f32;
    let eased = 1.0 - (1.0 - progress).powi(3);
    let center = (from as f32).mul_add(1.0 - eased, to as f32 * eased);
    #[expect(
        clippy::cast_sign_loss,
        reason = "from/to are usize indices; ease stays non-negative"
    )]
    {
        Some(center.round() as usize)
    }
}
