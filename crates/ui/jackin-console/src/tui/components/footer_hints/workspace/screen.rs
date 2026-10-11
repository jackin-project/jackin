// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace screen footer plans and items.

use termrock::scroll::ScrollAxes;
use termrock::widgets::HintSpan;

#[expect(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal scroll-axis input flags (inline-agent/role pickers, \
              list-names focus, scroll axes per pane, show_expand) — each tracks an \
              independent scrollable-pane state consumed individually by the scroll \
              axes planner. Named-field reads match the per-pane gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFooterScrollFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub inline_picker_scroll_axes: ScrollAxes,
    pub focused_block_scroll_axes: Option<ScrollAxes>,
    pub list_names_focused: bool,
    pub list_names_scroll_axes: ScrollAxes,
    pub show_expand: bool,
    pub show_collapse: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceInlinePickerContentFacts {
    pub agent_picker_count: Option<usize>,
    pub role_picker_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceScreenFooterFacts {
    List {
        list_items: Vec<HintSpan<'static>>,
        modal_items: Option<Vec<HintSpan<'static>>>,
    },
    CreatePrelude {
        modal_items: Option<Vec<HintSpan<'static>>>,
    },
    DestructiveConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScreenFooterPlan {
    List,
    CreatePrelude,
    DestructiveConfirm,
    ScreenOwned,
}

#[must_use]
pub const fn workspace_screen_footer_plan(
    route: crate::tui::model::ConsoleManagerStageRoute,
) -> WorkspaceScreenFooterPlan {
    match route {
        crate::tui::model::ConsoleManagerStageRoute::List => WorkspaceScreenFooterPlan::List,
        crate::tui::model::ConsoleManagerStageRoute::CreatePrelude => {
            WorkspaceScreenFooterPlan::CreatePrelude
        }
        crate::tui::model::ConsoleManagerStageRoute::ConfirmDelete
        | crate::tui::model::ConsoleManagerStageRoute::ConfirmInstancePurge => {
            WorkspaceScreenFooterPlan::DestructiveConfirm
        }
        crate::tui::model::ConsoleManagerStageRoute::Editor
        | crate::tui::model::ConsoleManagerStageRoute::Settings => {
            WorkspaceScreenFooterPlan::ScreenOwned
        }
    }
}

#[must_use]
pub fn workspace_screen_footer_items(facts: WorkspaceScreenFooterFacts) -> Vec<HintSpan<'static>> {
    match facts {
        WorkspaceScreenFooterFacts::List {
            list_items,
            modal_items,
        } => modal_items.unwrap_or(list_items),
        WorkspaceScreenFooterFacts::CreatePrelude { modal_items } => {
            modal_items.unwrap_or_else(create_prelude_footer_items)
        }
        WorkspaceScreenFooterFacts::DestructiveConfirm => destructive_confirm_footer_items(),
    }
}

#[must_use]
pub fn destructive_confirm_footer_items() -> Vec<HintSpan<'static>> {
    let mut items = crate::tui::components::confirm_hint_spans();
    super::super::common::append_keyboard_help_hint(&mut items);
    items
}

#[must_use]
pub fn create_prelude_footer_items() -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Dyn("Create workspace — follow the prompts".to_owned()),
        HintSpan::GroupSep,
        // UNREGISTERABLE(create-prelude-no-keymap): Esc handled inline; no dedicated create-prelude keymap.
        super::super::key_span("Esc"),
        HintSpan::Text("cancel"),
    ];
    super::super::common::append_keyboard_help_hint(&mut items);
    items
}
