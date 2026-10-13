// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List pre-render plans and inline states.

use crate::tui::components::{
    account_picker::AccountPickerState,
    agent_choice::{AgentChoice, AgentChoiceState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMouseWheelPlan {
    Horizontal {
        delta: i16,
        vertical_fallback: Option<i16>,
    },
    Vertical(i16),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderFocusPlan {
    pub list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    pub list_names_focused: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal scroll-reset flags (reset_workspace, reset_global, \
              reset_role_global, reset_roles) — each is an independent reset \
              channel the list-pre-render plan applies to the corresponding scroll \
              area. Named-field reads match the per-area reset gating."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderScrollResetPlan {
    pub reset_workspace: bool,
    pub reset_global: bool,
    pub reset_role_global: bool,
    pub reset_roles: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal list pre-render state flags (list_names_focused, \
              preview_focused, sidebar_available, focused_block_scrollable, \
              role_global_available, roles_available) — each tracks an independent \
              UI-scroll-availability signal consumed individually by the focus and \
              scroll-reset plans. Named-field reads match the per-pane gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderFacts {
    pub list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    pub list_names_focused: bool,
    pub preview_focused: bool,
    pub sidebar_available: bool,
    pub focused_block_scrollable: bool,
    pub role_global_available: bool,
    pub roles_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderPlan {
    pub scroll_reset: ListPreRenderScrollResetPlan,
    pub focus: ListPreRenderFocusPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineAccountFollowupPlan<C, A, P> {
    StartSession {
        context: C,
        agent: A,
        account: Option<P>,
    },
    OpenAccountPicker(AccountPickerState<C, A, P>),
}

pub trait InlineNewSessionPickerState<C, A: AgentChoice, P> {
    fn set_inline_new_session_picker(
        &mut self,
        context: C,
        picker: AgentChoiceState<A>,
        providers: Vec<P>,
    );
}

pub fn apply_inline_new_session_picker_plan<C, A: AgentChoice, P>(
    state: &mut impl InlineNewSessionPickerState<C, A, P>,
    context: C,
    picker: AgentChoiceState<A>,
    providers: Vec<P>,
) {
    state.set_inline_new_session_picker(context, picker, providers);
}

pub trait InlineAccountPickerState<C, A, P> {
    fn set_inline_account_picker(&mut self, picker: AccountPickerState<C, A, P>);
}

pub fn apply_inline_account_picker_plan<C, A, P>(
    state: &mut impl InlineAccountPickerState<C, A, P>,
    picker: AccountPickerState<C, A, P>,
) {
    state.set_inline_account_picker(picker);
}

#[must_use]
pub const fn list_scroll_focus_plan(
    focus: Option<crate::tui::focus::MountScrollFocus>,
) -> Option<crate::tui::focus::MountScrollFocus> {
    focus
}

#[must_use]
pub const fn list_names_focus_plan(focused: bool) -> bool {
    focused
}
