// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::scroll::{ScrollAxes, ScrollAxis, ScrollDelta, mouse_scroll_delta};

use super::{
    ConsoleInputDispatchPlan, ConsolePaintedInputOwner, FileBrowserState, ListModalScrollTarget,
    MOUSE_VERTICAL_SCROLL_STEP, ManagerStage, ManagerState, Modal, MouseEvent, Rect, SettingsModal,
    SettingsModalScrollTarget, SharedModalScrollTarget, modal_overlay, point_in_rect,
};

fn modal_vertical_scroll_delta(mouse: MouseEvent) -> Option<i16> {
    let ScrollDelta { axis, amount } = mouse_scroll_delta(
        mouse.kind.into(),
        mouse.modifiers.into(),
        ScrollAxes {
            vertical: true,
            horizontal: false,
        },
    )?;
    if axis != ScrollAxis::Vertical {
        return None;
    }
    Some(amount.saturating_mul(MOUSE_VERTICAL_SCROLL_STEP))
}

pub fn try_scroll_file_browser_modal(
    state: &mut ManagerState<'_>,
    owner: ConsolePaintedInputOwner,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some(delta) = modal_vertical_scroll_delta(mouse) else {
        return false;
    };
    match (owner, &mut state.stage) {
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::EditorModal),
            ManagerStage::Editor(editor),
        ) => {
            let Some(modal @ Modal::FileBrowser { .. }) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = editor.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::CreatePreludeModal),
            ManagerStage::CreatePrelude(prelude),
        ) => {
            let Some(modal @ Modal::FileBrowser { .. }) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = prelude.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::SettingsMountsModal),
            ManagerStage::Settings(settings),
        ) => {
            let area = modal_overlay::file_browser_overlay_rect(term_size);
            if let Some(SettingsModal::MountFileBrowser { state }) =
                settings.mounts.modals.current_mut()
            {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            false
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::SettingsAuthDialog),
            ManagerStage::Settings(settings),
        ) => {
            let area = modal_overlay::file_browser_overlay_rect(term_size);
            if let Some(SettingsModal::AuthSourceFolderPicker { state }) = settings.auth.modal_mut()
            {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            false
        }
        _ => false,
    }
}

pub fn scroll_file_browser_state_at(
    state: &mut FileBrowserState,
    area: Rect,
    mouse: MouseEvent,
    delta: i16,
) -> bool {
    state.scroll_selection_at(area, mouse.column, mouse.row, delta)
}

