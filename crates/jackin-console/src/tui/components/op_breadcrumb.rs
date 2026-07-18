// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared `OpRef.path` breadcrumb spans.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::tui::op_breadcrumb::parse_path_breadcrumb;

/// Render an `OpRef.path` as `vault / item [subtitle] / section -> field ?query`.
pub fn push_op_breadcrumb_spans(spans: &mut Vec<Span<'static>>, path: &str) {
    let dim = termrock::Theme::default().style(termrock::style::Role::TextMuted);
    let white_style = Style::default().fg(termrock::Theme::default()
        .style(termrock::style::Role::Text)
        .fg
        .unwrap_or_default());
    let green = termrock::Theme::default().style(termrock::style::Role::Accent);
    let green_bold = Style::default()
        .fg(termrock::Theme::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default())
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
