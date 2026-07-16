// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace-list footer facts, mode resolver, and the matching hint-span
//! builders for the workspace-list screen.

use termrock::HintSpan;
use termrock::layout::ScrollAxes;

use crate::tui::keymap::{
    PREVIEW_PANE_KEYMAP, PreviewPaneAction, WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};
use crate::tui::screens::workspaces::model::ManagerListRow;
use termrock::layout::scroll_hint_spans;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListFooterMode {
    AgentPicker {
        scroll_axes: ScrollAxes,
    },
    RolePicker {
        scroll_axes: ScrollAxes,
    },
    PreviewPane,
    InstanceRow {
        has_snapshot: bool,
        is_live: bool,
    },
    WorkspaceRow {
        scroll_axes: ScrollAxes,
        enter_label: &'static str,
        is_saved: bool,
        show_prewarm: bool,
        show_expand: bool,
        show_collapse: bool,
        show_open_in_github: bool,
    },
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Twelve orthogonal footer-state flags (inline-agent/role-picker, \
              selected row / preview focus, snapshot+live markers, saved vs new \
              workspace, show prewarm/expand/collapse/github) — each tracks an \
              independent UI hint visibility consumed individually by the footer \
              item builder. Named-field reads match the per-hint gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub selected_instance: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
    pub show_prewarm: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Eight orthogonal footer-input flags (selected row, inline-agent/role \
              pickers, preview focus, snapshot+live markers, show_expand/collapse, \
              scroll axes, open-in-github) — each is an independent input the \
              workspace-list-footer mode resolver reads individually. Named-field \
              reads match the per-input gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterInputFacts {
    pub selected_row: ManagerListRow,
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterRowFacts {
    pub selected_instance: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
}

#[must_use]
pub const fn workspace_list_footer_row_facts(row: ManagerListRow) -> WorkspaceListFooterRowFacts {
    match row {
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceListFooterRowFacts {
                selected_instance: true,
                selected_saved_workspace: false,
                selected_new_workspace: false,
            }
        }
        ManagerListRow::SavedWorkspace(_) => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: true,
            selected_new_workspace: false,
        },
        ManagerListRow::NewWorkspace => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
        },
        ManagerListRow::CurrentDirectory => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: false,
        },
    }
}

#[must_use]
pub const fn workspace_list_open_github_visible(
    row: ManagerListRow,
    selected_workspace_has_github_mounts: bool,
) -> bool {
    matches!(row, ManagerListRow::SavedWorkspace(_)) && selected_workspace_has_github_mounts
}

#[must_use]
pub const fn workspace_list_footer_facts(
    facts: WorkspaceListFooterInputFacts,
) -> WorkspaceListFooterFacts {
    let row_facts = workspace_list_footer_row_facts(facts.selected_row);
    WorkspaceListFooterFacts {
        inline_agent_picker: facts.inline_agent_picker,
        inline_role_picker: facts.inline_role_picker,
        selected_instance: row_facts.selected_instance,
        preview_focused: facts.preview_focused,
        selected_instance_has_snapshot: facts.selected_instance_has_snapshot,
        selected_instance_is_live: facts.selected_instance_is_live,
        selected_saved_workspace: row_facts.selected_saved_workspace,
        selected_new_workspace: row_facts.selected_new_workspace,
        // Surface the `W` prewarm hint exactly when a saved workspace is
        // selected — the only row for which `W` dispatches PrewarmNamed.
        show_prewarm: row_facts.selected_saved_workspace,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        workspace_scroll_axes: facts.workspace_scroll_axes,
        show_open_in_github: facts.show_open_in_github,
    }
}

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
    crate::tui::components::confirm_hint_spans()
}

#[must_use]
pub fn create_prelude_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Dyn("Create workspace — follow the prompts".to_owned()),
        HintSpan::GroupSep,
        // UNREGISTERABLE(create-prelude-no-keymap): Esc handled inline; no dedicated create-prelude keymap.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn workspace_footer_scroll_axes(facts: WorkspaceFooterScrollFacts) -> ScrollAxes {
    if facts.inline_agent_picker || facts.inline_role_picker {
        return facts.inline_picker_scroll_axes;
    }
    if let Some(axes) = facts.focused_block_scroll_axes {
        return axes;
    }
    if facts.list_names_focused && !facts.show_expand && !facts.show_collapse {
        return facts.list_names_scroll_axes;
    }
    ScrollAxes::none()
}

