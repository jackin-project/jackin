// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_tui::tokens::ACTION_ACCENT;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier};

#[test]
fn selected_action_row_uses_high_contrast_list_fill() {
    let style = action_row_style(true);

    assert_eq!(style.fg, Some(Color::Black));
    assert_eq!(
        style.bg,
        Some(
            termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default()
        )
    );
    assert!(style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn unselected_action_row_keeps_action_accent() {
    let style = action_row_style(false);

    assert_eq!(style.fg, Some(ACTION_ACCENT));
    assert_eq!(style.bg, None);
    assert!(!style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn auth_source_display_maps_secret_value_state() {
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain("secret".to_owned())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::MaskedPlain { chars: 6 },
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::OpRefPath("Vault/Item/key".to_owned())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::OpRefPath("Vault/Item/key".to_owned()),
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain(String::new())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::Unset {
            env_name: "API_KEY".to_owned(),
            mode_label: "api-key".to_owned(),
        },
    );
}

#[test]
fn auth_source_display_returns_not_required_without_env() {
    assert_eq!(
        auth_source_display_for_required_env(
            None,
            Some(AuthSourceValue::Plain("secret".to_owned())),
            "ignore",
        ),
        AuthSourceDisplay::NotRequired,
    );
}

#[test]
fn tab_regions_match_real_widget_paint_with_unicode_and_focus() {
    let area = Rect::new(5, 3, 78, 2);
    let labels = [
        ("Genéral🤖", true),
        ("Mounts", false),
        ("仓库", false),
        ("e\u{301}nv", false),
        ("A very long tab label", false),
    ];

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 10)).unwrap();
    terminal
        .draw(|frame| render_tab_strip(frame, area, &labels, true, Some(2)))
        .unwrap();

    let tabs = labels
        .iter()
        .enumerate()
        .map(|(id, (label, active))| termrock::widgets::Tab::new(id, label).active(*active))
        .collect::<Vec<_>>();
    let system = termrock::style::DesignSystem::default();
    let mut expected = ratatui::buffer::Buffer::empty(area);
    let mut state = termrock::widgets::TabsState::default();
    state.selected = Some(0);
    state.hovered = Some(2);
    state.focused = true;
    termrock::widgets::Tabs::new(&tabs, &system)
        .gap(termrock::widgets::TAB_GAP)
        .paint(area, &mut expected, &mut state);

    let rendered = terminal.backend().buffer();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let actual_cell = &rendered[(x, y)];
            let expected_cell = &expected[(x, y)];
            assert_eq!(actual_cell.symbol(), expected_cell.symbol(), "({x}, {y})");
            assert_eq!(actual_cell.fg, expected_cell.fg, "({x}, {y})");
            assert_eq!(actual_cell.bg, expected_cell.bg, "({x}, {y})");
            assert_eq!(actual_cell.modifier, expected_cell.modifier, "({x}, {y})");
        }
    }

    let regions = tab_strip_hit_regions(area, &labels, true, Some(2));
    let actual_regions = regions
        .iter()
        .map(|region| (region.id, region.area))
        .collect::<Vec<_>>();
    let painted_regions = state
        .regions
        .iter()
        .map(|region| (region.id, region.area))
        .collect::<Vec<_>>();
    assert_eq!(actual_regions, painted_regions);
    assert!(regions.iter().all(|region| {
        area.contains(Position::new(region.area.x, region.area.y))
            && area.right() >= region.area.right()
            && area.bottom() >= region.area.bottom()
    }));
}
