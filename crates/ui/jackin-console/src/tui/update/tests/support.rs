// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[derive(Default)]
pub(super) struct TestStatusOverlay {
    pub(super) overlay: Option<crate::tui::components::StatusPopupState>,
}

impl StatusOverlayState for TestStatusOverlay {
    fn set_status_overlay(&mut self, overlay: Option<crate::tui::components::StatusPopupState>) {
        self.overlay = overlay;
    }
}

#[derive(Default)]
pub(super) struct TestListModal {
    pub(super) opened: Option<&'static str>,
}

impl ListModalState for TestListModal {
    fn open_container_info_modal(
        &mut self,
        _state: crate::tui::components::container_info_surface::ContainerInfoState,
    ) {
        self.opened = Some("container-info");
    }

    fn open_error_popup_modal(&mut self, _state: crate::tui::components::ErrorPopupState) {
        self.opened = Some("error-popup");
    }

    fn open_github_picker_modal(
        &mut self,
        _state: crate::tui::components::github_picker::GithubPickerState,
    ) {
        self.opened = Some("github-picker");
    }

    fn dismiss_list_modal(&mut self) {
        self.opened = None;
    }
}

#[derive(Default)]
pub(super) struct TestInlinePickers {
    pub(super) cleared: Vec<&'static str>,
}

impl InlinePickerDismissalState for TestInlinePickers {
    fn clear_inline_new_session_picker(&mut self) {
        self.cleared.push("new-session");
    }

    fn clear_inline_role_picker(&mut self) {
        self.cleared.push("role");
    }

    fn clear_inline_agent_picker(&mut self) {
        self.cleared.push("agent");
    }

    fn clear_inline_account_picker(&mut self) {
        self.cleared.push("provider");
    }

    fn clear_launch_account_picker(&mut self) {
        self.cleared.push("launch-provider");
    }
}

#[derive(Default)]
pub(super) struct TestListShell {
    pub(super) drag: Option<crate::tui::split::DragState>,
    pub(super) split_pct: u16,
}

impl ListShellState for TestListShell {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>) {
        self.drag = drag;
    }

    fn set_list_split_pct(&mut self, pct: u16) {
        self.split_pct = pct;
    }
}

pub(super) struct TestInlineNewSessionPicker<C, A: AgentChoice, P> {
    pub(super) picker: Option<(C, AgentChoiceState<A>, Vec<P>)>,
}

impl<C, A: AgentChoice, P> Default for TestInlineNewSessionPicker<C, A, P> {
    fn default() -> Self {
        Self { picker: None }
    }
}

impl<C, A: AgentChoice, P> InlineNewSessionPickerState<C, A, P>
    for TestInlineNewSessionPicker<C, A, P>
{
    fn set_inline_new_session_picker(
        &mut self,
        context: C,
        picker: AgentChoiceState<A>,
        providers: Vec<P>,
    ) {
        self.picker = Some((context, picker, providers));
    }
}

#[derive(Default)]
pub(super) struct TestInlineAccountPicker<C, A, P> {
    pub(super) picker: Option<AccountPickerState<C, A, P>>,
}

impl<C, A, P> InlineAccountPickerState<C, A, P> for TestInlineAccountPicker<C, A, P> {
    fn set_inline_account_picker(&mut self, picker: AccountPickerState<C, A, P>) {
        self.picker = Some(picker);
    }
}