#[must_use]
pub fn workspace_inline_picker_content_height(facts: WorkspaceInlinePickerContentFacts) -> usize {
    facts
        .agent_picker_count
        .or(facts.role_picker_count)
        .unwrap_or(0)
}

#[must_use]
pub fn workspace_list_footer_mode_for_facts(
    facts: WorkspaceListFooterFacts,
) -> WorkspaceListFooterMode {
    if facts.inline_agent_picker {
        return WorkspaceListFooterMode::AgentPicker {
            scroll_axes: facts.workspace_scroll_axes,
        };
    }
    if facts.inline_role_picker {
        return WorkspaceListFooterMode::RolePicker {
            scroll_axes: facts.workspace_scroll_axes,
        };
    }
    if facts.selected_instance {
        if facts.preview_focused {
            return WorkspaceListFooterMode::PreviewPane;
        }
        return WorkspaceListFooterMode::InstanceRow {
            has_snapshot: facts.selected_instance_has_snapshot,
            is_live: facts.selected_instance_is_live,
        };
    }
    WorkspaceListFooterMode::WorkspaceRow {
        scroll_axes: facts.workspace_scroll_axes,
        enter_label: if facts.selected_new_workspace {
            "setup"
        } else {
            "launch"
        },
        is_saved: facts.selected_saved_workspace,
        show_prewarm: facts.show_prewarm,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        show_open_in_github: facts.show_open_in_github,
    }
}

