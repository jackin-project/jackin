// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared `OpRef.path` breadcrumb spans.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::tui::op_breadcrumb::parse_path_breadcrumb;

use jackin_ui::theme::{accent_fg, text_fg};

/// Render an `OpRef.path` as `vault / item [subtitle] / section -> field ?query`.
pub fn push_op_breadcrumb_spans(spans: &mut Vec<Span<'static>>, path: &str) {
    let dim = jackin_ui::theme::text_muted();
    let white_style = Style::default().fg(text_fg());
    let green = jackin_ui::theme::accent();
    let green_bold = Style::default()
        .fg(accent_fg())
        .add_modifier(Modifier::BOLD);
    let Some(parts) = parse_path_breadcrumb(path) else {
        spans.push(Span::styled("<unparseable path - re-pick>", dim));
        return;
    };
    spans.push(Span::styled(parts.vault, white_style));
    spans.push(Span::styled(" / ", dim));
    spans.push(Span::styled(parts.item, green));
    if let Some(subtitle) = parts.item_subtitle {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(subtitle, dim));
    }
    if let Some(section) = parts.section {
        spans.push(Span::styled(" / ", dim));
        spans.push(Span::styled(section, green));
    }
    spans.push(Span::styled(" \u{2192} ", dim));
    spans.push(Span::styled(parts.field, green_bold));
    if let Some(query) = parts.attribute_query {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(query, dim));
    }
}
