// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace-list footer items and save labels.

use super::{
    WorkspaceFooterScrollFacts, WorkspaceInlinePickerContentFacts, WorkspaceListFooterFacts,
    WorkspaceListFooterMode,
};
use crate::tui::keymap::{
    PREVIEW_PANE_KEYMAP, PreviewPaneAction, WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};
use crate::tui::screens::workspaces::model::ManagerListRow;
use termrock::scroll::ScrollAxes;
use termrock::scroll::scroll_hint_spans;
use termrock::widgets::HintSpan;

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
            let mut items = vec![
                super::super::key_span(g(PreviewPaneAction::NavigateUp)),
                HintSpan::Text("navigate panes"),
                HintSpan::Sep,
                super::super::key_span(g(PreviewPaneAction::Attach)),
                HintSpan::Text("attach focused pane"),
                HintSpan::GroupSep,
                super::super::key_span(g(PreviewPaneAction::Back)),
                HintSpan::Text("back"),
            ];
            super::super::common::append_keyboard_help_hint(&mut items);
            items
        }
        WorkspaceListFooterMode::InstanceRow {
            has_snapshot,
            is_live,
        } => instance_row_footer_items(has_snapshot, is_live),
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
                items.push(super::super::key_span(g(WorkspaceListAction::NavigateUp)));
                items.push(HintSpan::Sep);
            }
            items.extend([
                super::super::key_span(g(WorkspaceListAction::Enter)),
                HintSpan::Text(enter_label),
                HintSpan::GroupSep,
            ]);
            if is_saved {
                items.extend([
                    super::super::key_span(g(WorkspaceListAction::Edit)),
                    HintSpan::Text("edit"),
                    HintSpan::Sep,
                ]);
            }
            if show_prewarm {
                items.extend([
                    super::super::key_span(g(WorkspaceListAction::Prewarm)),
                    HintSpan::Text("prewarm"),
                    HintSpan::Sep,
                ]);
            }
            items.extend([
                super::super::key_span(g(WorkspaceListAction::NewSession)),
                HintSpan::Text("new"),
            ]);
            if is_saved {
                items.extend([
                    HintSpan::Sep,
                    super::super::key_span(g(WorkspaceListAction::Delete)),
                    HintSpan::Text("delete"),
                ]);
            }
            items.extend([
                HintSpan::Sep,
                super::super::key_span(g(WorkspaceListAction::Settings)),
                HintSpan::Text("settings"),
            ]);
            if show_expand {
                items.push(HintSpan::Sep);
                items.push(super::super::key_span(g(WorkspaceListAction::TreeRight)));
                items.push(HintSpan::Text("expand"));
            }
            if show_collapse {
                items.push(HintSpan::Sep);
                items.push(super::super::key_span(g(WorkspaceListAction::TreeLeft)));
                items.push(HintSpan::Text("collapse"));
            }
            if show_open_in_github {
                items.push(HintSpan::Sep);
                items.push(super::super::key_span(g(WorkspaceListAction::OpenGithub)));
                items.push(HintSpan::Text("open in GitHub"));
            }
            items.push(HintSpan::GroupSep);
            items.push(super::super::key_span(g(WorkspaceListAction::Quit)));
            items.push(HintSpan::Text("quit"));
            super::super::common::append_keyboard_help_hint(&mut items);
            items
        }
    }
}

pub(crate) fn instance_row_footer_items(
    has_snapshot: bool,
    is_live: bool,
) -> Vec<HintSpan<'static>> {
    // Glyphs derive from WORKSPACE_LIST_KEYMAP (the dispatch table);
    // labels are instance-row-specific and supplied here.
    let g = |a| WORKSPACE_LIST_KEYMAP.glyph_for(a);
    // A failed/stopped instance has no live daemon: new-session, shell,
    // and stop are meaningless. `Enter` enters the restore ladder
    // (docker start + reconnect, or recreate from image) — that is the
    // "restart" verb — so it is labelled accordingly (D15).
    let mut items = if is_live {
        vec![
            super::super::key_span(g(WorkspaceListAction::NavigateUp)),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::Enter)),
            HintSpan::Text("reconnect"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::NewSession)),
            HintSpan::Text("new session"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::InstanceShell)),
            HintSpan::Text("shell"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::InstanceStop)),
            HintSpan::Text("stop"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::ConfirmPurge)),
            HintSpan::Text("purge"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::InstanceInspect)),
            HintSpan::Text("info"),
        ]
    } else {
        vec![
            super::super::key_span(g(WorkspaceListAction::NavigateUp)),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::Enter)),
            HintSpan::Text("restart"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::ConfirmPurge)),
            HintSpan::Text("delete"),
            HintSpan::Sep,
            super::super::key_span(g(WorkspaceListAction::InstanceInspect)),
            HintSpan::Text("info"),
        ]
    };
    if has_snapshot {
        items.push(HintSpan::Sep);
        items.push(super::super::key_span(g(WorkspaceListAction::EnterPreview)));
        items.push(HintSpan::Text("into preview"));
    }
    items.extend([
        HintSpan::GroupSep,
        super::super::key_span(g(WorkspaceListAction::TreeLeft)),
        HintSpan::Text("back"),
        HintSpan::GroupSep,
        super::super::key_span(g(WorkspaceListAction::Quit)),
        HintSpan::Text("quit"),
    ]);
    super::super::common::append_keyboard_help_hint(&mut items);
    items
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
        super::super::key_span("↑↓"),
        HintSpan::Sep,
        super::super::key_span(WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Enter)),
        HintSpan::Text("launch"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(workspace-picker-no-keymap): Esc handled inline; no dedicated workspace-picker keymap.
        super::super::key_span("Esc"),
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
        items.push(super::super::key_span(
            WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit),
        ));
        items.push(HintSpan::Text("quit"));
    }
    items
}
