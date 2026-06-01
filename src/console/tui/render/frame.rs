use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::config::AppConfig;
use crate::console::tui::state::{ManagerListRow, ManagerStage, ManagerState};
use jackin_console::tui::components::footer_hints::{
    WorkspaceListFooterMode, workspace_list_footer_items,
};
use jackin_console::tui::view::{render_footer, render_header};
use jackin_tui::HintSpan;

use super::{centered_rect_fixed, editor, list, modal, settings};

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    if let ManagerStage::Editor(editor) = &state.stage {
        editor::render_editor(frame, area, editor, config, state.op_available);
    } else if let ManagerStage::Settings(settings) = &state.stage {
        settings::render_settings(frame, area, settings, state.op_available);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // header (brand pill + 1 spacer row)
                Constraint::Min(10),   // body
                Constraint::Length(2), // footer
            ])
            .split(area);

        render_header(frame, chunks[0], "workspaces");

        if matches!(&state.stage, ManagerStage::List) {
            list::render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<HintSpan<'static>> = match &state.stage {
            ManagerStage::List => {
                workspace_list_footer_items(workspace_list_footer_mode(state, config))
            }
            ManagerStage::CreatePrelude(_) => vec![
                HintSpan::Dyn("Create workspace — follow the prompts".to_string()),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("cancel"),
            ],
            ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
                vec![
                    HintSpan::Key("Y"),
                    HintSpan::Text("yes"),
                    HintSpan::Sep,
                    HintSpan::Key("N"),
                    HintSpan::Text("no"),
                    HintSpan::GroupSep,
                    HintSpan::Key("Esc"),
                    HintSpan::Text("cancel"),
                ]
            }
            ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
            ManagerStage::Settings(_) => unreachable!("Settings has its own render path"),
        };
        render_footer(frame, chunks[2], &footer_items);
    }

    if has_modal_overlay(state) {
        frame.render_widget(jackin_tui::components::ModalBackdrop, area);
    }

    // List-anchored modal lives on `ManagerState`, not on a stage
    // variant, so the borrow splits separately from stage-anchored
    // modals.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &prelude.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = centered_rect_fixed(area, 60, 7);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } => {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = centered_rect_fixed(area, 70, 9);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
            ManagerStage::Settings(settings) => {
                if let Some(popup) = &settings.error_popup {
                    let inner_width = (area.width * 60 / 100).saturating_sub(4);
                    let max_rows = area.height.saturating_sub(2);
                    let h = jackin_tui::components::error_dialog::required_height(
                        popup,
                        inner_width,
                        max_rows,
                    );
                    let popup_area = centered_rect_fixed(area, 60, h);
                    jackin_tui::components::render_error_dialog(frame, popup_area, popup);
                } else if let Some(modal) = &settings.mounts.modal {
                    settings::render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &settings.env.modal {
                    settings::render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &settings.auth.modal {
                    settings::render_settings_auth_modal(frame, modal);
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = centered_rect_fixed(area, 50, 7);
        jackin_tui::components::render_status_popup(frame, overlay_area, overlay);
    }
}

fn has_modal_overlay(state: &ManagerState<'_>) -> bool {
    if state.status_overlay.is_some() {
        return true;
    }
    match &state.stage {
        ManagerStage::List => state.list_modal.is_some(),
        ManagerStage::Editor(editor) => editor.modal.is_some(),
        ManagerStage::Settings(settings) => {
            settings.error_popup.is_some()
                || settings.mounts.modal.is_some()
                || settings.env.modal.is_some()
                || settings.auth.modal.is_some()
        }
        ManagerStage::CreatePrelude(prelude) => prelude.modal.is_some(),
        ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => true,
    }
}

fn workspace_list_footer_mode(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> WorkspaceListFooterMode {
    let scroll_focused = state.list_scroll_focus.is_some();
    if state.inline_agent_picker.is_some() {
        return WorkspaceListFooterMode::AgentPicker { scroll_focused };
    }
    if state.inline_role_picker.is_some() {
        return WorkspaceListFooterMode::RolePicker { scroll_focused };
    }

    let selected = state.selected_row();
    if matches!(
        selected,
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
    ) {
        if state.preview_focused {
            return WorkspaceListFooterMode::PreviewPane;
        }
        return WorkspaceListFooterMode::InstanceRow {
            has_snapshot: selected_instance_has_snapshot(state, selected),
        };
    }

    let is_saved = matches!(selected, ManagerListRow::SavedWorkspace(_));
    let show_open_in_github = is_saved
        && state
            .selected_workspace_summary()
            .and_then(|s| config.workspaces.get(&s.name))
            .is_some_and(|ws| {
                !jackin_console::github_mounts::resolve_for_workspace_from_cache(
                    ws,
                    &state.mount_info_cache,
                )
                .is_empty()
            });
    let show_expand = matches!(
        selected,
        ManagerListRow::SavedWorkspace(i)
            if !state.workspace_active_instances(i).is_empty() && !state.is_workspace_expanded(i)
    );
    let show_collapse = matches!(
        selected,
        ManagerListRow::SavedWorkspace(i) if state.is_workspace_expanded(i)
    );
    let enter_label = if matches!(selected, ManagerListRow::NewWorkspace) {
        "setup"
    } else {
        "launch"
    };

    WorkspaceListFooterMode::WorkspaceRow {
        scroll_focused,
        enter_label,
        is_saved,
        show_expand,
        show_collapse,
        show_open_in_github,
    }
}

fn selected_instance_has_snapshot(state: &ManagerState<'_>, selected: ManagerListRow) -> bool {
    match selected {
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => state
            .workspace_active_instances(ws_idx)
            .get(inst_idx)
            .copied()
            .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base)),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
            .current_dir_active_instances()
            .get(inst_idx)
            .copied()
            .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base)),
        _ => false,
    }
}
