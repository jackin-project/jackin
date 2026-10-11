// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Default)]
struct TestPreviewFocus {
    focused: bool,
    cursor: Option<(String, usize)>,
}

impl PreviewFocusState for TestPreviewFocus {
    fn set_preview_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl PreviewPaneCursorState for TestPreviewFocus {
    fn set_preview_pane_cursor(&mut self, container: &str, cursor: usize) {
        self.cursor = Some((container.to_owned(), cursor));
    }
}

#[derive(Default)]
struct TestWorkspaceListScroll {
    list_names_x: u16,
    workspace_x: u16,
    workspace_y: u16,
}

impl WorkspaceListScrollState for TestWorkspaceListScroll {
    fn list_names_scroll_x(&self) -> u16 {
        self.list_names_x
    }

    fn set_list_names_scroll_x(&mut self, value: u16) {
        self.list_names_x = value;
    }

    fn block_scroll_x(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.workspace_x,
            MountScrollFocus::Global | MountScrollFocus::RoleGlobal | MountScrollFocus::Roles => 0,
        }
    }

    fn set_block_scroll_x(&mut self, focus: MountScrollFocus, value: u16) {
        if matches!(focus, MountScrollFocus::Workspace) {
            self.workspace_x = value;
        }
    }

    fn block_scroll_y(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.workspace_y,
            MountScrollFocus::Global | MountScrollFocus::RoleGlobal | MountScrollFocus::Roles => 0,
        }
    }

    fn set_block_scroll_y(&mut self, focus: MountScrollFocus, value: u16) {
        if matches!(focus, MountScrollFocus::Workspace) {
            self.workspace_y = value;
        }
    }
}

#[derive(Default)]
struct TestTreeDisclosure {
    calls: Vec<String>,
}

impl WorkspaceTreeDisclosureState for TestTreeDisclosure {
    fn collapse_workspace(&mut self, index: usize) {
        self.calls.push(format!("collapse-workspace:{index}"));
    }

    fn collapse_current_dir(&mut self) {
        self.calls.push("collapse-current-dir".to_owned());
    }

    fn expand_workspace(&mut self, index: usize) {
        self.calls.push(format!("expand-workspace:{index}"));
    }

    fn expand_current_dir(&mut self) {
        self.calls.push("expand-current-dir".to_owned());
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Bundled 5 inline-picker clear flags carried by the test fixture — \
              each tracks one of role / agent / new_session / provider / \
              launch_account independently, and named-field reads match the \
              trait-method names this struct records."
)]
#[derive(Debug, Default, PartialEq, Eq)]
struct ClearedPickers {
    role: bool,
    agent: bool,
    new_session: bool,
    provider: bool,
    launch_account: bool,
}

#[derive(Default)]
struct TestListSelection {
    cleared: ClearedPickers,
    reset_scroll: bool,
    selected: Option<usize>,
}

impl WorkspaceListSelectionState for TestListSelection {
    fn clear_inline_role_picker(&mut self) {
        self.cleared.role = true;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.cleared.agent = true;
    }

    fn clear_inline_new_session_picker(&mut self) {
        self.cleared.new_session = true;
    }

    fn clear_inline_account_picker(&mut self) {
        self.cleared.provider = true;
    }

    fn clear_launch_account_picker(&mut self) {
        self.cleared.launch_account = true;
    }

    fn reset_list_scroll(&mut self) {
        self.reset_scroll = true;
    }

    fn set_selected(&mut self, selected: usize) {
        self.selected = Some(selected);
    }
}

#[derive(Default)]
struct TestListHover {
    target: Option<ManagerHoverTarget>,
}

impl WorkspaceListHoverState for TestListHover {
    fn set_workspace_list_hover_target(&mut self, target: Option<ManagerHoverTarget>) {
        self.target = target;
    }
}

fn mount(src: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: "/work".to_owned(),
        readonly: false,
        isolation: jackin_config::MountIsolation::default(),
    }
}

fn workspace_with_mounts(mounts: Vec<MountConfig>) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: "/work".to_owned(),
        mounts,
        ..WorkspaceConfig::default()
    }
}

fn mouse(kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
}

