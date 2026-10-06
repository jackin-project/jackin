// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `progress_rail`.

use ratatui::style::{Color, Modifier};

use super::{blocks_line, label_style_for_stage};
use crate::{StageStatus, initial_view};

#[test]
fn rail_text_spans_keep_pre_bump_white() {
    let mut running = initial_view();
    running.stages[0].status = StageStatus::Running;
    // frame 0 keeps the pulse phase on.
    let running_line = blocks_line(&running, false);
    assert_eq!(running_line.spans[0].content.as_ref(), "━━━");
    assert_eq!(
        running_line.spans[0].style.fg,
        Some(Color::Rgb(255, 255, 255))
    );

    let mut blocked = initial_view();
    blocked.stages[0].status = StageStatus::Blocked;
    let blocked_line = blocks_line(&blocked, false);
    assert_eq!(blocked_line.spans[0].content.as_ref(), "━━━");
    assert_eq!(
        blocked_line.spans[0].style.fg,
        Some(Color::Rgb(255, 255, 255))
    );
}

#[test]
fn rail_strong_span_keeps_pre_bump_white_bold() {
    let style = label_style_for_stage(StageStatus::Running, true, true);
    assert_eq!(style.fg, Some(Color::Rgb(255, 255, 255)));
    assert!(style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn rail_muted_span_keeps_pre_bump_dim_phosphor() {
    let style = label_style_for_stage(StageStatus::Done, false, false);
    assert_eq!(style.fg, Some(Color::Rgb(0, 140, 30)));
    assert!(style.add_modifier.is_empty());
}

#[test]
fn rail_queued_span_keeps_pre_bump_dark_phosphor() {
    let style = label_style_for_stage(StageStatus::Queued, false, false);
    assert_eq!(style.fg, Some(Color::Rgb(0, 80, 18)));
    assert!(style.add_modifier.is_empty());

    let mut view = initial_view();
    view.stages[0].status = StageStatus::Running;
    let line = blocks_line(&view, false);
    // spans: [running block, gap, queued block, ...]
    assert_eq!(line.spans[2].content.as_ref(), "───");
    assert_eq!(line.spans[2].style.fg, Some(Color::Rgb(0, 80, 18)));
}
