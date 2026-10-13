// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `header`.

use ratatui::style::{Color, Modifier};

use super::brand_header_line;

#[test]
fn cockpit_brand_chevron_keeps_pre_bump_white() {
    let line = brand_header_line("x");
    let chevron = &line.spans[1];
    assert_eq!(chevron.content, "❯");
    assert_eq!(chevron.style.fg, Some(Color::Rgb(255, 255, 255)));
    assert_eq!(chevron.style.bg, Some(Color::Rgb(0, 255, 65)));
    assert!(chevron.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn cockpit_brand_separator_keeps_pre_bump_dark_phosphor() {
    let line = brand_header_line("x");
    let separator = &line.spans[3];
    assert_eq!(separator.content, " · ");
    assert_eq!(separator.style.fg, Some(Color::Rgb(0, 80, 18)));
    assert_eq!(separator.style.bg, None);
}

#[test]
fn cockpit_brand_label_keeps_pre_bump_dim_phosphor() {
    let line = brand_header_line("x");
    let label = &line.spans[4];
    assert_eq!(label.content, "x");
    assert_eq!(label.style.fg, Some(Color::Rgb(0, 140, 30)));
    assert_eq!(label.style.bg, None);
    assert!(label.style.add_modifier.is_empty());
}
