//! Top-level frame render: compose sidebar, main area, and footer into one `ratatui` frame.
//!
//! Not responsible for: state mutation, event handling, or individual component
//! rendering — delegates to sub-modules in `view/` and `components/`.

use ratatui::{Frame, layout::Rect};

use crate::config::AppConfig;
use crate::console::tui::components::footer::editor::editor_footer_items;
use crate::console::tui::components::footer::modal::modal_footer_items;
use crate::console::tui::components::footer::settings::settings_footer_items;
use crate::console::tui::components::footer::workspace_list_footer_items_for_state;
use crate::console::tui::components::modal::render_modal;
use crate::console::tui::components::modal_layout::modal_outer_rect;
use crate::console::tui::components::settings::{
    render_global_mount_modal, render_settings_auth_modal, render_settings_env_modal,
};
use crate::console::tui::components::workspace_list::render_list_body;
use crate::console::tui::state::{ManagerStage, ManagerState, Modal};
use jackin_console::tui::components::footer_hints::{
    create_prelude_footer_items, destructive_confirm_footer_items,
};
use jackin_console::tui::view::{
    ModalOverlayState, delete_confirm_area, footer_height, modal_overlay_visible,
    purge_confirm_area, render_footer, render_header, render_modal_backdrop, settings_error_area,
    status_overlay_area, workspace_frame_areas, workspace_header_title,
};
use jackin_tui::HintSpan;

use super::{editor, settings};

pub fn render(
    frame: &mut Frame<'_>,
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
        let areas = workspace_frame_areas(area);

        render_header(frame, areas.header, workspace_header_title());

        if matches!(&state.stage, ManagerStage::List) {
            render_list_body(frame, areas.body, state, config, cwd);
        }

        render_footer(
            frame,
            areas.footer,
            &workspace_footer_items(state, config, area),
        );
    }

    if has_modal_overlay(state) {
        // The backdrop must not cover the reserved footer — hints stay visible
        // there (the footer is inviolable).
        let footer_h = reserved_footer_height(state, config, area);
        let backdrop = Rect {
            height: area.height.saturating_sub(footer_h),
            ..area
        };
        render_modal_backdrop(frame, backdrop);
    }

    // List-anchored modal lives on `ManagerState`, not on a stage
    // variant, so the borrow splits separately from stage-anchored
    // modals.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &state.list_modal {
            render_modal(frame, modal);
        }
    } else {
        match &state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &editor.modal {
                    render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &prelude.modal {
                    render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = delete_confirm_area(area);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } => {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = purge_confirm_area(area);
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
                    let popup_area = settings_error_area(area, h);
                    jackin_tui::components::render_error_dialog(frame, popup_area, popup);
                } else if let Some(modal) = &settings.mounts.modal {
                    render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &settings.env.modal {
                    render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &settings.auth.modal {
                    render_settings_auth_modal(frame, modal);
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = status_overlay_area(area);
        jackin_tui::components::render_status_popup(frame, overlay_area, overlay);
    }
}

/// Footer hints for the workspace-style screens (list / create-prelude /
/// destructive-confirm). An open modal owns the footer: its keys replace the
/// screen keys in the reserved footer rows (hints always live in the fixed
/// footer). The exhaustive `modal_footer_items` matcher means a new modal
/// variant cannot ship without a hint — it won't compile.
fn workspace_footer_items(
    state: &ManagerState<'_>,
    config: &AppConfig,
    area: Rect,
) -> Vec<HintSpan<'static>> {
    match &state.stage {
        ManagerStage::List => state.list_modal.as_ref().map_or_else(
            || workspace_list_footer_items_for_state(state, config),
            |modal| list_modal_footer_items(modal, area),
        ),
        ManagerStage::CreatePrelude(prelude) => prelude
            .modal
            .as_ref()
            .map_or_else(create_prelude_footer_items, |modal| {
                modal_footer_items(modal, false)
            }),
        ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
            destructive_confirm_footer_items()
        }
        ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
        ManagerStage::Settings(_) => unreachable!("Settings has its own render path"),
    }
}

/// Footer for an open list-anchored modal. The Debug-info dialog is intercepted
/// here — the only place with both the modal rect and its state — so its scroll
/// keys reflect the body's actual overflow (the axis-aware footer never claims a
/// scroll direction the operator cannot move). Every other modal routes through
/// the exhaustive `modal_footer_items` matcher.
fn list_modal_footer_items(modal: &Modal<'_>, area: Rect) -> Vec<HintSpan<'static>> {
    if let Modal::ContainerInfo { state } = modal {
        let rect = modal_outer_rect(modal, area);
        let axes = jackin_tui::components::dialog_scroll_axes(
            state.content_width(),
            state.content_height(),
            rect,
        );
        return jackin_console::tui::components::footer_hints::container_info_footer_items(axes);
    }
    modal_footer_items(modal, false)
}

/// Rows the current screen reserves for its footer — excluded from the modal
/// backdrop so the hints stay visible. Editor/settings size theirs to the hint
/// content; the workspace footer is fixed.
fn reserved_footer_height(state: &ManagerState<'_>, config: &AppConfig, area: Rect) -> u16 {
    match &state.stage {
        ManagerStage::Editor(editor) => footer_height(
            &editor_footer_items(editor, config, state.op_available),
            area.width,
        ),
        ManagerStage::Settings(settings) => footer_height(
            &settings_footer_items(settings, state.op_available),
            area.width,
        ),
        _ => workspace_frame_areas(area).footer.height,
    }
}

fn has_modal_overlay(state: &ManagerState<'_>) -> bool {
    let mut overlay = ModalOverlayState {
        status_overlay: state.status_overlay.is_some(),
        ..ModalOverlayState::default()
    };
    match &state.stage {
        ManagerStage::List => overlay.list_modal = state.list_modal.is_some(),
        ManagerStage::Editor(editor) => overlay.editor_modal = editor.modal.is_some(),
        ManagerStage::Settings(settings) => {
            overlay.settings_error = settings.error_popup.is_some();
            overlay.settings_mounts_modal = settings.mounts.modal.is_some();
            overlay.settings_env_modal = settings.env.modal.is_some();
            overlay.settings_auth_modal = settings.auth.modal.is_some();
        }
        ManagerStage::CreatePrelude(prelude) => {
            overlay.create_prelude_modal = prelude.modal.is_some();
        }
        ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
            overlay.destructive_confirm = true;
        }
    }
    modal_overlay_visible(overlay)
}
