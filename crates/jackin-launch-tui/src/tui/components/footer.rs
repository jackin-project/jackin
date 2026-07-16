// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch cockpit footer helpers.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use termrock::interaction::HitRegion;
use termrock::widgets::{StatusBar, StatusBarState, StatusSlot};

use crate::LaunchView;
use crate::tui::components::chrome::{BottomChromeAreas, bottom_chrome_areas};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent footer hit regions may be hovered simultaneously"
)]
pub struct StatusFooterHover {
    pub left: bool,
    pub usage: bool,
    pub right: bool,
    pub right_debug: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterSlot {
    Activity,
    Container,
    RunId,
}

pub fn footer_regions(
    area: Rect,
    activity: &str,
    instance: &str,
    run_id: Option<&str>,
) -> Vec<HitRegion<FooterSlot>> {
    let activity = format!(" {activity}");
    let container = format!(" {instance} ");
    let run = run_id.map(|value| format!(" {value} ")).unwrap_or_default();
    let left = [StatusSlot {
        id: FooterSlot::Activity,
        content: &activity,
        priority: 0,
        min_width: 0,
        enabled: true,
        style: Style::default(),
        hover_style: None,
    }];
    let right = [
        StatusSlot {
            id: FooterSlot::Container,
            content: &container,
            priority: 0,
            min_width: 0,
            enabled: !instance.is_empty(),
            style: Style::default(),
            hover_style: None,
        },
        StatusSlot {
            id: FooterSlot::RunId,
            content: &run,
            priority: 0,
            min_width: 0,
            enabled: run_id.is_some_and(|value| !value.is_empty()),
            style: Style::default(),
            hover_style: None,
        },
    ];
    StatusBar {
        left: &left,
        right: &right,
        style: Style::default(),
        alpha: 1.0,
    }
    .regions(area)
}

/// The status-bar activity text: the current step with an upper-cased first
/// word and a trailing ellipsis (`wiring private network` -> `Wiring private
/// network...`). The live build/step detail lives only here, never inside the
/// box.
#[must_use]
pub fn format_activity(status: &str) -> String {
    let trimmed = status
        .trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}…", first.to_uppercase(), chars.as_str())
}

pub fn render_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
) {
    let instance = footer_instance(view);
    // The run id rides the status bar only in --debug, in amber, so the
    // operator is never unsure whether they are in a debug run; the blue
    // instance-id chip always shows once the container is named.
    let debug_chip = debug_mode.then_some(run_id);
    // Fade the bar up from black over the first ~30 frames so it appears
    // gradually with the rain rather than popping in.
    #[expect(
        clippy::cast_precision_loss,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    let alpha = (view.frame as f32 / 30.0).min(1.0);
    let activity = format!(" {}", format_activity(&view.status));
    let container = format!(" {instance} ");
    let run = debug_chip
        .map(|value| format!(" {value} "))
        .unwrap_or_default();
    let left = [StatusSlot {
        id: FooterSlot::Activity,
        content: &activity,
        priority: 0,
        min_width: 0,
        enabled: true,
        style: Style::default()
            .bg(termrock::style::WHITE)
            .fg(if view.footer_hover.left {
                termrock::style::LINK_BLUE
            } else {
                termrock::style::INK
            })
            .add_modifier(Modifier::BOLD),
        hover_style: None,
    }];
    let right = [
        StatusSlot {
            id: FooterSlot::Container,
            content: &container,
            priority: 0,
            min_width: 0,
            enabled: !instance.is_empty(),
            style: Style::default()
                .bg(termrock::style::WHITE)
                .fg(if view.footer_hover.right {
                    termrock::style::DEBUG_AMBER
                } else {
                    termrock::style::LINK_BLUE
                })
                .add_modifier(Modifier::BOLD),
            hover_style: None,
        },
        StatusSlot {
            id: FooterSlot::RunId,
            content: &run,
            priority: 0,
            min_width: 0,
            enabled: debug_chip.is_some_and(|value| !value.is_empty()),
            style: Style::default()
                .bg(if view.footer_hover.right_debug {
                    termrock::style::WHITE
                } else {
                    termrock::style::DANGER_RED
                })
                .fg(if view.footer_hover.right_debug {
                    termrock::style::DANGER_RED
                } else {
                    termrock::style::WHITE
                })
                .add_modifier(Modifier::BOLD),
            hover_style: None,
        },
    ];
    frame.render_stateful_widget(
        &StatusBar {
            left: &left,
            right: &right,
            style: Style::default()
                .bg(termrock::style::WHITE)
                .fg(termrock::style::INK),
            alpha,
        },
        area,
        &mut StatusBarState {
            hovered: None,
            regions: Vec::new(),
        },
    );
}

#[must_use]
pub const fn launch_overlay_chrome_areas(area: Rect, debug_mode: bool) -> BottomChromeAreas {
    if debug_mode {
        return bottom_chrome_areas(area);
    }
    // spacer and footer collapse to a zero-height row past the bottom edge.
    let collapsed = Rect {
        x: area.x,
        y: area.y + area.height,
        width: area.width,
        height: 0,
    };
    BottomChromeAreas {
        body: Rect {
            height: area.height.saturating_sub(1),
            ..area
        },
        hint: Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: if area.height >= 1 { 1 } else { 0 },
        },
        spacer: collapsed,
        footer: collapsed,
    }
}

/// The container's short instance id once the container is named, else empty.
#[must_use]
pub fn footer_instance(view: &LaunchView) -> String {
    view.identity
        .as_ref()
        .and_then(|identity| identity.container.as_deref())
        .and_then(jackin_core::instance_id_from_container_base)
        .map(str::to_owned)
        .unwrap_or_default()
}
