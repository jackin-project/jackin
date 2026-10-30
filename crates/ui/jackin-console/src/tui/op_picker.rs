// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod load;
pub mod model;
pub mod state;

pub use load::{execute_load_request, start_load};
pub use state::{LoadResult, OpPickerState};
#[cfg(test)]
mod tests;

pub fn cli_available() -> bool {
    use jackin_env::OpRunner as _;
    jackin_env::OpCli::new_probe().probe().is_ok()
}

pub fn start_ref_validation(
    op_ref: jackin_core::OpRef,
) -> crate::tui::runtime::BlockingSubscription<anyhow::Result<()>> {
    use jackin_env::OpRunner as _;
    let runner = jackin_env::OpCli::new().with_account(op_ref.account.clone());
    let op = op_ref.op;
    crate::tui::runtime::spawn_blocking_subscription(move || runner.read(&op).map(|_| ()))
}

pub type OpPickerSelection = model::OpPickerSelection<
    jackin_core::OpRef,
    jackin_core::OpAccount,
    jackin_core::OpVault,
    jackin_core::OpItem,
    jackin_core::FieldTarget,
>;

pub fn invalidate_cache_for_ref(
    op_cache: &std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    op_ref: &jackin_core::OpRef,
) {
    let Some(parts) = jackin_core::parse_op_reference(&op_ref.op) else {
        return;
    };
    let account = op_ref.account.as_deref();
    let mut cache = op_cache.borrow_mut();
    cache.invalidate_items(account, &parts.vault);
    cache.invalidate_fields(account, &parts.vault, &parts.item);
}

pub fn poll_picker_loads(state: &mut crate::tui::state::ManagerState<'_>) -> bool {
    use crate::tui::state::{ManagerStage, Modal, SettingsModal};
    let mut dirty = false;
    if let Some(Modal::OpPicker { state, .. }) = state.list_modal.as_mut() {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Editor(editor) = &mut state.stage
        && let Some(Modal::OpPicker { state, .. }) = editor.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(SettingsModal::EnvOpPicker { state, .. }) = settings.env.modals.current_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(SettingsModal::AuthOpPicker { state }) = settings.auth.modal_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    dirty
}

fn poll_op_picker_load(state: &mut OpPickerState) -> bool {
    let mut dirty = execute_op_picker_pending_load(state);
    dirty |= state.poll_load();
    dirty |= execute_op_picker_pending_load(state);
    dirty
}

fn execute_op_picker_pending_load(state: &mut OpPickerState) -> bool {
    let Some(pending) = state.take_pending_load() else {
        return false;
    };
    let rx = start_load(
        pending.cached,
        pending.request,
        jackin_env::default_op_struct_runner(),
    );
    state.attach_load_receiver(rx);
    true
}