pub fn try_scroll_picker_modal(
    state: &mut ManagerState<'_>,
    owner: ConsolePaintedInputOwner,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let Some(delta) = modal_vertical_scroll_delta(mouse) else {
        return false;
    };

    if owner == ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal)
        && matches!(&state.stage, ManagerStage::List)
    {
        let Some(modal) = state.list_modal.as_ref() else {
            return false;
        };
        let area = modal.rect(term_size);
        let accepts_wheel = match modal {
            Modal::GithubPicker { state: picker } => {
                crate::tui::components::github_picker::selection_hit_at(
                    area,
                    picker,
                    ratatui::layout::Position::new(mouse.column, mouse.row),
                )
            }
            Modal::RolePicker { state: picker } => {
                crate::tui::components::role_picker::selection_hit_at(
                    area,
                    picker,
                    ratatui::layout::Position::new(mouse.column, mouse.row),
                )
            }
            Modal::OpPicker { state: picker, .. } => {
                crate::tui::components::op_picker::selection_hit_at(
                    area,
                    picker.as_ref(),
                    ratatui::layout::Position::new(mouse.column, mouse.row),
                )
            }
            _ => point_in_rect(mouse.column, mouse.row, area),
        };
        return accepts_wheel && scroll_list_modal_selection(state, delta);
    }

    match (owner, &mut state.stage) {
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::EditorModal),
            ManagerStage::Editor(editor),
        ) => {
            let Some(modal) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let accepts_wheel = match modal {
                Modal::WorkdirPick { state: picker } => {
                    crate::tui::components::workdir_pick::selection_hit_at(
                        area,
                        picker,
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                Modal::RolePicker { state: picker }
                | Modal::RoleOverridePicker { state: picker } => {
                    crate::tui::components::role_picker::selection_hit_at(
                        area,
                        picker,
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                Modal::OpPicker { state: picker, .. } => {
                    crate::tui::components::op_picker::selection_hit_at(
                        area,
                        picker.as_ref(),
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                _ => point_in_rect(mouse.column, mouse.row, area),
            };
            if !accepts_wheel {
                return false;
            }
            scroll_modal_selection(editor.modal.as_mut(), delta)
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::CreatePreludeModal),
            ManagerStage::CreatePrelude(prelude),
        ) => {
            let Some(modal) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let accepts_wheel = match modal {
                Modal::WorkdirPick { state: picker } => {
                    crate::tui::components::workdir_pick::selection_hit_at(
                        area,
                        picker,
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                Modal::RolePicker { state: picker }
                | Modal::RoleOverridePicker { state: picker } => {
                    crate::tui::components::role_picker::selection_hit_at(
                        area,
                        picker,
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                Modal::OpPicker { state: picker, .. } => {
                    crate::tui::components::op_picker::selection_hit_at(
                        area,
                        picker.as_ref(),
                        ratatui::layout::Position::new(mouse.column, mouse.row),
                    )
                }
                _ => point_in_rect(mouse.column, mouse.row, area),
            };
            if !accepts_wheel {
                return false;
            }
            scroll_modal_selection(prelude.modal.as_mut(), delta)
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::SettingsMountsModal),
            ManagerStage::Settings(settings),
        ) => {
            if let Some(modal) = settings.mounts.modals.current_mut() {
                return scroll_global_mount_modal_selection(modal, mouse, term_size, delta);
            }
            false
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::SettingsEnvDialog),
            ManagerStage::Settings(settings),
        ) => {
            if let Some(modal) = settings.env.modals.current_mut() {
                return scroll_settings_env_modal_selection(modal, mouse, term_size, delta);
            }
            false
        }
        (
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::SettingsAuthDialog),
            ManagerStage::Settings(settings),
        ) => {
            if let Some(modal) = settings.auth.modal_mut() {
                return scroll_settings_auth_modal_selection(modal, mouse, term_size, delta);
            }
            false
        }
        _ => false,
    }
}

pub fn scroll_list_modal_selection(state: &mut ManagerState<'_>, delta: i16) -> bool {
    let Some(modal) = state.list_modal.as_mut() else {
        return false;
    };
    let target = modal.list_scroll_target();
    match (target, modal) {
        (ListModalScrollTarget::GithubPicker, Modal::GithubPicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::RolePicker, Modal::RolePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::OpPicker, Modal::OpPicker { state, .. }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (ListModalScrollTarget::None, _) => false,
        _ => false,
    }
}

pub fn scroll_modal_selection(modal: Option<&mut Modal<'_>>, delta: i16) -> bool {
    let Some(modal) = modal else {
        return false;
    };
    let target = modal.shared_scroll_target();
    match (target, modal) {
        (SharedModalScrollTarget::WorkdirPick, Modal::WorkdirPick { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::RolePicker, Modal::RolePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::RolePicker, Modal::RoleOverridePicker { state }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::OpPicker, Modal::OpPicker { state, .. }) => {
            let _changed = state.scroll_selection(delta);
            true
        }
        (SharedModalScrollTarget::None, _) => false,
        _ => false,
    }
}

pub fn scroll_global_mount_modal_selection(
    modal: &mut SettingsModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.mount_scroll_target();
    match (target, modal) {
        (SettingsModalScrollTarget::MountRolePicker, SettingsModal::MountRolePicker { state }) => {
            let area = modal_overlay::role_picker_overlay_rect(term_size, state.filtered.len());
            let position = ratatui::layout::Position::new(mouse.column, mouse.row);
            if !crate::tui::components::role_picker::selection_hit_at(area, state, position) {
                return false;
            }
            let _changed = state.scroll_selection(delta);
            true
        }
        (SettingsModalScrollTarget::None, _) => false,
        _ => false,
    }
}

pub fn scroll_settings_env_modal_selection(
    modal: &mut SettingsModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.env_scroll_target();
    match (target, modal) {
        (SettingsModalScrollTarget::EnvOpPicker, SettingsModal::EnvOpPicker { state, .. }) => {
            let area = modal_overlay::op_picker_overlay_rect(term_size);
            let position = ratatui::layout::Position::new(mouse.column, mouse.row);
            if !crate::tui::components::op_picker::selection_hit_at(area, state.as_ref(), position)
            {
                return false;
            }
            let _changed = state.scroll_selection(delta);
            true
        }
        (SettingsModalScrollTarget::EnvRolePicker, SettingsModal::EnvRolePicker { state }) => {
            let area = modal_overlay::role_picker_overlay_rect(term_size, state.filtered.len());
            let position = ratatui::layout::Position::new(mouse.column, mouse.row);
            if !crate::tui::components::role_picker::selection_hit_at(area, state, position) {
                return false;
            }
            let _changed = state.scroll_selection(delta);
            true
        }
        (SettingsModalScrollTarget::None, _) => false,
        _ => false,
    }
}

pub fn scroll_settings_auth_modal_selection(
    modal: &mut SettingsModal<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    delta: i16,
) -> bool {
    let target = modal.auth_scroll_target();
    match (target, modal) {
        (SettingsModalScrollTarget::AuthOpPicker, SettingsModal::AuthOpPicker { state }) => {
            let area = modal_overlay::op_picker_overlay_rect(term_size);
            let position = ratatui::layout::Position::new(mouse.column, mouse.row);
            if !crate::tui::components::op_picker::selection_hit_at(area, state.as_ref(), position)
            {
                return false;
            }
            let _changed = state.scroll_selection(delta);
            true
        }
        (SettingsModalScrollTarget::None, _) => false,
        _ => false,
    }
}
