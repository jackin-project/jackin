// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `footer_hints`.

use super::*;

fn has_help_hint(items: &[HintSpan<'static>]) -> bool {
    items.windows(2).any(|pair| {
        matches!(&pair[0], HintSpan::Key("?")) && matches!(&pair[1], HintSpan::Text("help"))
    })
}

#[test]
fn every_stage_footer_builder_advertises_help_hint() {
    assert!(has_help_hint(&tab_bar_footer_items("save", true, None)));
    assert!(has_help_hint(&content_footer_items(
        "save",
        Vec::new(),
        None
    )));
    assert!(has_help_hint(&create_prelude_footer_items()));
    assert!(has_help_hint(&destructive_confirm_footer_items()));
    assert!(has_help_hint(&workspace_list_footer_items(
        WorkspaceListFooterMode::PreviewPane
    )));
    assert!(has_help_hint(&workspace_list_footer_items(
        WorkspaceListFooterMode::InstanceRow {
            has_snapshot: false,
            is_live: true,
        }
    )));
    assert!(has_help_hint(&workspace_list_footer_items(
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_axes: termrock::scroll::ScrollAxes::none(),
            enter_label: "launch",
            is_saved: true,
            show_prewarm: false,
            show_expand: false,
            show_collapse: false,
            show_open_in_github: false,
        }
    )));
}

#[test]
fn picker_footers_do_not_advertise_help_hint() {
    // Inline pickers own `?` as typed input (dispatch precedence), so their
    // footers must not advertise the overlay.
    assert!(!has_help_hint(&workspace_list_footer_items(
        WorkspaceListFooterMode::AgentPicker {
            scroll_axes: termrock::scroll::ScrollAxes::none(),
        }
    )));
    assert!(!has_help_hint(&workspace_list_footer_items(
        WorkspaceListFooterMode::RolePicker {
            scroll_axes: termrock::scroll::ScrollAxes::none(),
        }
    )));
}
