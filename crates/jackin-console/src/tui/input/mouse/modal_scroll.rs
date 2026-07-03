//! Per-modal scroll helpers: dispatch mouse wheel events into the
//! currently-open modal's body (file browser, picker, settings env/auth
//! tabs).

use super::{
    FileBrowserState, ListModalScrollTarget, MOUSE_VERTICAL_SCROLL_STEP, ManagerStage,
    ManagerState, Modal, ModalRectMode, MouseEvent, MouseEventKind, Rect, SettingsModal,
    SettingsModalScrollTarget, SharedModalScrollTarget, modal_rects, point_in_rect,
    scroll_selection_at_position,
};

pub fn try_scroll_file_browser_modal(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let delta = match mouse.kind {
        MouseEventKind::ScrollUp => -MOUSE_VERTICAL_SCROLL_STEP,
        MouseEventKind::ScrollDown => MOUSE_VERTICAL_SCROLL_STEP,
        _ => return false,
    };
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let Some(modal @ Modal::FileBrowser { .. }) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = editor.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        ManagerStage::CreatePrelude(prelude) => {
            let Some(modal @ Modal::FileBrowser { .. }) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            let Some(Modal::FileBrowser { state, .. }) = prelude.modal.as_mut() else {
                return false;
            };
            scroll_file_browser_state_at(state, area, mouse, delta)
        }
        ManagerStage::Settings(settings) => {
            let area = modal_rects::modal_rect_for_mode(term_size, ModalRectMode::FileBrowser);
            if let Some(SettingsModal::MountFileBrowser { state }) = settings.mounts.modal.as_mut()
            {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            if let Some(SettingsModal::AuthSourceFolderPicker { state }) = settings.auth.modal_mut()
            {
                return scroll_file_browser_state_at(state, area, mouse, delta);
            }
            false
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
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
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let delta = match mouse.kind {
        MouseEventKind::ScrollUp => -MOUSE_VERTICAL_SCROLL_STEP,
        MouseEventKind::ScrollDown => MOUSE_VERTICAL_SCROLL_STEP,
        _ => return false,
    };

    if let Some(modal) = state.list_modal.as_ref() {
        let area = modal.rect(term_size);
        if point_in_rect(mouse.column, mouse.row, area) {
            return scroll_list_modal_selection(state, delta);
        }
    }

    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let Some(modal) = editor.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            if !point_in_rect(mouse.column, mouse.row, area) {
                return false;
            }
            scroll_modal_selection(editor.modal.as_mut(), delta)
        }
        ManagerStage::CreatePrelude(prelude) => {
            let Some(modal) = prelude.modal.as_ref() else {
                return false;
            };
            let area = modal.rect(term_size);
            if !point_in_rect(mouse.column, mouse.row, area) {
                return false;
            }
            scroll_modal_selection(prelude.modal.as_mut(), delta)
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut() {
                return scroll_global_mount_modal_selection(modal, mouse, term_size, delta);
            }
            if let Some(modal) = settings.env.modal.as_mut() {
                return scroll_settings_env_modal_selection(modal, mouse, term_size, delta);
            }
            if let Some(modal) = settings.auth.modal_mut() {
                return scroll_settings_auth_modal_selection(modal, mouse, term_size, delta);
            }
            false
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => false,
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
        (SharedModalScrollTarget::RolePicker, Modal::AuthRolePicker { state }) => {
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
            let area = modal_rects::role_picker_rect_for_count(term_size, state.filtered.len());
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
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
            let area = modal_rects::op_picker_rect(term_size);
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (SettingsModalScrollTarget::EnvRolePicker, SettingsModal::EnvRolePicker { state }) => {
            let area = modal_rects::role_picker_rect_for_count(term_size, state.filtered.len());
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
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
            let area = modal_rects::op_picker_rect(term_size);
            scroll_selection_at_position(area, mouse.column, mouse.row, delta, |delta| {
                state.scroll_selection(delta)
            })
        }
        (SettingsModalScrollTarget::None, _) => false,
        _ => false,
    }
}