#[must_use]
pub fn workspace_list_footer_items(mode: WorkspaceListFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        WorkspaceListFooterMode::AgentPicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, true)
        }
        WorkspaceListFooterMode::RolePicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, true)
        }
        WorkspaceListFooterMode::PreviewPane => {
            // Glyphs derive from PREVIEW_PANE_KEYMAP — the same table that drives
            // `preview_pane_key_plan` dispatch — so advertised keys cannot drift
            // from handled keys. BackTab (HiddenAlias) and the upstream Ctrl-Q
            // are intentionally not advertised.
            let g = |a| PREVIEW_PANE_KEYMAP.glyph_for(a);
            vec![
                HintSpan::Key(g(PreviewPaneAction::NavigateUp)),
                HintSpan::Text("navigate panes"),
                HintSpan::Sep,
                HintSpan::Key(g(PreviewPaneAction::Attach)),
                HintSpan::Text("attach focused pane"),
                HintSpan::GroupSep,
                HintSpan::Key(g(PreviewPaneAction::Back)),
                HintSpan::Text("back"),
            ]
        }
        WorkspaceListFooterMode::InstanceRow {
            has_snapshot,
            is_live,
        } => {
            // Glyphs derive from WORKSPACE_LIST_KEYMAP (the dispatch table);
            // labels are instance-row-specific and supplied here.
            let g = |a| WORKSPACE_LIST_KEYMAP.glyph_for(a);
            // A failed/stopped instance has no live daemon: new-session, shell,
            // and stop are meaningless. `Enter` enters the restore ladder
            // (docker start + reconnect, or recreate from image) — that is the
            // "restart" verb — so it is labelled accordingly (D15).
            let mut items = if is_live {
                vec![
                    HintSpan::Key(g(WorkspaceListAction::NavigateUp)),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Enter)),
                    HintSpan::Text("reconnect"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::NewSession)),
                    HintSpan::Text("new session"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceShell)),
                    HintSpan::Text("shell"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceStop)),
                    HintSpan::Text("stop"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::ConfirmPurge)),
                    HintSpan::Text("purge"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceInspect)),
                    HintSpan::Text("info"),
                ]
            } else {
                vec![
                    HintSpan::Key(g(WorkspaceListAction::NavigateUp)),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Enter)),
                    HintSpan::Text("restart"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::ConfirmPurge)),
                    HintSpan::Text("delete"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceInspect)),
                    HintSpan::Text("info"),
                ]
            };
            if has_snapshot {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::EnterPreview)));
                items.push(HintSpan::Text("into preview"));
            }
            items.extend([
                HintSpan::GroupSep,
                HintSpan::Key(g(WorkspaceListAction::TreeLeft)),
                HintSpan::Text("back"),
                HintSpan::GroupSep,
                HintSpan::Key(g(WorkspaceListAction::Quit)),
                HintSpan::Text("quit"),
            ]);
            items
        }
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_axes,
            enter_label,
            is_saved,
            show_prewarm,
            show_expand,
            show_collapse,
            show_open_in_github,
        } => {
            // Glyphs derive from WORKSPACE_LIST_KEYMAP (the dispatch table);
            // labels and conditional composition are workspace-row-specific.
            let g = |a| WORKSPACE_LIST_KEYMAP.glyph_for(a);
            let mut items = Vec::new();
            if scroll_axes.any() {
                items.extend(scroll_hint_spans(scroll_axes));
                items.push(HintSpan::GroupSep);
            } else {
                items.push(HintSpan::Key(g(WorkspaceListAction::NavigateUp)));
                items.push(HintSpan::Sep);
            }
            items.extend([
                HintSpan::Key(g(WorkspaceListAction::Enter)),
                HintSpan::Text(enter_label),
                HintSpan::GroupSep,
            ]);
            if is_saved {
                items.extend([
                    HintSpan::Key(g(WorkspaceListAction::Edit)),
                    HintSpan::Text("edit"),
                    HintSpan::Sep,
                ]);
            }
            if show_prewarm {
                items.extend([
                    HintSpan::Key(g(WorkspaceListAction::Prewarm)),
                    HintSpan::Text("prewarm"),
                    HintSpan::Sep,
                ]);
            }
            items.extend([
                HintSpan::Key(g(WorkspaceListAction::NewSession)),
                HintSpan::Text("new"),
            ]);
            if is_saved {
                items.extend([
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Delete)),
                    HintSpan::Text("delete"),
                ]);
            }
            items.extend([
                HintSpan::Sep,
                HintSpan::Key(g(WorkspaceListAction::Settings)),
                HintSpan::Text("settings"),
            ]);
            if show_expand {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::TreeRight)));
                items.push(HintSpan::Text("expand"));
            }
            if show_collapse {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::TreeLeft)));
                items.push(HintSpan::Text("collapse"));
            }
            if show_open_in_github {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::OpenGithub)));
                items.push(HintSpan::Text("open in GitHub"));
            }
            items.push(HintSpan::GroupSep);
            items.push(HintSpan::Key(g(WorkspaceListAction::Quit)));
            items.push(HintSpan::Text("quit"));
            items
        }
    }
}

#[must_use]
pub fn selected_instance_snapshot_available(
    selected: ManagerListRow,
    workspace_has_snapshot: impl FnOnce(usize, usize) -> bool,
    current_dir_has_snapshot: impl FnOnce(usize) -> bool,
) -> bool {
    match selected {
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
            workspace_has_snapshot(ws_idx, inst_idx)
        }
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => current_dir_has_snapshot(inst_idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub const fn editor_save_footer_label() -> &'static str {
    "save workspace"
}

#[must_use]
pub const fn settings_save_footer_label() -> &'static str {
    "save settings"
}

#[must_use]
pub const fn pick_list_select_footer_label() -> &'static str {
    "select"
}

#[must_use]
pub const fn pick_list_confirm_footer_label() -> &'static str {
    "confirm"
}

#[must_use]
pub fn workspace_picker_footer_items(
    scroll_axes: ScrollAxes,
    include_quit: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group): combined up/down navigation display.
        HintSpan::Key("↑↓"),
        HintSpan::Sep,
        HintSpan::Key(WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Enter)),
        HintSpan::Text("launch"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(workspace-picker-no-keymap): Esc handled inline; no dedicated workspace-picker keymap.
        HintSpan::Key("Esc"),
        HintSpan::Text("return to workspaces"),
        HintSpan::GroupSep,
        HintSpan::Text("type to filter"),
    ];
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(scroll_items);
    }
    if include_quit {
        items.push(HintSpan::GroupSep);
        items.push(HintSpan::Key(
            WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit),
        ));
        items.push(HintSpan::Text("quit"));
    }
    items
}
