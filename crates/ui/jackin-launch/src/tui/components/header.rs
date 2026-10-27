// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::LaunchView;
use crate::tui::components::cells::coalesce_cells;

fn brand_header_line(label: &str) -> Line<'static> {
    let block = Style::default()
        .bg(jackin_tui::tokens::BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" jackin", block.fg(jackin_tui::tokens::INK)),
        Span::styled("❯", block.fg(jackin_tui::tokens::BRAND_CHEVRON)),
        Span::styled(" ", block),
        Span::styled(
            " · ",
            Style::default().fg(jackin_tui::tokens::BRAND_SEPARATOR),
        ),
        Span::styled(
            label.to_owned(),
            Style::default().fg(jackin_tui::tokens::BRAND_LABEL),
        ),
    ])
}

pub fn render_cockpit_header(frame: &mut Frame<'_>, area: Rect, view: &LaunchView, frozen: bool) {
    let mut brand_line = brand_header_line("launch");
    brand_line.spans.pop();
    brand_line.spans.extend(loading_line_spans(view, frozen));
    frame.render_widget(Paragraph::new(brand_line), area);
}

fn loading_line_spans(view: &LaunchView, frozen: bool) -> Vec<Span<'static>> {
    let Some(id) = view.identity.as_ref() else {
        return vec![Span::styled(
            "Preparing launch...",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        )];
    };
    let prep = " in ";
    let mut chars: Vec<(char, u8)> = Vec::new();
    for ch in "Loading ".chars() {
        chars.push((ch, 0));
    }
    for ch in id.role.chars() {
        chars.push((ch, 1));
    }
    for ch in prep.chars() {
        chars.push((ch, 0));
    }
    for ch in id.target_label.chars() {
        chars.push((ch, 2));
    }

    let len = chars.len();
    #[expect(
        clippy::cast_sign_loss,
        reason = "sweep brightness is non-negative; lerp stays in u8 range"
    )]
    let lerp = |a: u8, b: u8, t: f32| (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)) as u8;
    let period = (len + 16) as f32;
    let peak = (view.frame as f32 % period) - 8.0;
    coalesce_cells(chars.into_iter().enumerate().map(|(i, (ch, kind))| {
        let bright = if frozen {
            0.0
        } else {
            (1.0 - (i as f32 - peak).abs() / 5.0).max(0.0)
        };
        let color = if kind == 0 {
            Color::Rgb(
                lerp(0, 120, bright),
                lerp(140, 255, bright),
                lerp(30, 120, bright),
            )
        } else {
            let v = lerp(170, 255, bright);
            Color::Rgb(v, v, v)
        };
        let mut style = Style::default().fg(color);
        if kind != 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        (ch, style)
    }))
}
